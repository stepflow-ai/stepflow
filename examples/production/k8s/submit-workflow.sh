#!/bin/bash
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

# submit-workflow.sh - Submit workflow with secrets injected via tweaks
#
# This script reads secrets from environment variables (injected by K8s) and
# constructs tweaks to override hardcoded values in the workflow YAML.
#
# Usage:
#   ./submit-workflow.sh '{"document_url": "https://example.com/doc.pdf"}'
#
# Required environment variables:
#   OPENSEARCH_PASSWORD - OpenSearch password
#
# Optional environment variables (with defaults):
#   STEPFLOW_SERVER_URL - Stepflow server URL (default: http://stepflow-server:7840/api/v1)
#   OPENSEARCH_URL      - OpenSearch URL (default: http://opensearch.stepflow.svc.cluster.local:9200)
#   OPENSEARCH_USERNAME - OpenSearch username (default: admin)
#   WORKFLOW_PATH       - Path to workflow YAML (default: /workflows/openrag_ingestion_flow.yaml)

set -e

# Configuration with defaults
STEPFLOW_SERVER_URL="${STEPFLOW_SERVER_URL:-http://stepflow-server:7840/api/v1}"
OPENSEARCH_URL="${OPENSEARCH_URL:-http://opensearch.stepflow.svc.cluster.local:9200}"
OPENSEARCH_USERNAME="${OPENSEARCH_USERNAME:-admin}"
WORKFLOW_PATH="${WORKFLOW_PATH:-/workflows/openrag_ingestion_flow.yaml}"

# Validate required environment variables
if [[ -z "${OPENSEARCH_PASSWORD}" ]]; then
    echo "Error: OPENSEARCH_PASSWORD environment variable is required" >&2
    exit 1
fi

# Get input JSON from first argument or default to empty object
INPUT_JSON="${1:-{}}"

# Build tweaks JSON from environment variables
# These override hardcoded values in the exported Langflow workflow
TWEAKS=$(cat <<EOF
{
  "OpenSearchVectorStoreComponentMultimodalMultiEmbedding-By9U4": {
    "opensearch_url": "${OPENSEARCH_URL}",
    "username": "${OPENSEARCH_USERNAME}",
    "password": "${OPENSEARCH_PASSWORD}"
  }
}
EOF
)

echo "Submitting workflow to ${STEPFLOW_SERVER_URL}..."
echo "Workflow: ${WORKFLOW_PATH}"
echo "Input: ${INPUT_JSON}"

# Submit with tweaks
stepflow submit \
  --url "${STEPFLOW_SERVER_URL}" \
  --flow "${WORKFLOW_PATH}" \
  --tweaks "${TWEAKS}" \
  --input-json "${INPUT_JSON}"
