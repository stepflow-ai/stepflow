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

# Dockerfile for Langflow Component Server
# Provides HTTP-based component execution for Stepflow workflows
# Build context: repository root (stepflow/)
#
# Uses langflowai/langflow:1.7.1 as base to match project dependencies

# Stage 1: Dependency installation (cached layer)
FROM langflowai/langflow:1.7.1 AS deps

WORKDIR /app

# Switch to root for installations
USER root

# Copy dependency files first for better layer caching
COPY sdks/python/stepflow-py/pyproject.toml /tmp/stepflow-py/pyproject.toml
COPY integrations/langflow/pyproject.toml /tmp/stepflow-langflow/pyproject.toml

# Install dependencies only (no source code yet)
# This layer is cached unless pyproject.toml files change
RUN uv pip install --python /app/.venv/bin/python \
    msgspec>=0.19.0 \
    jsonschema>=4.17.0 \
    opentelemetry-api>=1.20.0 \
    opentelemetry-sdk>=1.20.0 \
    opentelemetry-exporter-otlp-proto-grpc>=1.20.0 \
    fastapi>=0.104.1 \
    uvicorn>=0.24.0 \
    sse-starlette>=1.6.5 \
    pyyaml>=6.0 \
    click>=8.0 \
    pydantic>=2.0.0 \
    pandas>=2.0.0 \
    python-dotenv>=1.0.0 \
    openai>=1.0.0 \
    langchain_core>=0.1.0 \
    langchain_openai>=0.1.0 \
    nest-asyncio>=1.6.0 \
    opensearch-py>=2.8.0


# Stage 2: Application code (changes frequently)
FROM deps AS runtime

# Copy stepflow-py SDK source
COPY sdks/python/stepflow-py /tmp/stepflow-py

# Copy langflow integration source
COPY integrations/langflow /tmp/stepflow-langflow

# Install stepflow-py from local copy with HTTP support (no deps - already installed)
RUN uv pip install --python /app/.venv/bin/python --no-deps /tmp/stepflow-py[http]

# Install lfx (required by langflow components)
RUN uv pip install --python /app/.venv/bin/python "lfx==0.2.1"

# Install langflow integration (skip deps - already installed above)
RUN uv pip install --python /app/.venv/bin/python --no-deps /tmp/stepflow-langflow

# Copy component server entry point
COPY docker/langflow-component-server/components.py /app/

# Use existing user from langflow base image (uid 1000, name 'user')
RUN chown -R 1000:0 /app

USER 1000

# Expose HTTP port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD python -c "import requests; requests.get('http://localhost:8080/health')" || exit 1

# Run component server in HTTP mode
CMD ["python", "components.py", "--http"]
