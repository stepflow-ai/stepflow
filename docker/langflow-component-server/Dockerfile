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
# Note: langflow base image already has uv and a venv at /app/.venv

# Stage 1: Dependency installation (cached layer)
FROM langflowai/langflow:1.7.1 AS deps

WORKDIR /app

# Switch to root for installations
USER root

# Install uv from official image (langflow's uv is in .venv which we'll recreate)
COPY --from=ghcr.io/astral-sh/uv:0.6.14 /uv /bin/uv

# Copy dependency specification files for caching
# stepflow-py workspace
COPY sdks/python/pyproject.toml sdks/python/uv.lock ./sdks/python/
COPY sdks/python/stepflow-py/pyproject.toml ./sdks/python/stepflow-py/
COPY sdks/python/stepflow-orchestrator/pyproject.toml ./sdks/python/stepflow-orchestrator/

# stepflow-langflow integration
COPY integrations/langflow/pyproject.toml integrations/langflow/uv.lock ./integrations/langflow/

# Create minimal package structure for uv to resolve dependencies
# Note: README.md files are required by hatchling build backend
RUN mkdir -p sdks/python/stepflow-py/src/stepflow_py && \
    touch sdks/python/stepflow-py/src/stepflow_py/__init__.py && \
    touch sdks/python/stepflow-py/README.md && \
    mkdir -p sdks/python/stepflow-orchestrator/src/stepflow_orchestrator && \
    touch sdks/python/stepflow-orchestrator/src/stepflow_orchestrator/__init__.py && \
    touch sdks/python/stepflow-orchestrator/README.md && \
    mkdir -p integrations/langflow/src/stepflow_langflow_integration && \
    touch integrations/langflow/src/stepflow_langflow_integration/__init__.py && \
    touch integrations/langflow/README.md

# Install dependencies only (cached unless pyproject.toml/uv.lock change)
# Note: langflow base image venv is at /app/.venv
# UV_PROJECT_ENVIRONMENT tells uv sync to install to that venv instead of creating a new one
ENV UV_PROJECT_ENVIRONMENT=/app/.venv
# Prevent uv from downloading a managed Python - use the system Python (3.12.3)
# from the base image. Without this, uv may install Python 3.13+ into a location
# that is inaccessible when running as non-root user 1000.
ENV UV_PYTHON_PREFERENCE=only-system
RUN cd sdks/python && uv sync --frozen --no-install-workspace --no-dev --extra http
RUN cd integrations/langflow && uv sync --frozen --no-install-project --no-dev


# Stage 2: Application code (changes frequently, but deps layer is cached)
FROM deps AS runtime

# Copy full source code (--chown avoids expensive chown -R on entire /app later)
COPY --chown=1000:0 sdks/python ./sdks/python/
COPY --chown=1000:0 integrations/langflow ./integrations/langflow/

# Install packages and lfx in a single layer to minimize disk usage.
# The langflow base image /app is ~4.5GB - a blanket chown -R /app would duplicate
# all of it into a new layer. Instead, we only chown the dirs we modified.
# UV_PROJECT_ENVIRONMENT and UV_PYTHON_PREFERENCE are inherited from deps stage.
# Remove .python-version which pins Python 3.13 for development - the base image
# only has Python 3.12.3 and we must use it.
RUN rm -f sdks/python/.python-version integrations/langflow/.python-version && \
    cd sdks/python && uv sync --frozen --no-dev --extra http && \
    cd /app/integrations/langflow && uv sync --frozen --no-dev && \
    uv pip install --python /app/.venv/bin/python "lfx==0.2.1" && \
    chown -R 1000:0 /app/.venv

# Note: Don't remove source directories - uv sync creates editable installs
# that require the source code to be present

# Copy component server entry point
COPY --chown=1000:0 docker/langflow-component-server/components.py /app/

# Use existing user from langflow base image (uid 1000, name 'user')
USER 1000

# Expose HTTP port
EXPOSE 8080

# Health check - using urllib (no external deps needed)
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8080/health')" || exit 1

# Run component server in HTTP mode
CMD ["python", "components.py", "--http"]
