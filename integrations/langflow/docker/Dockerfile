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

# Langflow gRPC pull-based worker image
#
# Build from repo root:
#   docker build --load -f integrations/langflow/docker/Dockerfile \
#     -t localhost/langflow-worker:latest .
#
# Load into Kind:
#   kind load docker-image localhost/langflow-worker:latest --name stepflow

FROM python:3.12-slim

USER 0

# Install uv for fast dependency resolution (pip backtracking on langflow's
# deep dependency tree takes 20+ minutes; uv resolves in seconds)
COPY --from=ghcr.io/astral-sh/uv:0.6.14 /uv /bin/uv

# System dependencies for langflow and its ML libraries
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install local stepflow SDKs
COPY sdks/python/stepflow-py/ /tmp/stepflow-py/
COPY sdks/python/stepflow-orchestrator/ /tmp/stepflow-orchestrator/

# Remove .python-version files that may conflict with base image Python
RUN rm -f /tmp/stepflow-py/.python-version \
          /tmp/stepflow-orchestrator/.python-version

RUN uv pip install --system --no-cache \
    "/tmp/stepflow-py" \
    "/tmp/stepflow-orchestrator" \
    && rm -rf /tmp/stepflow-py /tmp/stepflow-orchestrator

# Install langflow integration package (brings in langflow, lfx, all deps)
COPY integrations/langflow/ /tmp/langflow-integration/
# Strip [tool.uv.sources] from pyproject.toml — the editable path overrides
# (../../sdks/...) don't exist in the image; the SDKs are already installed
# as system packages from the previous step.
RUN rm -f /tmp/langflow-integration/.python-version && \
    sed -i '/^\[tool\.uv\.sources\]/,/^\[/{ /^\[tool\.uv\.sources\]/d; /^\[/!d; }' \
        /tmp/langflow-integration/pyproject.toml && \
    uv pip install --system --no-cache "/tmp/langflow-integration" && \
    rm -rf /tmp/langflow-integration

WORKDIR /app

# Create non-root user matching K8s pod securityContext (UID 1000)
RUN useradd -m -u 1000 stepflow
USER 1000

# gRPC worker is a client — no ports exposed
ENTRYPOINT ["stepflow-langflow-server"]
