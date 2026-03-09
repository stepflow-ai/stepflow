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

"""Unit tests for facade translate functions.

Pure function tests — no server, no I/O, no docling imports.
"""

from __future__ import annotations

import base64

import pytest

from docling_step_worker.facade.translate import (
    flow_output_to_response,
    normalize_v1alpha_request,
    request_to_flow_input,
)

# ---------------------------------------------------------------------------
# normalize_v1alpha_request
# ---------------------------------------------------------------------------


class TestNormalizeV1alphaRequest:
    def test_http_sources_converted(self):
        body = {
            "http_sources": [{"url": "https://example.com/doc.pdf"}],
            "options": {"to_formats": ["md"]},
        }
        result = normalize_v1alpha_request(body)
        assert "http_sources" not in result
        assert result["sources"] == [
            {"kind": "http", "url": "https://example.com/doc.pdf"}
        ]
        assert result["options"] == {"to_formats": ["md"]}

    def test_file_sources_converted(self):
        body = {
            "file_sources": [{"base64_string": "dGVzdA==", "filename": "test.pdf"}],
        }
        result = normalize_v1alpha_request(body)
        assert "file_sources" not in result
        assert result["sources"] == [
            {"kind": "file", "base64_string": "dGVzdA==", "filename": "test.pdf"}
        ]

    def test_mixed_sources(self):
        body = {
            "http_sources": [{"url": "https://example.com/a.pdf"}],
            "file_sources": [{"base64_string": "abc", "filename": "b.pdf"}],
        }
        result = normalize_v1alpha_request(body)
        assert len(result["sources"]) == 2
        assert result["sources"][0]["kind"] == "http"
        assert result["sources"][1]["kind"] == "file"

    def test_v1_body_unchanged(self):
        body = {
            "sources": [{"kind": "http", "url": "https://example.com/doc.pdf"}],
        }
        result = normalize_v1alpha_request(body)
        assert result is body  # same object, no copy

    def test_empty_sources(self):
        body: dict = {}
        result = normalize_v1alpha_request(body)
        assert result["sources"] == []


# ---------------------------------------------------------------------------
# request_to_flow_input
# ---------------------------------------------------------------------------


class TestRequestToFlowInput:
    def test_url_source(self):
        sources = [{"kind": "http", "url": "https://arxiv.org/pdf/2501.17887"}]
        result = request_to_flow_input(sources=sources)
        assert result["source"] == "https://arxiv.org/pdf/2501.17887"
        assert result["source_kind"] == "url"

    def test_file_source(self):
        sources = [
            {"kind": "file", "base64_string": "dGVzdA==", "filename": "test.pdf"}
        ]
        result = request_to_flow_input(sources=sources)
        assert result["source"] == "dGVzdA=="
        assert result["source_kind"] == "base64"

    def test_file_bytes_upload(self):
        raw = b"fake pdf content"
        result = request_to_flow_input(file_bytes=raw, filename="uploaded.pdf")
        assert result["source"] == base64.b64encode(raw).decode("ascii")
        assert result["source_kind"] == "base64"

    def test_options_passthrough(self):
        sources = [{"kind": "http", "url": "https://example.com/doc.pdf"}]
        options = {
            "do_ocr": True,
            "table_mode": "accurate",
            "to_formats": ["md", "json"],
            "image_export_mode": "placeholder",
        }
        result = request_to_flow_input(sources=sources, options=options)
        # to_formats and image_export_mode hoisted to top level
        assert result["to_formats"] == ["md", "json"]
        assert result["image_export_mode"] == "placeholder"
        # remaining options passed through
        assert result["options"]["do_ocr"] is True
        assert result["options"]["table_mode"] == "accurate"
        # hoisted fields removed from options
        assert "to_formats" not in result["options"]
        assert "image_export_mode" not in result["options"]

    def test_no_sources_or_bytes_raises(self):
        with pytest.raises(ValueError, match="Either sources or file_bytes"):
            request_to_flow_input()

    def test_empty_options_gets_defaults(self):
        sources = [{"kind": "http", "url": "https://example.com/doc.pdf"}]
        result = request_to_flow_input(sources=sources, options=None)
        # Default images_scale is applied even when no options provided
        assert result["options"] == {"images_scale": 2.0}

    def test_options_with_only_hoisted_fields(self):
        sources = [{"kind": "http", "url": "https://example.com/doc.pdf"}]
        options = {"to_formats": ["md"]}
        result = request_to_flow_input(sources=sources, options=options)
        assert result["to_formats"] == ["md"]
        # After hoisting, only the default images_scale remains
        assert result["options"] == {"images_scale": 2.0}


# ---------------------------------------------------------------------------
# flow_output_to_response
# ---------------------------------------------------------------------------


class TestFlowOutputToResponse:
    def test_full_output(self):
        flow_output = {
            "document": {
                "filename": "doc.pdf",
                "md_content": "# Hello",
                "html_content": "<h1>Hello</h1>",
            },
            "status": "success",
            "errors": [],
            "processing_time": 3.14,
            "timings": {"parse": 1.0, "render": 2.14},
            "chunks": [{"text": "chunk1"}],
            "classification": {"format": "pdf"},
        }
        result = flow_output_to_response(flow_output)
        assert result["document"]["md_content"] == "# Hello"
        assert result["status"] == "success"
        assert result["errors"] == []
        assert result["processing_time"] == 3.14
        assert result["timings"]["parse"] == 1.0
        # chunks and classification are not in ConvertDocumentResponse
        assert "chunks" not in result
        assert "classification" not in result

    def test_minimal_output(self):
        result = flow_output_to_response({})
        assert result["document"] == {}
        assert result["status"] == "success"
        assert result["errors"] == []
        assert result["processing_time"] == 0.0
        assert result["timings"] == {}

    def test_failed_status(self):
        flow_output = {
            "document": {},
            "status": "failure",
            "errors": [
                {
                    "component_type": "response_builder",
                    "module_name": "docling_step_worker",
                    "error_message": "Conversion failed",
                }
            ],
            "processing_time": 0.5,
            "timings": {},
        }
        result = flow_output_to_response(flow_output)
        assert result["status"] == "failure"
        assert len(result["errors"]) == 1
        assert result["errors"][0]["error_message"] == "Conversion failed"
