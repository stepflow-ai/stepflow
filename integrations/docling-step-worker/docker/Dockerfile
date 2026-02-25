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

FROM python:3.12-slim

# System deps for docling (PDF parsing, image processing)
RUN apt-get update && apt-get install -y --no-install-recommends \
    libgl1-mesa-glx libglib2.0-0 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install uv for fast dependency management
RUN pip install --no-cache-dir uv

# Copy project definition and install dependencies
COPY pyproject.toml .
RUN uv pip install --system --no-cache .

# Pre-download model artifacts to avoid first-request latency
RUN python -c "from docling.pipeline.standard_pdf_pipeline import StandardPdfPipeline; \
    StandardPdfPipeline.download_models_hf(force=True)"

COPY src/ /app/src/

ENTRYPOINT ["docling-step-worker-server"]
