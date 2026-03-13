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

"""Document chunking component.

Wraps docling's HybridChunker to split DoclingDocument into
semantic chunks for RAG pipelines.
"""

from __future__ import annotations

import asyncio
import logging
from typing import Any

from stepflow_py.worker import StepflowContext

from docling_proto_step_worker.blob_utils import get_document_dict
from docling_proto_step_worker.exceptions import ChunkingError
from docling_proto_step_worker.metrics import chunk_tokens_total, chunks_per_document

logger = logging.getLogger(__name__)

# Default chunking options
DEFAULT_MAX_TOKENS = 512
DEFAULT_MERGE_PEERS = True


def _run_chunking(
    doc_dict: dict[str, Any], chunk_options: dict[str, Any] | None
) -> dict[str, Any]:
    """Run synchronous docling chunking.

    Args:
        doc_dict: DoclingDocument dict (from export_to_dict())
        chunk_options: Optional chunking configuration

    Returns:
        Dict with chunks list and chunk_count
    """
    from docling.chunking import HybridChunker
    from docling_core.types import DoclingDocument

    # Reconstitute DoclingDocument from dict
    doc = DoclingDocument.model_validate(doc_dict)

    # Build chunker options
    opts = chunk_options or {}
    chunker_kwargs: dict[str, Any] = {}

    tokenizer = opts.get("tokenizer")
    if tokenizer:
        chunker_kwargs["tokenizer"] = tokenizer

    max_tokens = opts.get("max_tokens", DEFAULT_MAX_TOKENS)
    chunker_kwargs["max_tokens"] = max_tokens

    merge_peers = opts.get("merge_peers", DEFAULT_MERGE_PEERS)
    chunker_kwargs["merge_peers"] = merge_peers

    chunker = HybridChunker(**chunker_kwargs)
    chunk_iter = chunker.chunk(doc)

    chunks = []
    for chunk in chunk_iter:
        chunk_data: dict[str, Any] = {
            "text": chunk.text,
            "metadata": {},
        }

        # Extract metadata from chunk
        if hasattr(chunk, "meta") and chunk.meta is not None:
            meta = chunk.meta
            if hasattr(meta, "headings") and meta.headings:
                chunk_data["metadata"]["headings"] = list(meta.headings)
            if hasattr(meta, "origin") and meta.origin:
                chunk_data["metadata"]["origin"] = {
                    "filename": getattr(meta.origin, "filename", None),
                    "mimetype": getattr(meta.origin, "mimetype", None),
                }
            if hasattr(meta, "doc_items") and meta.doc_items:
                # Extract page numbers from doc_items provenance
                page_numbers = set()
                doc_item_refs = []
                for item in meta.doc_items:
                    if hasattr(item, "self_ref"):
                        doc_item_refs.append(item.self_ref)
                    if hasattr(item, "prov") and item.prov:
                        for prov in item.prov:
                            if hasattr(prov, "page_no"):
                                page_numbers.add(prov.page_no)
                chunk_data["metadata"]["doc_items"] = doc_item_refs
                if page_numbers:
                    chunk_data["metadata"]["page"] = sorted(page_numbers)

        chunks.append(chunk_data)

    # Record chunking metrics
    chunks_per_document.record(len(chunks))
    total_tokens = sum(
        chunker.tokenizer.count_tokens(text=c["text"]) for c in chunks if c.get("text")
    )
    tokenizer_name = opts.get("tokenizer", "default")
    chunk_tokens_total.add(total_tokens, {"tokenizer": tokenizer_name})

    return {
        "chunks": chunks,
        "chunk_count": len(chunks),
    }


async def chunk_document(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Chunk a DoclingDocument using docling's HybridChunker.

    Input:
        document: dict (DoclingDocument from /docling/convert, or blob ref string)
        chunk_options: optional dict with:
            tokenizer: str (HuggingFace tokenizer name)
            max_tokens: int
            merge_peers: bool

    Output:
        chunks: list of {text, metadata: {headings, page, doc_items}}
        chunk_count: int
    """
    document = input_data.get("document")
    chunk_options = input_data.get("chunk_options")

    if document is None:
        return {"chunks": [], "chunk_count": 0}

    # If document is a string, treat as blob reference
    if isinstance(document, str):
        try:
            document = await get_document_dict(document)
        except Exception as e:
            logger.error("Failed to retrieve document from blob: %s", e)
            raise ChunkingError(f"Failed to retrieve document from blob: {e}") from e

    if not isinstance(document, dict):
        raise ChunkingError(
            f"Expected document dict or blob ref, got {type(document).__name__}"
        )

    # Check for empty document body
    body = document.get("body", {})
    children = body.get("children", [])
    if not children:
        return {"chunks": [], "chunk_count": 0}

    try:
        # Run sync chunking in thread to avoid blocking event loop
        result = await asyncio.to_thread(_run_chunking, document, chunk_options)
    except Exception as e:
        logger.error("Document chunking failed: %s", e)
        raise ChunkingError(f"Chunking failed: {e}") from e

    return result
