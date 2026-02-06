#!/usr/bin/env bash
# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

# Stepflow K8s Workflow Runner
# Processes Wikipedia articles through the ingestion pipeline
#
# Usage:
#   ./run-workflow.sh                           # Use default wikipedia-ai-articles.jsonl
#   ./run-workflow.sh urls.txt                  # One URL per line
#   ./run-workflow.sh articles.jsonl            # JSONL format: {"message": "url"}
#   ./run-workflow.sh --parallel 3 urls.txt    # Run 3 concurrent workflows
#
# Environment:
#   OPENAI_API_KEY - Required for embedding steps
#   STEPFLOW_PORT  - Server port (default: 7840)
#   KUBECONFIG     - Kubernetes config (default: ~/.kube/config)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STEPFLOW_PORT="${STEPFLOW_PORT:-7840}"
PARALLEL="${PARALLEL:-1}"
FLOW_FILE="${SCRIPT_DIR}/ingestion_flow.yaml"
VARIABLES_FILE="${SCRIPT_DIR}/variables.yaml"
DEFAULT_URLS_FILE="${SCRIPT_DIR}/wikipedia-ai-articles.jsonl"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
TOTAL=0
COMPLETED=0
FAILED=0
SKIPPED=0

log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[OK]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS] [URL_FILE]

Process Wikipedia articles through the Stepflow ingestion pipeline.

Options:
    -p, --parallel N    Run N workflows concurrently (default: 1)
    -f, --flow FILE     Use custom flow file (default: ingestion_flow.yaml)
    -v, --vars FILE     Use custom variables file (default: variables.yaml)
    --dry-run           Show what would be done without executing
    -h, --help          Show this help message

Arguments:
    URL_FILE            File containing URLs. Formats supported:
                        - Plain text: one URL per line
                        - JSONL: {"message": "url"} per line
                        Default: wikipedia-ai-articles.jsonl

Environment Variables:
    OPENAI_API_KEY      Required for embedding model steps
    STEPFLOW_PORT       Server port (default: 7840)

Examples:
    ./run-workflow.sh
    ./run-workflow.sh my-urls.txt
    ./run-workflow.sh --parallel 3 wikipedia-ai-articles.jsonl
EOF
    exit 0
}

# Parse arguments
DRY_RUN=false
POSITIONAL_ARGS=()

while [[ $# -gt 0 ]]; do
    case $1 in
        -p|--parallel)
            PARALLEL="$2"
            shift 2
            ;;
        -f|--flow)
            FLOW_FILE="$2"
            shift 2
            ;;
        -v|--vars)
            VARIABLES_FILE="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        -*)
            log_error "Unknown option: $1"
            usage
            ;;
        *)
            POSITIONAL_ARGS+=("$1")
            shift
            ;;
    esac
done

# Set URL file from positional args
URL_FILE="${POSITIONAL_ARGS[0]:-$DEFAULT_URLS_FILE}"

# Validate inputs
check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check for required tools
    for cmd in kubectl curl python3 jq; do
        if ! command -v "$cmd" &> /dev/null; then
            log_error "Required command not found: $cmd"
            exit 1
        fi
    done

    # Check for OPENAI_API_KEY
    if [[ -z "${OPENAI_API_KEY:-}" ]]; then
        log_error "OPENAI_API_KEY environment variable is required"
        exit 1
    fi

    # Check files exist
    if [[ ! -f "$FLOW_FILE" ]]; then
        log_error "Flow file not found: $FLOW_FILE"
        exit 1
    fi

    if [[ ! -f "$VARIABLES_FILE" ]]; then
        log_error "Variables file not found: $VARIABLES_FILE"
        exit 1
    fi

    if [[ ! -f "$URL_FILE" ]]; then
        log_error "URL file not found: $URL_FILE"
        exit 1
    fi

    # Check kubectl context
    if ! kubectl cluster-info &> /dev/null; then
        log_error "Cannot connect to Kubernetes cluster. Check your KUBECONFIG."
        exit 1
    fi

    log_success "Prerequisites check passed"
}

# Port forwarding management
PF_PID=""

start_port_forward() {
    log_info "Setting up port-forward to stepflow-server:${STEPFLOW_PORT}..."

    # Check if port is already in use
    if lsof -i ":${STEPFLOW_PORT}" &> /dev/null; then
        log_warn "Port ${STEPFLOW_PORT} already in use, assuming existing port-forward"
        return 0
    fi

    # Start port-forward in background
    kubectl port-forward -n stepflow svc/stepflow-server "${STEPFLOW_PORT}:7840" &> /dev/null &
    PF_PID=$!

    # Wait for port-forward to be ready
    local max_wait=30
    local waited=0
    while ! curl -s "http://localhost:${STEPFLOW_PORT}/api/v1/health" &> /dev/null; do
        sleep 1
        ((waited++))
        if [[ $waited -ge $max_wait ]]; then
            log_error "Port-forward failed to become ready after ${max_wait}s"
            cleanup
            exit 1
        fi
    done

    log_success "Port-forward ready on localhost:${STEPFLOW_PORT}"
}

cleanup() {
    if [[ -n "$PF_PID" ]] && kill -0 "$PF_PID" 2>/dev/null; then
        log_info "Stopping port-forward (PID: $PF_PID)"
        kill "$PF_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT

# Upload flow and get flow ID
FLOW_ID=""

upload_flow() {
    log_info "Uploading workflow to stepflow-server..."

    local response
    response=$(python3 -c "
import json, yaml, sys
try:
    with open('$FLOW_FILE') as f:
        flow = yaml.safe_load(f)
    print(json.dumps({'flow': flow}))
except Exception as e:
    print(f'Error: {e}', file=sys.stderr)
    sys.exit(1)
" | curl -s -X POST "http://localhost:${STEPFLOW_PORT}/api/v1/flows" \
        -H "Content-Type: application/json" \
        -d @-)

    FLOW_ID=$(echo "$response" | jq -r '.flowId // empty')

    if [[ -z "$FLOW_ID" ]]; then
        log_error "Failed to upload flow. Response: $response"
        exit 1
    fi

    log_success "Flow uploaded: $FLOW_ID"
}

# Load variables with OPENAI_API_KEY injected
get_variables_json() {
    python3 -c "
import json, yaml, os
with open('$VARIABLES_FILE') as f:
    variables = yaml.safe_load(f) or {}

# Inject runtime variables
variables['OPENAI_API_KEY'] = os.environ.get('OPENAI_API_KEY', '')
variables['OPENSEARCH_URL'] = 'http://opensearch.stepflow.svc.cluster.local:9200'
variables['DOCLING_SERVE_URL'] = 'http://docling-serve.stepflow.svc.cluster.local:5001'
variables['SELECTED_EMBEDDING_MODEL'] = 'text-embedding-3-small'

print(json.dumps(variables))
"
}

# Parse URL file and extract URLs
parse_urls() {
    local file="$1"

    if [[ "$file" == *.jsonl ]]; then
        # JSONL format
        jq -r '.message' "$file" 2>/dev/null || cat "$file"
    else
        # Plain text, one URL per line
        grep -E '^https?://' "$file" || cat "$file"
    fi
}

# Run a single workflow
run_workflow() {
    local url="$1"
    local index="$2"
    local total="$3"

    log_info "[$index/$total] Processing: $url"

    local variables_json
    variables_json=$(get_variables_json)

    local start_time
    start_time=$(date +%s)

    local response
    local http_code

    # Build request body and pipe to curl via stdin to avoid exposing secrets in process table
    response=$(python3 -c "
import json
print(json.dumps({
    'flowId': '$FLOW_ID',
    'input': [{'message': '$url'}],
    'variables': $variables_json
}))
" | curl -s -w "\n%{http_code}" -X POST "http://localhost:${STEPFLOW_PORT}/api/v1/runs" \
        -H "Content-Type: application/json" \
        -d @-)

    http_code=$(echo "$response" | tail -n1)
    response=$(echo "$response" | sed '$d')

    local end_time
    end_time=$(date +%s)
    local duration=$((end_time - start_time))

    local run_id status
    run_id=$(echo "$response" | jq -r '.runId // empty')
    status=$(echo "$response" | jq -r '.status // "unknown"')

    if [[ "$status" == "completed" ]]; then
        log_success "[$index/$total] Completed in ${duration}s - Run: $run_id"
        ((COMPLETED++))
        return 0
    elif [[ "$status" == "failed" ]]; then
        local error_msg
        error_msg=$(echo "$response" | jq -r '.result.error.message // .error // "Unknown error"')
        log_error "[$index/$total] Failed (${duration}s) - $error_msg"
        ((FAILED++))
        return 1
    else
        log_warn "[$index/$total] Unknown status: $status (HTTP $http_code)"
        log_warn "Response: $response"
        ((FAILED++))
        return 1
    fi
}

# Main execution
main() {
    echo ""
    echo "=========================================="
    echo "  Stepflow Wikipedia Ingestion Pipeline"
    echo "=========================================="
    echo ""

    check_prerequisites

    # Read URLs into array (compatible with bash 3.x on macOS)
    URLS=()
    while IFS= read -r url; do
        [[ -n "$url" ]] && URLS+=("$url")
    done < <(parse_urls "$URL_FILE")
    TOTAL=${#URLS[@]}

    if [[ $TOTAL -eq 0 ]]; then
        log_error "No URLs found in $URL_FILE"
        exit 1
    fi

    log_info "Found $TOTAL URLs to process"
    log_info "Parallelism: $PARALLEL"
    log_info "Flow file: $FLOW_FILE"
    log_info "Variables file: $VARIABLES_FILE"
    echo ""

    if [[ "$DRY_RUN" == "true" ]]; then
        log_warn "Dry run mode - showing URLs that would be processed:"
        for url in "${URLS[@]}"; do
            echo "  - $url"
        done
        exit 0
    fi

    start_port_forward
    upload_flow

    echo ""
    log_info "Starting workflow execution..."
    echo ""

    local start_time
    start_time=$(date +%s)

    if [[ $PARALLEL -eq 1 ]]; then
        # Sequential execution
        local index=0
        for url in "${URLS[@]}"; do
            ((index++))
            run_workflow "$url" "$index" "$TOTAL" || true
        done
    else
        # Parallel execution using GNU parallel or background jobs
        if command -v parallel &> /dev/null; then
            # Use GNU parallel if available
            export -f run_workflow log_info log_success log_error log_warn get_variables_json
            export FLOW_ID STEPFLOW_PORT VARIABLES_FILE OPENAI_API_KEY
            export RED GREEN YELLOW BLUE NC
            export COMPLETED FAILED

            printf '%s\n' "${URLS[@]}" | parallel -j "$PARALLEL" --line-buffer \
                "run_workflow {} {#} $TOTAL"
        else
            # Fallback: simple background job control
            local index=0
            local running=0
            local pids=()

            for url in "${URLS[@]}"; do
                ((index++))

                # Run in background
                run_workflow "$url" "$index" "$TOTAL" &
                pids+=($!)
                ((running++))

                # Wait if we've hit parallel limit
                if [[ $running -ge $PARALLEL ]]; then
                    wait "${pids[0]}"
                    pids=("${pids[@]:1}")
                    ((running--))
                fi
            done

            # Wait for remaining jobs
            for pid in "${pids[@]}"; do
                wait "$pid" || true
            done
        fi
    fi

    local end_time
    end_time=$(date +%s)
    local total_duration=$((end_time - start_time))

    echo ""
    echo "=========================================="
    echo "  Execution Summary"
    echo "=========================================="
    echo ""
    log_info "Total URLs:     $TOTAL"
    log_success "Completed:      $COMPLETED"
    [[ $FAILED -gt 0 ]] && log_error "Failed:         $FAILED"
    [[ $SKIPPED -gt 0 ]] && log_warn "Skipped:        $SKIPPED"
    log_info "Total time:     ${total_duration}s"

    if [[ $TOTAL -gt 0 ]]; then
        local avg=$((total_duration / TOTAL))
        log_info "Avg per URL:    ${avg}s"
    fi
    echo ""

    # Exit with error if any failures
    [[ $FAILED -gt 0 ]] && exit 1
    exit 0
}

main "$@"
