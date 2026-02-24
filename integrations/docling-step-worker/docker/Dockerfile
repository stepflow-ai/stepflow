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
