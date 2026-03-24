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

"""Contract tests validating facade behavior against Stepflow API response shapes.

These fixtures represent the actual response structures from Stepflow v0.12.0+.
Field names, nesting, and types are derived from stepflow-rs/proto/stepflow/v1/runs.proto
and verified against live API output.

When the Stepflow API changes its response shape, these fixtures must be updated
to match — and the facade extraction logic must be updated accordingly.
If a test fails, it means either:
  1. The API changed and the fixture is stale (update fixture + facade), or
  2. The facade's extraction logic regressed (fix the facade).
"""

from __future__ import annotations

import copy

import pytest

from docling_proto_step_worker.facade.translate import (
    flow_output_to_response,
    run_result_to_convert_response,
    run_status_to_poll_response,
)

# ---------------------------------------------------------------------------
# Stepflow v0.12.0 API response fixtures
#
# Source of truth: stepflow-rs/proto/stepflow/v1/runs.proto
# ExecutionStatus enum: 0=unspecified, 1=running, 2=completed, 3=failed,
#                       4=cancelled, 5=paused, 6=recovery_failed
# ---------------------------------------------------------------------------

# POST /api/v1/runs (wait=true) — completed successfully
COMPLETED_RUN_RESPONSE: dict = {
    "summary": {
        "runId": "019d1d32-26d2-7af3-8368-7378b198918c",
        "flowId": "5a2bc43a6089050d28348fefcde79e3d6d06a70dafac777a832497d9b10837dd",
        "flowName": "docling-process-document",
        "status": 2,
        "items": {
            "total": 1,
            "completed": 1,
            "running": 0,
            "failed": 0,
            "cancelled": 0,
        },
        "createdAt": "2026-03-25T10:30:00Z",
        "completedAt": "2026-03-25T10:30:05Z",
        "rootRunId": "019d1d32-26d2-7af3-8368-7378b198918c",
        "orchestratorId": "test-orchestrator",
        "createdAtSeqno": 1,
        "finishedAtSeqno": 10,
    },
    "results": [
        {
            "itemIndex": 0,
            "status": 2,
            "output": {
                "document": {
                    "md_content": "# Docling Technical Report\n\nThis is test content.",
                    "filename": "test.pdf",
                },
                "status": "success",
                "errors": [],
                "processing_time": 1.5,
                "timings": {"classify": 0.1, "convert": 1.2, "chunk": 0.2},
                "classification": {"format": "pdf", "has_text_layer": True},
                "chunks": [{"text": "chunk1"}],
            },
            "completedAt": "2026-03-25T10:30:05Z",
        }
    ],
}

# POST /api/v1/runs (wait=true) — failed execution
FAILED_RUN_RESPONSE: dict = {
    "summary": {
        "runId": "019d1d32-aaaa-bbbb-cccc-ddddeeeeeeee",
        "flowId": "5a2bc43a6089050d28348fefcde79e3d6d06a70dafac777a832497d9b10837dd",
        "flowName": "docling-process-document",
        "status": 3,
        "items": {
            "total": 1,
            "completed": 0,
            "running": 0,
            "failed": 1,
            "cancelled": 0,
        },
        "createdAt": "2026-03-25T10:30:00Z",
        "completedAt": "2026-03-25T10:30:02Z",
        "rootRunId": "019d1d32-aaaa-bbbb-cccc-ddddeeeeeeee",
        "orchestratorId": "test-orchestrator",
        "createdAtSeqno": 1,
        "finishedAtSeqno": 5,
    },
    "results": [
        {
            "itemIndex": 0,
            "status": 3,
            "output": {},
            "error": {
                "message": "Failed to retrieve document for conversion",
                "code": "COMPONENT_ERROR",
            },
            "completedAt": "2026-03-25T10:30:02Z",
        }
    ],
}

# GET /api/v1/runs/{id} — running (no results yet)
RUNNING_GET_RUN_RESPONSE: dict = {
    "summary": {
        "runId": "019d1d32-1111-2222-3333-444455556666",
        "flowId": "5a2bc43a6089050d28348fefcde79e3d6d06a70dafac777a832497d9b10837dd",
        "flowName": "docling-process-document",
        "status": 1,
        "items": {
            "total": 1,
            "completed": 0,
            "running": 1,
            "failed": 0,
            "cancelled": 0,
        },
        "createdAt": "2026-03-25T10:30:00Z",
        "rootRunId": "019d1d32-1111-2222-3333-444455556666",
        "orchestratorId": "test-orchestrator",
        "createdAtSeqno": 1,
    },
    "steps": [
        {
            "stepId": "classify",
            "stepIndex": 0,
            "itemIndex": 0,
            "status": 1,
            "component": "/docling/classify",
            "startedAt": "2026-03-25T10:30:01Z",
        }
    ],
}

# GET /api/v1/runs/{id} — completed (with steps)
COMPLETED_GET_RUN_RESPONSE: dict = {
    "summary": {
        "runId": "019d1d32-26d2-7af3-8368-7378b198918c",
        "flowId": "5a2bc43a6089050d28348fefcde79e3d6d06a70dafac777a832497d9b10837dd",
        "flowName": "docling-process-document",
        "status": 2,
        "items": {
            "total": 1,
            "completed": 1,
            "running": 0,
            "failed": 0,
            "cancelled": 0,
        },
        "createdAt": "2026-03-25T10:30:00Z",
        "completedAt": "2026-03-25T10:30:05Z",
        "rootRunId": "019d1d32-26d2-7af3-8368-7378b198918c",
        "orchestratorId": "test-orchestrator",
        "createdAtSeqno": 1,
        "finishedAtSeqno": 10,
    },
    "steps": [
        {
            "stepId": "classify",
            "stepIndex": 0,
            "itemIndex": 0,
            "status": 2,
            "component": "/docling/classify",
            "startedAt": "2026-03-25T10:30:01Z",
            "completedAt": "2026-03-25T10:30:02Z",
        },
        {
            "stepId": "convert",
            "stepIndex": 1,
            "itemIndex": 0,
            "status": 2,
            "component": "/docling/convert",
            "startedAt": "2026-03-25T10:30:02Z",
            "completedAt": "2026-03-25T10:30:04Z",
        },
        {
            "stepId": "chunk",
            "stepIndex": 2,
            "itemIndex": 0,
            "status": 2,
            "component": "/docling/chunk",
            "startedAt": "2026-03-25T10:30:04Z",
            "completedAt": "2026-03-25T10:30:05Z",
        },
    ],
}

# GET /api/v1/runs/{id}/items — same shape as POST results
GET_RUN_ITEMS_RESPONSE: dict = {
    "results": [
        {
            "itemIndex": 0,
            "status": 2,
            "output": {
                "document": {
                    "md_content": "# Docling Technical Report\n\nThis is test content.",
                    "filename": "test.pdf",
                },
                "status": "success",
                "errors": [],
                "processing_time": 1.5,
                "timings": {"classify": 0.1, "convert": 1.2, "chunk": 0.2},
                "classification": {"format": "pdf", "has_text_layer": True},
                "chunks": [{"text": "chunk1"}],
            },
            "completedAt": "2026-03-25T10:30:05Z",
        }
    ],
}


# ---------------------------------------------------------------------------
# Fixture shape validation — catches stale fixtures
# ---------------------------------------------------------------------------


class TestResponseShapeValidation:
    """Structural assertions on the fixture data itself.

    If the Stepflow API shape changes, update the fixtures above and these
    tests will verify the new shape is internally consistent.
    """

    def test_run_response_has_required_fields(self):
        assert "summary" in COMPLETED_RUN_RESPONSE
        assert "results" in COMPLETED_RUN_RESPONSE
        assert isinstance(COMPLETED_RUN_RESPONSE["results"], list)

    def test_summary_has_required_fields(self):
        summary = COMPLETED_RUN_RESPONSE["summary"]
        for field in ("runId", "status", "items", "createdAt"):
            assert field in summary, f"Missing required summary field: {field}"

    def test_result_item_has_required_fields(self):
        item = COMPLETED_RUN_RESPONSE["results"][0]
        for field in ("itemIndex", "status", "output"):
            assert field in item, f"Missing required result item field: {field}"

    def test_status_values_are_integers(self):
        """The v0.12.0 change that broke the facade: status is int, not str."""
        assert isinstance(COMPLETED_RUN_RESPONSE["summary"]["status"], int)
        assert isinstance(COMPLETED_RUN_RESPONSE["results"][0]["status"], int)
        assert isinstance(FAILED_RUN_RESPONSE["summary"]["status"], int)
        assert isinstance(FAILED_RUN_RESPONSE["results"][0]["status"], int)
        assert isinstance(RUNNING_GET_RUN_RESPONSE["summary"]["status"], int)

    def test_get_run_response_has_steps_not_results(self):
        """GET /api/v1/runs/{id} returns 'steps', not 'results'."""
        assert "steps" in COMPLETED_GET_RUN_RESPONSE
        assert "results" not in COMPLETED_GET_RUN_RESPONSE

    def test_get_run_items_uses_results_key(self):
        """GET /api/v1/runs/{id}/items returns 'results' key."""
        assert "results" in GET_RUN_ITEMS_RESPONSE

    def test_output_is_direct_field_not_nested(self):
        """v0.12.0: output is results[i].output, NOT results[i].result.result."""
        item = COMPLETED_RUN_RESPONSE["results"][0]
        assert "output" in item
        assert "result" not in item


# ---------------------------------------------------------------------------
# Run response extraction — mirrors app.py _submit_and_respond logic
# ---------------------------------------------------------------------------


class TestRunResponseExtraction:
    """Validates that facade extraction logic works with v0.12.0 response shapes.

    These tests replicate the extraction logic from app.py's _submit_and_respond
    and _get_result methods against the contract fixtures.
    """

    def test_extract_flow_output_from_completed_run(self):
        item = COMPLETED_RUN_RESPONSE["results"][0]
        flow_output = item.get("output", {})
        assert flow_output["document"]["md_content"] != ""
        assert flow_output["status"] == "success"

    def test_extract_status_completed(self):
        item = COMPLETED_RUN_RESPONSE["results"][0]
        assert item.get("status") == 2

    def test_extract_status_failed(self):
        item = FAILED_RUN_RESPONSE["results"][0]
        assert item.get("status") == 3

    def test_failed_run_has_error_info(self):
        item = FAILED_RUN_RESPONSE["results"][0]
        error = item.get("error", {})
        assert error.get("message") is not None
        assert len(error["message"]) > 0

    def test_no_results_handled(self):
        run_data = copy.deepcopy(COMPLETED_RUN_RESPONSE)
        run_data["results"] = []
        results = run_data.get("results") or []
        assert len(results) == 0

    def test_flow_output_to_response_with_contract_data(self):
        """End-to-end: extract output from fixture → translate to docling response."""
        item = COMPLETED_RUN_RESPONSE["results"][0]
        flow_output = item.get("output", {})
        response = flow_output_to_response(flow_output)

        assert response["document"]["md_content"] != ""
        assert response["status"] == "success"
        assert response["errors"] == []
        assert response["processing_time"] == 1.5
        # chunks and classification must NOT appear in ConvertDocumentResponse
        assert "chunks" not in response
        assert "classification" not in response

    def test_items_endpoint_same_extraction_pattern(self):
        """GET /api/v1/runs/{id}/items uses same results[i].output pattern."""
        item = GET_RUN_ITEMS_RESPONSE["results"][0]
        flow_output = item.get("output", {})
        response = flow_output_to_response(flow_output)
        assert response["document"]["md_content"] != ""


# ---------------------------------------------------------------------------
# Poll response contract — run_status_to_poll_response
# ---------------------------------------------------------------------------


class TestPollResponseContract:
    """Validates run_status_to_poll_response against v0.12.0 response shapes."""

    def test_completed_run_maps_to_success(self):
        result = run_status_to_poll_response(COMPLETED_GET_RUN_RESPONSE)
        assert result["task_status"] == "success"

    def test_running_run_maps_to_started(self):
        result = run_status_to_poll_response(RUNNING_GET_RUN_RESPONSE)
        assert result["task_status"] == "started"

    def test_failed_run_maps_to_failure(self):
        result = run_status_to_poll_response(FAILED_RUN_RESPONSE)
        assert result["task_status"] == "failure"

    def test_failed_run_includes_error_message(self):
        result = run_status_to_poll_response(FAILED_RUN_RESPONSE)
        assert result["error_message"] is not None
        assert "Failed to retrieve" in result["error_message"]

    def test_run_id_extracted_from_summary(self):
        """runId lives in summary.runId, not at the top level."""
        result = run_status_to_poll_response(COMPLETED_GET_RUN_RESPONSE)
        assert result["task_id"] == COMPLETED_GET_RUN_RESPONSE["summary"]["runId"]

    def test_poll_response_has_all_required_fields(self):
        """docling-serve TaskStatusResponse requires these fields."""
        result = run_status_to_poll_response(COMPLETED_GET_RUN_RESPONSE)
        for field in (
            "task_id",
            "task_type",
            "task_status",
            "task_position",
            "task_meta",
            "error_message",
        ):
            assert field in result, f"Missing required poll response field: {field}"


# ---------------------------------------------------------------------------
# Result extraction contract — run_result_to_convert_response
# ---------------------------------------------------------------------------


class TestResultExtractionContract:
    """Validates run_result_to_convert_response against v0.12.0 response shapes."""

    def test_extracts_document_from_output(self):
        result = run_result_to_convert_response(COMPLETED_RUN_RESPONSE)
        assert result is not None
        assert result["document"]["md_content"] != ""

    def test_strips_non_response_fields(self):
        result = run_result_to_convert_response(COMPLETED_RUN_RESPONSE)
        assert result is not None
        assert "chunks" not in result
        assert "classification" not in result

    def test_returns_none_when_no_results(self):
        run_data = {"summary": COMPLETED_RUN_RESPONSE["summary"], "results": []}
        result = run_result_to_convert_response(run_data)
        assert result is None

    def test_returns_none_when_results_key_missing(self):
        run_data = {"summary": COMPLETED_RUN_RESPONSE["summary"]}
        result = run_result_to_convert_response(run_data)
        assert result is None

    def test_items_endpoint_response_works(self):
        """GET /api/v1/runs/{id}/items response works with same function."""
        result = run_result_to_convert_response(GET_RUN_ITEMS_RESPONSE)
        assert result is not None
        assert result["document"]["md_content"] != ""
