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

# Dockerfile for Stepflow Docling Worker
# Component server for document processing via docling-serve sidecar
# Build context: repository root (stepflow/)
#
# This image runs alongside docling-serve in the same pod (sidecar pattern).
# It provides Stepflow component endpoints that route to the local docling-serve.

# Stage 1: Build dependencies
FROM python:3.11-slim AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc \
    && rm -rf /var/lib/apt/lists/*

# Install uv for fast package management
RUN pip install --no-cache-dir uv

# Copy dependency files first for better layer caching
COPY sdks/python/stepflow-py/pyproject.toml /tmp/stepflow-py/pyproject.toml
COPY integrations/docling/pyproject.toml /tmp/stepflow-docling/pyproject.toml

# Create virtual environment
RUN python -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Install core dependencies (stepflow-py + stepflow-docling deps)
RUN uv pip install --python /opt/venv/bin/python \
    # stepflow-docling deps
    httpx>=0.25.0 \
    click>=8.0 \
    python-dotenv>=1.0.0 \
    nest-asyncio>=1.6.0 \
    # stepflow-py core deps
    msgspec>=0.19.0 \
    jsonschema>=4.17.0 \
    pydantic>=2.0.0 \
    aiohttp>=3.9.0 \
    aiohttp-retry>=2.8.0 \
    python-dateutil>=2.8.2 \
    typing-extensions>=4.7.1 \
    urllib3>=2.0.0 \
    # stepflow-py http deps
    fastapi>=0.104.1 \
    uvicorn>=0.24.0 \
    sse-starlette>=1.6.5 \
    # opentelemetry deps
    opentelemetry-api>=1.20.0 \
    opentelemetry-sdk>=1.20.0 \
    opentelemetry-exporter-otlp-proto-grpc>=1.20.0 \
    opentelemetry-instrumentation-httpx>=0.48b0


# Stage 2: Runtime image
FROM python:3.11-slim AS runtime

WORKDIR /app

# Copy virtual environment from builder
COPY --from=builder /opt/venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Install uv for package installation
RUN pip install --no-cache-dir uv

# Copy stepflow-py SDK source
COPY sdks/python/stepflow-py /tmp/stepflow-py

# Copy docling integration source
COPY integrations/docling /tmp/stepflow-docling

# Install stepflow-py from local copy with HTTP support (no deps - already installed)
RUN uv pip install --python /opt/venv/bin/python --no-deps /tmp/stepflow-py[http]

# Install stepflow-docling (no deps - already installed)
RUN uv pip install --python /opt/venv/bin/python --no-deps /tmp/stepflow-docling

# Clean up source copies
RUN rm -rf /tmp/stepflow-py /tmp/stepflow-docling

# Create non-root user
RUN useradd --create-home --uid 1000 appuser
RUN chown -R appuser:appuser /app
USER appuser

# Environment variables for docling-serve sidecar
# In K8s, docling-serve runs in the same pod on localhost:5001
ENV DOCLING_SERVE_URL="http://localhost:5001"

# Expose HTTP port for Stepflow component server
EXPOSE 8080

# Health check - verify server is responding
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8080/health')" || exit 1

# Run stepflow-docling server
CMD ["stepflow-docling", "serve", "--host", "0.0.0.0", "--port", "8080"]
