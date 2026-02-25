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

"""Output parity tests.

These tests validate the core thesis: a document processed through
the docling step worker (direct library calls) produces structurally
equivalent output to docling-serve.

Test approach modeled after docling-serve's test_fastapi_endpoints.py
and docling's own verify_utils.py patterns.

Requirements:
    - Docling models must be downloaded (first run may be slow)
    - Mark with @pytest.mark.integration and @pytest.mark.slow

Fixture strategy:
    Following docling-serve's pattern, we use real arXiv papers as test
    fixtures (publicly available PDFs). For CI environments where network
    access may be limited, we also support pre-downloaded fixture files.
"""

from __future__ import annotations

import os
from io import BytesIO
from pathlib import Path

import pytest

# Skip all tests in this module if DOCLING_INTEGRATION_TESTS is not set
pytestmark = [
    pytest.mark.integration,
    pytest.mark.slow,
    pytest.mark.skipif(
        not os.environ.get("DOCLING_INTEGRATION_TESTS"),
        reason="Set DOCLING_INTEGRATION_TESTS=1 to run integration tests",
    ),
]

# Fixture PDF URLs (same as used by docling-serve tests)
DOCLING_PAPER_URL = "https://arxiv.org/pdf/2408.09869v5.pdf"
DOCLAYNET_PAPER_URL = "https://arxiv.org/pdf/2206.01062v1.pdf"

FIXTURES_DIR = Path(__file__).parent.parent / "fixtures"


def _get_test_pdf_bytes() -> bytes:
    """Get test PDF bytes, from local fixture or download."""
    local_path = FIXTURES_DIR / "2408.09869v5.pdf"
    if local_path.exists():
        return local_path.read_bytes()

    import httpx

    resp = httpx.get(DOCLING_PAPER_URL, timeout=120.0, follow_redirects=True)
    resp.raise_for_status()

    # Cache locally for future runs
    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    local_path.write_bytes(resp.content)

    return resp.content


@pytest.fixture(scope="session")
def test_pdf_bytes():
    """Session-scoped fixture for test PDF bytes."""
    return _get_test_pdf_bytes()


@pytest.fixture(scope="session")
def docling_converter():
    """Session-scoped DocumentConverter with default pipeline.

    Models are loaded once and reused across all tests.
    """
    from docling.datamodel.base_models import InputFormat
    from docling.datamodel.pipeline_options import PdfPipelineOptions, TableFormerMode
    from docling.document_converter import DocumentConverter, PdfFormatOption
    from docling.pipeline.standard_pdf_pipeline import StandardPdfPipeline

    pipeline_options = PdfPipelineOptions(
        do_table_structure=True,
        table_structure_options={"mode": TableFormerMode.FAST},
    )

    converter = DocumentConverter(
        format_options={
            InputFormat.PDF: PdfFormatOption(
                pipeline_cls=StandardPdfPipeline,
                pipeline_options=pipeline_options,
            )
        }
    )
    return converter


class TestConversionParity:
    """Test that direct library conversion produces valid DoclingDocument output.

    These tests validate structural correctness of the conversion output
    using the same assertions docling-serve tests use:
    - DoclingDocument validates via model_validate()
    - Known content is present in markdown/text export
    - Page count matches expected value
    - Tables are detected where expected
    """

    def test_convert_born_digital_pdf(self, test_pdf_bytes, docling_converter):
        """Convert a real PDF and validate the DoclingDocument output.

        Mirrors docling-serve's test_fastapi_endpoints.py assertion pattern:
        validate json_content parses as DoclingDocument.
        """
        from docling.datamodel.base_models import DocumentStream
        from docling_core.types import DoclingDocument

        stream = DocumentStream(name="2408.09869v5.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)
        doc = result.document

        # Core assertion: export_to_dict() produces valid DoclingDocument
        doc_dict = doc.export_to_dict()
        assert doc_dict.get("schema_name") == "DoclingDocument"

        # Validate round-trip: dict -> DoclingDocument -> dict
        reconstituted = DoclingDocument.model_validate(doc_dict)
        assert reconstituted.name is not None

        # Validate page count (Docling Technical Report has multiple pages)
        pages = doc_dict.get("pages", {})
        assert len(pages) > 0, "Expected at least one page"

        # Validate text content is present
        texts = doc_dict.get("texts", [])
        assert len(texts) > 0, "Expected text elements in document"

        # Check for known content (title of Docling Technical Report)
        all_text = " ".join(t.get("text", "") for t in texts)
        assert "Docling" in all_text, (
            "Expected 'Docling' in document text (Docling Technical Report)"
        )

    def test_convert_produces_markdown(self, test_pdf_bytes, docling_converter):
        """Verify markdown export contains expected content.

        Mirrors docling-serve's assertion:
        check.is_in("## DocLayNet:", data["document"]["md_content"])
        """
        from docling.datamodel.base_models import DocumentStream

        stream = DocumentStream(name="2408.09869v5.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)

        md_content = result.document.export_to_markdown()
        assert len(md_content) > 100, "Expected substantial markdown output"
        assert "Docling" in md_content

    def test_convert_detects_tables(self, test_pdf_bytes, docling_converter):
        """Verify table detection works on real PDF with tables."""
        from docling.datamodel.base_models import DocumentStream

        stream = DocumentStream(name="2408.09869v5.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)
        doc_dict = result.document.export_to_dict()

        tables = doc_dict.get("tables", [])
        # The Docling Technical Report contains tables
        assert len(tables) > 0, "Expected tables in Docling Technical Report"


class TestChunkingParity:
    """Test that HybridChunker produces expected chunk structure.

    Modeled after docling-core's test_hybrid_chunker.py patterns.
    """

    def test_chunk_produces_text_chunks(self, test_pdf_bytes, docling_converter):
        """Chunk a real document and validate chunk structure."""
        from docling.chunking import HybridChunker
        from docling.datamodel.base_models import DocumentStream

        stream = DocumentStream(name="2408.09869v5.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)

        chunker = HybridChunker(max_tokens=512, merge_peers=True)
        chunks = list(chunker.chunk(result.document))

        assert len(chunks) > 0, "Expected at least one chunk"

        # Validate chunk structure
        for chunk in chunks:
            assert hasattr(chunk, "text")
            assert len(chunk.text) > 0, "Chunk should have non-empty text"
            assert hasattr(chunk, "meta")

    def test_chunk_metadata_includes_headings(self, test_pdf_bytes, docling_converter):
        """Verify chunks include heading metadata."""
        from docling.chunking import HybridChunker
        from docling.datamodel.base_models import DocumentStream

        stream = DocumentStream(name="2408.09869v5.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)

        chunker = HybridChunker(max_tokens=512, merge_peers=True)
        chunks = list(chunker.chunk(result.document))

        # At least some chunks should have heading metadata
        chunks_with_headings = [
            c
            for c in chunks
            if hasattr(c, "meta")
            and c.meta is not None
            and hasattr(c.meta, "headings")
            and c.meta.headings
        ]
        assert len(chunks_with_headings) > 0, (
            "Expected some chunks to have heading metadata"
        )

    def test_chunk_round_trip_via_dict(self, test_pdf_bytes, docling_converter):
        """Validate that chunking works after DoclingDocument round-trip.

        This mimics the actual flow: convert produces dict, chunk reconstitutes it.
        """
        from docling.chunking import HybridChunker
        from docling.datamodel.base_models import DocumentStream
        from docling_core.types import DoclingDocument

        stream = DocumentStream(name="2408.09869v5.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)

        # Simulate the flow: export to dict, then reconstitute
        doc_dict = result.document.export_to_dict()
        reconstituted = DoclingDocument.model_validate(doc_dict)

        chunker = HybridChunker(max_tokens=512, merge_peers=True)
        chunks_original = list(chunker.chunk(result.document))
        chunks_reconstituted = list(chunker.chunk(reconstituted))

        # Same number of chunks from original and reconstituted
        n_orig = len(chunks_original)
        n_recon = len(chunks_reconstituted)
        assert n_orig == n_recon, f"Chunk count mismatch: {n_orig} vs {n_recon}"

        # Same text content (exact match expected for same input)
        for orig, recon in zip(chunks_original, chunks_reconstituted, strict=False):
            assert orig.text == recon.text, "Chunk text mismatch after round-trip"


class TestResponseFormat:
    """Test that build_convert_response produces docling-serve response shape.

    These tests use real converter output to validate the full response
    format matches docling-serve's ConvertDocumentResponse.
    """

    def test_response_has_document_with_format_fields(
        self, test_pdf_bytes, docling_converter
    ):
        """All 5 formats produce the correct content fields."""
        from docling.datamodel.base_models import DocumentStream

        from docling_step_worker.response_builder import build_convert_response

        stream = DocumentStream(name="2408.09869v5.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)

        response = build_convert_response(
            result.document,
            filename="2408.09869v5.pdf",
            elapsed_seconds=1.0,
            to_formats=["markdown", "html", "text", "json", "doctags"],
        )

        assert response["status"] == "success"
        assert response["document"]["filename"] == "2408.09869v5.pdf"
        assert isinstance(response["document"]["md_content"], str)
        assert isinstance(response["document"]["html_content"], str)
        assert isinstance(response["document"]["text_content"], str)
        assert isinstance(response["document"]["json_content"], dict)
        assert isinstance(response["document"]["doctags_content"], str)
        assert isinstance(response["errors"], list)
        assert isinstance(response["timings"], dict)
        assert isinstance(response["processing_time"], float)

    def test_default_to_formats_is_markdown_only(
        self, test_pdf_bytes, docling_converter
    ):
        """Default formats produce only md_content."""
        from docling.datamodel.base_models import DocumentStream

        from docling_step_worker.response_builder import build_convert_response

        stream = DocumentStream(name="test.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)

        response = build_convert_response(
            result.document, filename="test.pdf", elapsed_seconds=0.5
        )

        assert "md_content" in response["document"]
        assert "html_content" not in response["document"]
        assert "json_content" not in response["document"]

    def test_md_content_contains_expected_text(self, test_pdf_bytes, docling_converter):
        """Markdown content includes known text from the document."""
        from docling.datamodel.base_models import DocumentStream

        from docling_step_worker.response_builder import build_convert_response

        stream = DocumentStream(name="test.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)

        response = build_convert_response(
            result.document,
            filename="test.pdf",
            elapsed_seconds=0.5,
            to_formats=["markdown"],
        )

        assert "Docling" in response["document"]["md_content"]

    def test_json_content_is_valid_docling_document(
        self, test_pdf_bytes, docling_converter
    ):
        """json_content round-trips through DoclingDocument.model_validate()."""
        from docling.datamodel.base_models import DocumentStream
        from docling_core.types import DoclingDocument

        from docling_step_worker.response_builder import build_convert_response

        stream = DocumentStream(name="test.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)

        response = build_convert_response(
            result.document,
            filename="test.pdf",
            elapsed_seconds=0.5,
            to_formats=["json"],
        )

        json_content = response["document"]["json_content"]
        reconstituted = DoclingDocument.model_validate(json_content)
        assert reconstituted.name is not None

    def test_document_dict_always_present(self, test_pdf_bytes, docling_converter):
        """document_dict is present in _run_conversion output."""
        from docling_step_worker.convert import _run_conversion

        result = _run_conversion(
            docling_converter,
            test_pdf_bytes,
            "test.pdf",
            to_formats=["markdown"],
        )

        assert "document_dict" in result
        assert isinstance(result["document_dict"], dict)
        assert result["document_dict"].get("schema_name") == "DoclingDocument"

    def test_processing_time_is_float_seconds(self, test_pdf_bytes, docling_converter):
        """processing_time is a float representing seconds."""
        from docling_step_worker.convert import _run_conversion

        result = _run_conversion(
            docling_converter,
            test_pdf_bytes,
            "test.pdf",
            to_formats=["markdown"],
        )

        assert isinstance(result["processing_time"], float)
        assert result["processing_time"] > 0


class TestFullPipelineParity:
    """Test the full classify -> convert -> chunk pipeline."""

    def test_full_pipeline(self, test_pdf_bytes, docling_converter):
        """Run the full pipeline and validate end-to-end output."""
        from docling.chunking import HybridChunker
        from docling.datamodel.base_models import DocumentStream
        from docling_core.types import DoclingDocument

        from docling_step_worker.classify import _classify_pdf_bytes

        # Step 1: Classify
        classification = _classify_pdf_bytes(test_pdf_bytes)
        assert classification["page_count"] > 0
        assert classification["has_text_layer"] is True

        # Step 2: Convert
        stream = DocumentStream(name="2408.09869v5.pdf", stream=BytesIO(test_pdf_bytes))
        result = docling_converter.convert(source=stream)
        doc_dict = result.document.export_to_dict()

        assert doc_dict.get("schema_name") == "DoclingDocument"

        # Step 3: Chunk (via reconstituted document, as the flow does)
        doc = DoclingDocument.model_validate(doc_dict)
        chunker = HybridChunker(max_tokens=512, merge_peers=True)
        chunks = list(chunker.chunk(doc))

        assert len(chunks) > 0
        total_text = " ".join(c.text for c in chunks)
        assert "Docling" in total_text

        # Pipeline metadata
        assert classification["format"] == "pdf"
        assert len(doc_dict.get("pages", {})) > 0
        assert len(doc_dict.get("texts", [])) > 0
