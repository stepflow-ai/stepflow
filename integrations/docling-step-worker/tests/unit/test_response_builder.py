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

"""Tests for response_builder module.

Uses mocked DoclingDocument — no real models needed.
"""

from __future__ import annotations

from unittest.mock import MagicMock

from docling_step_worker.response_builder import (
    build_convert_response,
    build_export_document,
    normalize_format_name,
)


def _make_mock_doc() -> MagicMock:
    """Create a mock DoclingDocument with all 5 export methods."""
    doc = MagicMock()
    doc.export_to_markdown.return_value = "# Title\n\nBody text"
    doc.export_to_html.return_value = "<h1>Title</h1><p>Body text</p>"
    doc.export_to_text.return_value = "Title\nBody text"
    doc.export_to_dict.return_value = {"schema_name": "DoclingDocument", "name": "test"}
    doc.export_to_doctags.return_value = "<doctag>content</doctag>"
    return doc


class TestBuildExportDocument:
    """Tests for build_export_document."""

    def test_default_formats_markdown_only(self):
        doc = _make_mock_doc()
        result = build_export_document(doc, "test.pdf")

        assert result["filename"] == "test.pdf"
        assert result["md_content"] == "# Title\n\nBody text"
        assert "html_content" not in result
        assert "text_content" not in result
        assert "json_content" not in result
        assert "doctags_content" not in result

    def test_all_five_formats(self):
        doc = _make_mock_doc()
        result = build_export_document(
            doc,
            "test.pdf",
            to_formats=["markdown", "html", "text", "json", "doctags"],
        )

        assert result["md_content"] == "# Title\n\nBody text"
        assert result["html_content"] == "<h1>Title</h1><p>Body text</p>"
        assert result["text_content"] == "Title\nBody text"
        assert result["json_content"] == {
            "schema_name": "DoclingDocument",
            "name": "test",
        }
        assert result["doctags_content"] == "<doctag>content</doctag>"

    def test_image_mode_passed_to_markdown_and_html(self):
        from docling_core.types.doc.document import ImageRefMode

        doc = _make_mock_doc()
        build_export_document(
            doc,
            "test.pdf",
            to_formats=["markdown", "html"],
            image_export_mode="embedded",
        )

        doc.export_to_markdown.assert_called_once_with(image_mode=ImageRefMode.EMBEDDED)
        doc.export_to_html.assert_called_once_with(image_mode=ImageRefMode.EMBEDDED)

    def test_image_mode_not_passed_to_text_json_doctags(self):
        doc = _make_mock_doc()
        build_export_document(
            doc,
            "test.pdf",
            to_formats=["text", "json", "doctags"],
        )

        doc.export_to_text.assert_called_once_with()
        doc.export_to_dict.assert_called_once_with()
        doc.export_to_doctags.assert_called_once_with()

    def test_unknown_format_skipped(self):
        doc = _make_mock_doc()
        result = build_export_document(
            doc, "test.pdf", to_formats=["markdown", "unknown_fmt"]
        )

        assert result["md_content"] == "# Title\n\nBody text"
        assert "unknown_fmt_content" not in result
        assert result["_export_errors"] == []

    def test_invalid_image_export_mode_falls_back_to_embedded(self):
        doc = _make_mock_doc()
        result = build_export_document(
            doc, "test.pdf", to_formats=["markdown"], image_export_mode="bogus"
        )

        assert result["md_content"] == "# Title\n\nBody text"
        assert result["_export_errors"] == []

    def test_format_name_md_normalized_to_markdown(self):
        doc = _make_mock_doc()
        result = build_export_document(doc, "test.pdf", to_formats=["md"])

        assert result["md_content"] == "# Title\n\nBody text"

    def test_format_name_all_aliases(self):
        assert normalize_format_name("md") == "markdown"
        assert normalize_format_name("html") == "html"
        assert normalize_format_name("json") == "json"
        assert normalize_format_name("text") == "text"
        assert normalize_format_name("doctags") == "doctags"

    def test_unknown_format_name_skipped_after_normalization(self):
        doc = _make_mock_doc()
        result = build_export_document(doc, "test.pdf", to_formats=["xml"])

        assert "xml_content" not in result
        assert result["_export_errors"] == []

    def test_export_failure_produces_none_and_error_item(self):
        doc = _make_mock_doc()
        doc.export_to_html.side_effect = RuntimeError("render failed")

        result = build_export_document(doc, "test.pdf", to_formats=["markdown", "html"])

        assert result["md_content"] == "# Title\n\nBody text"
        assert result["html_content"] is None
        errors = result["_export_errors"]
        assert len(errors) == 1
        assert errors[0]["component_type"] == "response_builder"
        assert errors[0]["module_name"] == "docling_step_worker"
        assert "render failed" in errors[0]["error_message"]


class TestBuildConvertResponse:
    """Tests for build_convert_response."""

    def test_full_response_shape(self):
        doc = _make_mock_doc()
        result = build_convert_response(
            doc,
            filename="report.pdf",
            elapsed_seconds=1.234,
            to_formats=["markdown"],
        )

        assert result["status"] == "success"
        assert result["processing_time"] == 1.234
        assert isinstance(result["errors"], list)
        assert isinstance(result["timings"], dict)
        assert result["document"]["filename"] == "report.pdf"
        assert result["document"]["md_content"] == "# Title\n\nBody text"
        assert "_export_errors" not in result["document"]

    def test_errors_pass_through(self):
        doc = _make_mock_doc()
        pre_errors = [
            {
                "component_type": "converter",
                "module_name": "test",
                "error_message": "warning",
            }
        ]
        result = build_convert_response(
            doc,
            filename="test.pdf",
            elapsed_seconds=0.5,
            errors=pre_errors,
        )

        assert len(result["errors"]) == 1
        assert result["errors"][0]["error_message"] == "warning"

    def test_export_errors_propagated_to_top_level(self):
        doc = _make_mock_doc()
        doc.export_to_markdown.side_effect = RuntimeError("md fail")

        result = build_convert_response(
            doc,
            filename="test.pdf",
            elapsed_seconds=0.1,
            to_formats=["markdown"],
        )

        assert result["document"]["md_content"] is None
        assert len(result["errors"]) == 1
        assert "md fail" in result["errors"][0]["error_message"]

    def test_export_errors_stripped_from_document(self):
        doc = _make_mock_doc()
        result = build_convert_response(
            doc,
            filename="test.pdf",
            elapsed_seconds=0.1,
        )

        assert "_export_errors" not in result["document"]
