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

"""Shared test fixtures for docling step worker tests."""

from __future__ import annotations

from unittest.mock import AsyncMock, MagicMock

import pytest


@pytest.fixture
def sample_pdf_bytes():
    """Minimal valid PDF with text layer, no tables.

    Generated using fpdf2 for a lightweight, self-contained fixture.
    """
    from fpdf import FPDF

    pdf = FPDF()
    pdf.add_page()
    pdf.set_font("Helvetica", size=16)
    pdf.cell(0, 10, "Introduction", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=12)
    pdf.multi_cell(
        0,
        8,
        "This is the body text of the test document. "
        "It contains simple paragraphs for testing purposes.",
    )
    pdf.add_page()
    pdf.cell(0, 10, "Second Page", new_x="LMARGIN", new_y="NEXT")
    pdf.multi_cell(0, 8, "Content on the second page.")
    return bytes(pdf.output())


@pytest.fixture
def sample_pdf_with_tables_bytes():
    """PDF with table content for table detection testing."""
    from fpdf import FPDF

    pdf = FPDF()
    pdf.add_page()
    pdf.set_font("Helvetica", size=16)
    pdf.cell(0, 10, "Document with Tables", new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("Helvetica", size=12)

    # Create a simple table using cells with borders
    col_width = 60
    row_height = 10
    headers = ["Name", "Value", "Description"]
    for header in headers:
        pdf.cell(col_width, row_height, header, border=1)
    pdf.ln()
    for i in range(3):
        pdf.cell(col_width, row_height, f"Item {i}", border=1)
        pdf.cell(col_width, row_height, f"{i * 10}", border=1)
        pdf.cell(col_width, row_height, f"Description {i}", border=1)
        pdf.ln()

    return bytes(pdf.output())


@pytest.fixture
def sample_docling_document_dict():
    """A DoclingDocument dict matching docling's export_to_dict() format.

    This is the interchange format between /convert and /chunk.
    """
    return {
        "schema_name": "DoclingDocument",
        "version": "1.0.0",
        "name": "test_document",
        "origin": {"filename": "test.pdf", "mimetype": "application/pdf"},
        "furniture": {
            "self_ref": "#/furniture",
            "children": [],
            "content_layer": "furniture",
        },
        "body": {
            "self_ref": "#/body",
            "children": [
                {"$ref": "#/texts/0"},
                {"$ref": "#/texts/1"},
            ],
            "content_layer": "body",
        },
        "texts": [
            {
                "self_ref": "#/texts/0",
                "text": "Introduction",
                "label": "section_header",
                "prov": [
                    {
                        "page_no": 1,
                        "bbox": {"l": 72, "t": 700, "r": 500, "b": 720},
                    }
                ],
            },
            {
                "self_ref": "#/texts/1",
                "text": "This is the body text of the document.",
                "label": "paragraph",
                "prov": [
                    {
                        "page_no": 1,
                        "bbox": {"l": 72, "t": 650, "r": 500, "b": 670},
                    }
                ],
            },
        ],
        "tables": [],
        "pictures": [],
        "key_value_items": [],
        "pages": {"1": {"size": {"width": 612, "height": 792}, "page_no": 1}},
    }


@pytest.fixture
def sample_chunk_options():
    """Default chunk options."""
    return {
        "tokenizer": "sentence-transformers/all-MiniLM-L6-v2",
        "max_tokens": 512,
        "merge_peers": True,
    }


@pytest.fixture
def mock_context(mocker):
    """Mock StepflowContext with blob store access."""
    context = MagicMock()
    context.put_blob = AsyncMock(return_value="blob:sha256:abc123")
    context.get_blob = AsyncMock(return_value=None)
    context.get_blob_binary = AsyncMock(return_value=b"fake pdf bytes")
    context.put_blob_binary = AsyncMock(return_value="blob:sha256:def456")
    return context
