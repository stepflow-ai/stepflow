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

"""Application-level OTel metrics for the docling processing pipeline.

Metrics are exported via the existing MeterProvider (OTLP → OTel Collector →
Prometheus). Instruments are created at module level so they are singletons
shared across all component invocations.
"""

from opentelemetry import metrics

meter = metrics.get_meter("docling-step-worker")

documents_processed = meter.create_counter(
    "docling.documents.processed",
    description="Documents processed through conversion",
    unit="documents",
)

convert_duration = meter.create_histogram(
    "docling.convert.duration_seconds",
    description="Document conversion duration",
    unit="s",
)

classify_page_count = meter.create_histogram(
    "docling.classify.page_count",
    description="Page count per classified document",
    unit="pages",
)

chunks_per_document = meter.create_histogram(
    "docling.chunk.chunks_per_document",
    description="Chunks produced per document",
    unit="chunks",
)

chunk_tokens_total = meter.create_counter(
    "docling.chunk.tokens.total",
    description="Total tokens produced by chunking",
    unit="tokens",
)
