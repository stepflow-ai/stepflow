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

# Stage 1: Dependency installation (cached layer)
# Dependencies are installed from uv.lock files for reproducibility
FROM python:3.11-slim AS deps

WORKDIR /app

# Install uv from official image (pinned version for reproducibility)
COPY --from=ghcr.io/astral-sh/uv:0.6.14 /uv /bin/uv

# Install build dependencies (needed for some packages)
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc \
    && rm -rf /var/lib/apt/lists/*

# Copy dependency specification files for caching
# stepflow-py workspace (includes stepflow-py and stepflow-orchestrator)
COPY sdks/python/pyproject.toml sdks/python/uv.lock ./sdks/python/
COPY sdks/python/stepflow-py/pyproject.toml ./sdks/python/stepflow-py/
COPY sdks/python/stepflow-orchestrator/pyproject.toml ./sdks/python/stepflow-orchestrator/

# stepflow-docling
COPY integrations/docling/pyproject.toml integrations/docling/uv.lock ./integrations/docling/

# Create minimal package structure for uv to resolve dependencies
# Note: README.md files are required by hatchling build backend
RUN mkdir -p sdks/python/stepflow-py/src/stepflow_py && \
    touch sdks/python/stepflow-py/src/stepflow_py/__init__.py && \
    touch sdks/python/stepflow-py/README.md && \
    mkdir -p sdks/python/stepflow-orchestrator/src/stepflow_orchestrator && \
    touch sdks/python/stepflow-orchestrator/src/stepflow_orchestrator/__init__.py && \
    touch sdks/python/stepflow-orchestrator/README.md && \
    mkdir -p integrations/docling/src/stepflow_docling && \
    touch integrations/docling/src/stepflow_docling/__init__.py && \
    touch integrations/docling/README.md

# Install dependencies only (cached unless pyproject.toml/uv.lock change)
# Use --frozen to ensure lock file is used exactly as-is
RUN cd sdks/python && uv sync --frozen --no-install-workspace --no-dev --extra http
RUN cd integrations/docling && uv sync --frozen --no-install-project --no-dev


# Stage 2: Application code (changes frequently, but deps layer is cached)
FROM deps AS builder

# Copy full source code
COPY sdks/python ./sdks/python/
COPY integrations/docling ./integrations/docling/

# Install the actual packages (deps already installed)
RUN cd sdks/python && uv sync --frozen --no-dev --extra http
RUN cd integrations/docling && uv sync --frozen --no-dev


# Stage 3: Minimal runtime image (no build tools)
FROM python:3.11-slim AS runtime

WORKDIR /app

# Copy virtual environments from builder
COPY --from=builder /app/sdks/python/.venv ./sdks/python/.venv
COPY --from=builder /app/integrations/docling/.venv ./integrations/docling/.venv

# Add both venvs to PATH (docling venv takes precedence)
ENV PATH="/app/integrations/docling/.venv/bin:/app/sdks/python/.venv/bin:$PATH"

# Create non-root user
RUN useradd --create-home --uid 1000 appuser \
    && chown -R appuser:appuser /app
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
