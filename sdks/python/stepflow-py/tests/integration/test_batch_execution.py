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

"""Integration tests for batch execution via StepflowClient.

These tests verify that the StepflowClient can correctly submit and execute
batch workflows using the stepflow-server API.
"""

from __future__ import annotations

from typing import Any

import pytest
from google.protobuf import json_format

from stepflow_py import StepflowClient
from stepflow_py.proto.common_pb2 import (
    EXECUTION_STATUS_COMPLETED,
    EXECUTION_STATUS_FAILED,
)
from stepflow_py.proto.runs_pb2 import CreateRunResponse


@pytest.fixture
def simple_workflow():
    """Create a simple workflow for batch testing using Python UDF.

    This creates a workflow that doubles an input number using a Python UDF.
    """
    return {
        "schema": "https://stepflow.org/schemas/v1/flow.json",
        "input_schema": {
            "type": "object",
            "properties": {"x": {"type": "number"}},
        },
        "output_schema": {
            "type": "object",
            "properties": {"result": {"type": "number"}},
        },
        "steps": [
            # Create blob with UDF code
            {
                "id": "create_double_blob",
                "component": "/builtin/put_blob",
                "input": {
                    "data": {
                        "input_schema": {
                            "type": "object",
                            "properties": {"x": {"type": "number"}},
                            "required": ["x"],
                        },
                        "code": "input['x'] * 2",
                    },
                    "blob_type": "data",
                },
            },
            # Execute UDF
            {
                "id": "double",
                "component": "/python/udf",
                "input": {
                    "blob_id": {
                        "$step": "create_double_blob",
                        "path": "blob_id",
                    },
                    "input": {"x": {"$input": "x"}},
                },
            },
        ],
        "output": {"result": {"$step": "double"}},
    }


@pytest.fixture
def batch_inputs():
    """Create batch inputs for testing."""
    return [
        {"x": 1},
        {"x": 2},
        {"x": 3},
        {"x": 5},
        {"x": 8},
    ]


async def store_and_run_batch(
    client: StepflowClient,
    workflow: dict,
    inputs: list[dict],
    max_concurrency: int | None = None,
    timeout: float = 120.0,
) -> CreateRunResponse:
    """Helper to store workflow and run batch execution."""
    # Store the workflow
    store_response = await client.store_flow(workflow)
    flow_id = store_response.flow_id

    # Run with batch inputs
    return await client.run(
        flow_id=flow_id,
        input_data=inputs,
        max_concurrency=max_concurrency,
        timeout=timeout,
    )


def get_result_items(response: CreateRunResponse) -> list[dict[str, Any]]:
    """Extract item results from the protobuf response.

    Converts each ItemResult protobuf message to a dict with
    'outcome' and optionally 'result' keys for uniform access.
    """
    items = []
    for item_result in response.results:
        item_dict: dict[str, Any] = {}
        if item_result.status == EXECUTION_STATUS_COMPLETED:
            item_dict["outcome"] = "success"
            if item_result.HasField("output"):
                item_dict["result"] = json_format.MessageToDict(item_result.output)
        elif item_result.status == EXECUTION_STATUS_FAILED:
            item_dict["outcome"] = "failed"
            if item_result.HasField("error_message"):
                item_dict["error"] = item_result.error_message
        else:
            from stepflow_py.proto.common_pb2 import ExecutionStatus

            item_dict["outcome"] = ExecutionStatus.Name(item_result.status)
        items.append(item_dict)
    return items


@pytest.mark.asyncio
async def test_batch_execution_basic(stepflow_client, simple_workflow, batch_inputs):
    """Test basic batch execution with multiple inputs."""
    response = await store_and_run_batch(stepflow_client, simple_workflow, batch_inputs)

    assert response.summary.status == EXECUTION_STATUS_COMPLETED, (
        f"Batch execution failed with status {response.summary.status}"
    )
    assert response.summary.items.total == 5, (
        f"Expected 5 items, got {response.summary.items.total}"
    )

    # For batch execution, result contains items array
    items = get_result_items(response)
    assert len(items) == 5, f"Expected 5 results, got {len(items)}"

    # Verify each item succeeded
    for i, item in enumerate(items):
        assert item.get("outcome") == "success", f"Item {i} failed: {item}"


@pytest.mark.asyncio
async def test_batch_execution_with_max_concurrent(
    stepflow_client, simple_workflow, batch_inputs
):
    """Test batch execution with max_concurrent limit."""
    response = await store_and_run_batch(
        stepflow_client, simple_workflow, batch_inputs, max_concurrency=2
    )

    assert response.summary.status == EXECUTION_STATUS_COMPLETED, (
        f"Batch execution failed with status {response.summary.status}"
    )
    assert response.summary.items.total == 5

    items = get_result_items(response)
    assert len(items) == 5, f"Expected 5 results, got {len(items)}"

    for i, item in enumerate(items):
        assert item.get("outcome") == "success", f"Item {i} failed: {item}"


@pytest.mark.asyncio
async def test_batch_execution_results(stepflow_client, simple_workflow, batch_inputs):
    """Test batch execution returns correct results for each input."""
    response = await store_and_run_batch(stepflow_client, simple_workflow, batch_inputs)

    assert response.summary.status == EXECUTION_STATUS_COMPLETED
    assert response.summary.items.total == 5

    items = get_result_items(response)
    assert len(items) == 5, f"Expected 5 results, got {len(items)}"

    # Expected inputs and outputs: [1, 2, 3, 5, 8] -> [2, 4, 6, 10, 16]
    expected_outputs = [2, 4, 6, 10, 16]

    for i, item in enumerate(items):
        assert item.get("outcome") == "success", f"Item {i} failed: {item}"
        workflow_output = item.get("result", {})
        actual_result = workflow_output.get("result")
        expected_result = expected_outputs[i]
        assert actual_result == expected_result, (
            f"Index {i}: Expected {expected_result}, got {actual_result}"
        )


@pytest.mark.asyncio
async def test_batch_execution_with_failures(stepflow_client, simple_workflow):
    """Test batch execution with mix of valid and invalid inputs.

    The Python UDF will fail on invalid inputs (missing field or wrong type),
    demonstrating that batch execution properly handles partial failures.
    """
    # Create various inputs - some will fail with Python UDF type checking
    inputs = [
        {"x": 1},  # Valid
        {},  # Invalid - missing x
        {"x": 2},  # Valid
        {"x": "not_a_number"},  # Invalid - wrong type
        {"x": 3},  # Valid
    ]

    response = await store_and_run_batch(stepflow_client, simple_workflow, inputs)

    # Batch with failures may still complete (partial success)
    assert response.summary.status in [
        EXECUTION_STATUS_COMPLETED,
        EXECUTION_STATUS_FAILED,
    ], f"Unexpected status: {response.summary.status}"
    assert response.summary.items.total == 5

    items = get_result_items(response)
    assert len(items) == 5, f"Expected 5 results, got {len(items)}"

    # Count completed vs failed
    completed_count = sum(1 for item in items if item.get("outcome") == "success")
    failed_count = sum(1 for item in items if item.get("outcome") == "failed")

    assert completed_count == 3, f"Expected 3 successful, got {completed_count}"
    assert failed_count == 2, f"Expected 2 failed, got {failed_count}"

    # Verify specific results
    assert items[0]["outcome"] == "success"
    assert items[0]["result"]["result"] == 2

    assert items[1]["outcome"] == "failed"

    assert items[2]["outcome"] == "success"
    assert items[2]["result"]["result"] == 4

    assert items[3]["outcome"] == "failed"

    assert items[4]["outcome"] == "success"
    assert items[4]["result"]["result"] == 6


@pytest.mark.asyncio
async def test_batch_execution_empty_inputs(stepflow_client, simple_workflow):
    """Test batch execution with empty input list."""
    response = await store_and_run_batch(
        stepflow_client, simple_workflow, [], timeout=30.0
    )

    assert response.summary.status == EXECUTION_STATUS_COMPLETED
    assert response.summary.items.total == 0

    items = get_result_items(response)
    assert len(items) == 0, f"Expected 0 results, got {len(items)}"


@pytest.mark.asyncio
async def test_single_input_execution(stepflow_client, simple_workflow):
    """Test that single input (non-batch) execution also works."""
    # Store the workflow
    store_response = await stepflow_client.store_flow(simple_workflow)
    assert store_response.flow_id, "Failed to store workflow"

    # Run with single input (dict, not list)
    response = await stepflow_client.run(
        flow_id=store_response.flow_id,
        input_data={"x": 7},
        timeout=60.0,
    )

    assert response.summary.status == EXECUTION_STATUS_COMPLETED, (
        f"Execution failed with status {response.summary.status}"
    )
    assert response.summary.items.total == 1

    items = get_result_items(response)
    assert len(items) == 1, f"Expected 1 result, got {len(items)}"
    assert items[0]["outcome"] == "success"
    assert items[0]["result"]["result"] == 14  # 7 * 2
