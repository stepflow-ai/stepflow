#!/bin/bash
# Copyright 2025 DataStax Inc.
# Licensed under the Apache License, Version 2.0

# =============================================================================
# Stepflow PDF Ingestion Pipeline Runner
# =============================================================================
# Downloads popular ML papers from arxiv.org and processes them through
# the Stepflow document ingestion pipeline.
#
# Usage:
#   ./run-pdf-workflow.sh [options]
#
# Options:
#   --download-only    Only download PDFs, don't run workflow
#   --skip-download    Skip download, use existing PDFs
#   --parallelism N    Number of parallel workflow submissions (default: 1)
#   --pdf-dir DIR      Directory to store PDFs (default: ./arxiv-pdfs)
#   -h, --help         Show this help message
#
# Environment:
#   OPENAI_API_KEY     Required for embedding generation
#
# =============================================================================

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_ok() { echo -e "${GREEN}[OK]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Default configuration
PDF_DIR="${SCRIPT_DIR}/arxiv-pdfs"
PARALLELISM=2
DOWNLOAD_ONLY=false
SKIP_DOWNLOAD=false
FLOW_FILE="${SCRIPT_DIR}/ingestion_flow.yaml"
VARIABLES_FILE="${SCRIPT_DIR}/variables.yaml"

# Stepflow port (same as original run-workflow.sh)
STEPFLOW_PORT="${STEPFLOW_PORT:-7840}"

# Popular ML papers from arxiv (selected for size < 5MB)
# Format: "arxiv_id|filename|title"
declare -a PAPERS=(
    "1706.03762|attention-is-all-you-need.pdf|Attention Is All You Need (Transformer)"
    "1810.04805|bert.pdf|BERT: Pre-training of Deep Bidirectional Transformers"
    "1512.03385|resnet.pdf|Deep Residual Learning for Image Recognition"
    "1412.6980|adam.pdf|Adam: A Method for Stochastic Optimization"
    "1207.0580|dropout.pdf|Dropout: Improving Neural Networks by Preventing Co-adaptation"
    "1502.03167|batch-norm.pdf|Batch Normalization: Accelerating Deep Network Training"
    "1301.3781|word2vec.pdf|Efficient Estimation of Word Representations in Vector Space"
    "1406.2661|gan.pdf|Generative Adversarial Networks"
    "1409.1556|vgg.pdf|Very Deep Convolutional Networks for Large-Scale Recognition"
    "1608.06993|densenet.pdf|Densely Connected Convolutional Networks"
)

# =============================================================================
# Helper Functions
# =============================================================================

show_help() {
    head -30 "$0" | grep -E "^#" | sed 's/^# //' | sed 's/^#//'
    exit 0
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --download-only)
                DOWNLOAD_ONLY=true
                shift
                ;;
            --skip-download)
                SKIP_DOWNLOAD=true
                shift
                ;;
            --parallelism)
                PARALLELISM="$2"
                shift 2
                ;;
            --pdf-dir)
                PDF_DIR="$2"
                shift 2
                ;;
            -h|--help)
                show_help
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                ;;
        esac
    done
}

check_prerequisites() {
    local missing=()

    command -v curl >/dev/null 2>&1 || missing+=("curl")
    command -v kubectl >/dev/null 2>&1 || missing+=("kubectl")
    command -v python3 >/dev/null 2>&1 || missing+=("python3")

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        exit 1
    fi

    if [[ "$SKIP_DOWNLOAD" != "true" ]] && [[ "$DOWNLOAD_ONLY" != "true" ]]; then
        if [[ -z "${OPENAI_API_KEY:-}" ]]; then
            log_error "OPENAI_API_KEY environment variable is not set"
            exit 1
        fi
    fi

    if [[ "$DOWNLOAD_ONLY" != "true" ]]; then
        if [[ ! -f "$FLOW_FILE" ]]; then
            log_error "Flow file not found: $FLOW_FILE"
            exit 1
        fi
    fi

    log_ok "Prerequisites check passed"
}

download_papers() {
    log_info "Downloading papers to: $PDF_DIR"
    mkdir -p "$PDF_DIR"

    local downloaded=0
    local skipped=0
    local failed=0

    for paper in "${PAPERS[@]}"; do
        IFS='|' read -r arxiv_id filename title <<< "$paper"
        local pdf_path="${PDF_DIR}/${filename}"
        local pdf_url="https://arxiv.org/pdf/${arxiv_id}.pdf"

        if [[ -f "$pdf_path" ]]; then
            local size=$(stat -f%z "$pdf_path" 2>/dev/null || stat -c%s "$pdf_path" 2>/dev/null)
            if [[ $size -gt 1000 ]]; then
                log_info "  [SKIP] ${filename} (already exists, $(numfmt --to=iec-i --suffix=B $size 2>/dev/null || echo "${size} bytes"))"
                ((skipped++))
                continue
            fi
        fi

        log_info "  [GET]  ${filename} - ${title}"

        if curl -sL --fail -o "$pdf_path" "$pdf_url" 2>/dev/null; then
            local size=$(stat -f%z "$pdf_path" 2>/dev/null || stat -c%s "$pdf_path" 2>/dev/null)

            # Check if file is too large (> 5MB)
            if [[ $size -gt 5242880 ]]; then
                log_warn "         Removing ${filename} - too large ($(numfmt --to=iec-i --suffix=B $size 2>/dev/null || echo "${size} bytes"))"
                rm -f "$pdf_path"
                ((failed++))
                continue
            fi

            log_ok "         Downloaded ($(numfmt --to=iec-i --suffix=B $size 2>/dev/null || echo "${size} bytes"))"
            ((downloaded++))
        else
            log_error "         Failed to download ${filename}"
            ((failed++))
        fi

        # Be nice to arxiv
        sleep 1
    done

    echo ""
    log_info "Download summary: $downloaded downloaded, $skipped skipped, $failed failed"

    # List available PDFs
    local pdf_count=$(find "$PDF_DIR" -name "*.pdf" -type f 2>/dev/null | wc -l | tr -d ' ')
    log_info "Total PDFs available: $pdf_count"
}

setup_port_forward() {
    log_info "Setting up port-forward to stepflow-server:${STEPFLOW_PORT}..."

    # Check if port is already in use
    if lsof -i ":${STEPFLOW_PORT}" >/dev/null 2>&1; then
        log_warn "Port ${STEPFLOW_PORT} already in use, assuming existing port-forward"
        return 0
    fi

    kubectl port-forward -n stepflow svc/stepflow-server ${STEPFLOW_PORT}:7840 >/dev/null 2>&1 &
    local pf_pid=$!
    sleep 2

    if ! kill -0 $pf_pid 2>/dev/null; then
        log_error "Failed to start port-forward"
        exit 1
    fi

    log_ok "Port-forward established (PID: $pf_pid)"
}

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

    FLOW_ID=$(echo "$response" | python3 -c "import json,sys; print(json.load(sys.stdin).get('flowId',''))" 2>/dev/null || echo "")

    if [[ -z "$FLOW_ID" ]]; then
        log_error "Failed to upload flow. Response: $response"
        exit 1
    fi

    log_ok "Flow uploaded: $FLOW_ID"
}

run_workflow() {
    local pdf_url="$1"
    local index="$2"
    local total="$3"
    local filename=$(basename "$pdf_url")

    log_info "[$index/$total] Processing: $filename"

    # Read variables
    local variables_json="{}"
    if [[ -f "$VARIABLES_FILE" ]]; then
        variables_json=$(python3 -c "
import yaml
import json
import os

with open('$VARIABLES_FILE') as f:
    vars = yaml.safe_load(f) or {}

# Substitute OPENAI_API_KEY from environment
api_key = os.environ.get('OPENAI_API_KEY', '')
for key, value in vars.items():
    if value == 'OPENAI_API_KEY':
        vars[key] = api_key

print(json.dumps(vars))
")
    fi

    local start_time=$SECONDS

    # Build request body and pipe to curl via stdin
    local response
    response=$(python3 -c "
import json
print(json.dumps({
    'flowId': '$FLOW_ID',
    'input': [{'message': '$pdf_url'}],
    'variables': $variables_json
}))
" | curl -s -w "\n%{http_code}" -X POST "http://localhost:${STEPFLOW_PORT}/api/v1/runs" \
        -H "Content-Type: application/json" \
        -d @-)

    local http_code
    http_code=$(echo "$response" | tail -n1)
    local body
    body=$(echo "$response" | sed '$d')

    local elapsed=$((SECONDS - start_time))

    if [[ "$http_code" == "200" || "$http_code" == "201" ]]; then
        local run_id
        run_id=$(echo "$body" | python3 -c "import json,sys; print(json.load(sys.stdin).get('id',''))" 2>/dev/null || echo "unknown")
        log_ok "[$index/$total] Completed in ${elapsed}s - Run: $run_id"
        return 0
    else
        log_error "[$index/$total] Failed (HTTP $http_code) after ${elapsed}s"
        return 1
    fi
}

run_workflows() {
    log_info "Starting workflow execution..."
    echo ""

    # Build list of PDF URLs from arxiv
    local urls=()
    for paper in "${PAPERS[@]}"; do
        IFS='|' read -r arxiv_id filename title <<< "$paper"
        local pdf_path="${PDF_DIR}/${filename}"

        # Only include papers we have downloaded (size check)
        if [[ -f "$pdf_path" ]]; then
            local size=$(stat -f%z "$pdf_path" 2>/dev/null || stat -c%s "$pdf_path" 2>/dev/null)
            if [[ $size -gt 1000 && $size -le 5242880 ]]; then
                urls+=("https://arxiv.org/pdf/${arxiv_id}.pdf")
            fi
        fi
    done

    local total=${#urls[@]}
    if [[ $total -eq 0 ]]; then
        log_error "No PDFs available to process. Run with --download-only first."
        exit 1
    fi

    log_info "Processing $total papers with parallelism=$PARALLELISM..."
    echo ""

    local completed=0
    local failed=0
    local start_time=$SECONDS

    # Temp directory for job results
    local tmp_dir=$(mktemp -d)
    trap "rm -rf $tmp_dir" EXIT

    # Array to track background jobs
    local -a job_pids=()
    local job_count=0

    for i in "${!urls[@]}"; do
        local url="${urls[$i]}"
        local index=$((i + 1))
        local result_file="${tmp_dir}/result_${index}"

        # Run workflow in background
        (
            if run_workflow "$url" "$index" "$total"; then
                echo "success" > "$result_file"
            else
                echo "failed" > "$result_file"
            fi
        ) &

        job_pids+=($!)
        ((job_count++))

        # Wait for jobs if we've hit parallelism limit
        if [[ $job_count -ge $PARALLELISM ]]; then
            # Wait for any job to complete
            wait -n 2>/dev/null || wait "${job_pids[0]}"
            job_count=$((job_count - 1))
            # Remove completed job from array (simplified - just track count)
        fi
    done

    # Wait for remaining jobs
    wait

    # Count results
    for result_file in "$tmp_dir"/result_*; do
        if [[ -f "$result_file" ]]; then
            if [[ "$(cat "$result_file")" == "success" ]]; then
                ((completed++))
            else
                ((failed++))
            fi
        fi
    done

    local total_time=$((SECONDS - start_time))
    local avg_time=0
    if [[ $completed -gt 0 ]]; then
        avg_time=$((total_time / completed))
    fi

    echo ""
    echo "=========================================="
    echo "  Execution Summary"
    echo "=========================================="
    echo ""
    log_info "Total papers:   $total"
    log_info "Parallelism:    $PARALLELISM"
    log_ok "Completed:      $completed"
    if [[ $failed -gt 0 ]]; then
        log_error "Failed:         $failed"
    fi
    log_info "Total time:     ${total_time}s"
    log_info "Avg per paper:  ${avg_time}s"
}

# =============================================================================
# Main
# =============================================================================

main() {
    echo "=========================================="
    echo "  Stepflow PDF Ingestion Pipeline"
    echo "=========================================="
    echo ""

    parse_args "$@"
    check_prerequisites

    if [[ "$SKIP_DOWNLOAD" != "true" ]]; then
        download_papers
        echo ""
    fi

    if [[ "$DOWNLOAD_ONLY" == "true" ]]; then
        log_info "Download complete. Use --skip-download to run workflow only."
        exit 0
    fi

    setup_port_forward
    upload_flow
    echo ""

    run_workflows
}

main "$@"
