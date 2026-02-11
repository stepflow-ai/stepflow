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

# Copy dependency specification files for caching
# stepflow-py workspace
COPY sdks/python/pyproject.toml sdks/python/uv.lock ./sdks/python/
COPY sdks/python/stepflow-py/pyproject.toml ./sdks/python/stepflow-py/
COPY sdks/python/stepflow-orchestrator/pyproject.toml ./sdks/python/stepflow-orchestrator/

# stepflow-langflow integration
COPY integrations/langflow/pyproject.toml integrations/langflow/uv.lock ./integrations/langflow/

# Create minimal package structure for uv to resolve dependencies
RUN mkdir -p sdks/python/stepflow-py/src/stepflow_py && \
    touch sdks/python/stepflow-py/src/stepflow_py/__init__.py && \
    mkdir -p sdks/python/stepflow-orchestrator/src/stepflow_orchestrator && \
    touch sdks/python/stepflow-orchestrator/src/stepflow_orchestrator/__init__.py && \
    mkdir -p integrations/langflow/src/stepflow_langflow_integration && \
    touch integrations/langflow/src/stepflow_langflow_integration/__init__.py

# Install dependencies only (cached unless pyproject.toml/uv.lock change)
# Note: langflow base image venv is at /app/.venv, use --python to target it
RUN cd sdks/python && uv sync --frozen --no-install-workspace --no-dev --extra http --python /app/.venv/bin/python
RUN cd integrations/langflow && uv sync --frozen --no-install-project --no-dev --python /app/.venv/bin/python


# Stage 2: Application code (changes frequently, but deps layer is cached)
FROM deps AS runtime

# Copy full source code
COPY sdks/python ./sdks/python/
COPY integrations/langflow ./integrations/langflow/

# Install the actual packages (deps already installed)
RUN cd sdks/python && uv sync --frozen --no-dev --extra http --python /app/.venv/bin/python
RUN cd integrations/langflow && uv sync --frozen --no-dev --python /app/.venv/bin/python

# Install lfx (required by langflow components, not in lock file)
RUN uv pip install --python /app/.venv/bin/python "lfx==0.2.1"

# Clean up source copies to reduce image size
RUN rm -rf sdks/python integrations/langflow

# Copy component server entry point
COPY docker/langflow-component-server/components.py /app/

# Use existing user from langflow base image (uid 1000, name 'user')
RUN chown -R 1000:0 /app

USER 1000

# Expose HTTP port
EXPOSE 8080

# Health check - using urllib (no external deps needed)
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8080/health')" || exit 1

# Run component server in HTTP mode
CMD ["python", "components.py", "--http"]
