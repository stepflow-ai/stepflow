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

from docling_proto_step_worker.facade.translate import (
    flow_output_to_response,
    normalize_v1alpha_request,
    request_to_flow_input,
    run_result_to_convert_response,
    run_status_to_poll_response,
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
                    "module_name": "docling_proto_step_worker",
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


# ---------------------------------------------------------------------------
# run_status_to_poll_response
# ---------------------------------------------------------------------------


class TestRunStatusToPollResponse:
    def test_completed_status(self):
        run_data = {"summary": {"runId": "run-1", "status": 2}}
        result = run_status_to_poll_response(run_data)
        assert result["task_id"] == "run-1"
        assert result["task_status"] == "success"
        assert result["task_type"] == "convert"

    def test_running_status(self):
        run_data = {"summary": {"runId": "run-2", "status": 1}}
        result = run_status_to_poll_response(run_data)
        assert result["task_status"] == "started"

    def test_failed_status(self):
        run_data = {
            "summary": {"runId": "run-3", "status": 3},
            "results": [
                {"error": {"message": "Component crashed"}},
            ],
        }
        result = run_status_to_poll_response(run_data)
        assert result["task_status"] == "failure"
        assert result["error_message"] == "Component crashed"

    def test_failed_without_results_uses_default_message(self):
        run_data = {"summary": {"runId": "run-4", "status": 3}, "results": [{}]}
        result = run_status_to_poll_response(run_data)
        assert result["task_status"] == "failure"
        assert result["error_message"] == "Flow execution failed"

    def test_cancelled_maps_to_failure(self):
        run_data = {"summary": {"runId": "run-5", "status": 4}}
        result = run_status_to_poll_response(run_data)
        assert result["task_status"] == "failure"

    def test_unknown_status_maps_to_pending(self):
        run_data = {"summary": {"runId": "run-6", "status": 0}}
        result = run_status_to_poll_response(run_data)
        assert result["task_status"] == "pending"

    def test_custom_task_type(self):
        run_data = {"summary": {"runId": "run-7", "status": 2}}
        result = run_status_to_poll_response(run_data, task_type="chunk")
        assert result["task_type"] == "chunk"

    def test_all_response_fields_present(self):
        run_data = {"summary": {"runId": "run-8", "status": 2}}
        result = run_status_to_poll_response(run_data)
        expected_fields = {
            "task_id",
            "task_type",
            "task_status",
            "task_position",
            "task_meta",
            "error_message",
        }
        assert set(result.keys()) == expected_fields

    def test_fallback_when_no_summary_wrapper(self):
        """Handles flat run_data (no summary wrapper) for backward compat."""
        run_data = {"runId": "run-flat", "status": 2}
        result = run_status_to_poll_response(run_data)
        assert result["task_id"] == "run-flat"
        assert result["task_status"] == "success"


# ---------------------------------------------------------------------------
# run_result_to_convert_response
# ---------------------------------------------------------------------------


class TestRunResultToConvertResponse:
    def test_extracts_from_output(self):
        run_data = {
            "results": [
                {
                    "output": {
                        "document": {"md_content": "# Hello", "filename": "doc.pdf"},
                        "status": "success",
                        "errors": [],
                        "processing_time": 2.0,
                        "timings": {"convert": 1.5},
                        "chunks": [{"text": "c1"}],
                        "classification": {"format": "pdf"},
                    }
                }
            ]
        }
        result = run_result_to_convert_response(run_data)
        assert result is not None
        assert result["document"]["md_content"] == "# Hello"
        assert result["status"] == "success"
        assert result["processing_time"] == 2.0
        assert "chunks" not in result
        assert "classification" not in result

    def test_returns_none_for_empty_results(self):
        assert run_result_to_convert_response({"results": []}) is None

    def test_returns_none_for_missing_results(self):
        assert run_result_to_convert_response({}) is None

    def test_empty_output(self):
        run_data = {"results": [{"output": {}}]}
        result = run_result_to_convert_response(run_data)
        assert result is not None
        assert result["document"] == {}
        assert result["status"] == "success"
