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

"""Integration tests for batch execution functionality."""

import json
import tempfile
from pathlib import Path

import pytest
import yaml


@pytest.fixture(scope="module")
def batch_config():
    """Create configuration for batch execution tests."""

    # Get path to Python SDK for UDF component
    current_dir = Path(__file__).parent.parent.parent.parent.parent
    python_sdk_path = current_dir / "sdks" / "python"

    config_dict = {
        "plugins": {
            "builtin": {"type": "builtin"},
            "python": {
                "type": "stepflow",
                "transport": "stdio",
                "command": "uv",
                "args": [
                    "--project",
                    str(python_sdk_path),
                    "run",
                    "stepflow_py",
                ],
            },
        },
        "routes": {
            "/builtin/{*component}": [{"plugin": "builtin"}],
            "/python/{*component}": [{"plugin": "python"}],
        },
    }

    # Write config to temporary file (persists for module scope)
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yml", delete=False
    ) as config_file:
        yaml.dump(config_dict, config_file, default_flow_style=False, sort_keys=False)
        config_path = Path(config_file.name)

    yield str(config_path)

    # Cleanup after all tests in module complete
    config_path.unlink(missing_ok=True)


@pytest.fixture(scope="module")
def batch_server(stepflow_runner, batch_config):
    """Start a shared Stepflow server for batch execution tests."""
    server = stepflow_runner.start_server(config_path=batch_config, timeout=60.0)
    yield server
    server.stop()


@pytest.fixture
def simple_workflow_yaml():
    """Create a simple workflow YAML for batch testing using Python UDF.

    This creates a workflow that doubles an input number using a Python UDF.
    """
    workflow = {
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
    return yaml.dump(workflow, default_flow_style=False, sort_keys=False)


@pytest.fixture
def batch_inputs_jsonl(tmp_path):
    """Create JSONL file with batch inputs."""
    inputs = [
        {"x": 1},
        {"x": 2},
        {"x": 3},
        {"x": 5},
        {"x": 8},
    ]

    inputs_file = tmp_path / "batch_inputs.jsonl"
    with open(inputs_file, "w") as f:
        for input_data in inputs:
            f.write(json.dumps(input_data) + "\n")

    return str(inputs_file)


def test_batch_execution_basic(
    stepflow_runner, batch_server, simple_workflow_yaml, batch_inputs_jsonl
):
    """Test basic batch execution with multiple inputs."""
    success, batch_stats, stdout, stderr = stepflow_runner.submit_batch(
        server=batch_server,
        workflow_yaml=simple_workflow_yaml,
        inputs_jsonl_path=batch_inputs_jsonl,
        timeout=120.0,
    )

    assert success, f"Batch execution failed:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    assert batch_stats["total"] == 5, "Expected 5 total executions"
    assert batch_stats["completed"] == 5, "Expected all 5 to complete"
    assert batch_stats["failed"] == 0, "Expected no failures"
    assert batch_stats["batch_id"] is not None, "Expected batch ID"


def test_batch_execution_with_max_concurrent(
    stepflow_runner, batch_server, simple_workflow_yaml, batch_inputs_jsonl
):
    """Test batch execution with max_concurrent limit."""
    success, batch_stats, stdout, stderr = stepflow_runner.submit_batch(
        server=batch_server,
        workflow_yaml=simple_workflow_yaml,
        inputs_jsonl_path=batch_inputs_jsonl,
        max_concurrent=2,
        timeout=120.0,
    )

    assert success, f"Batch execution failed:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    assert batch_stats["total"] == 5, "Expected 5 total executions"
    assert batch_stats["completed"] == 5, "Expected all 5 to complete"
    assert batch_stats["failed"] == 0, "Expected no failures"


def test_batch_execution_with_output_file(
    stepflow_runner,
    batch_server,
    simple_workflow_yaml,
    batch_inputs_jsonl,
    tmp_path,
):
    """Test batch execution with output file specified and verify actual results."""
    output_file = tmp_path / "batch_output.jsonl"

    success, batch_stats, stdout, stderr = stepflow_runner.submit_batch(
        server=batch_server,
        workflow_yaml=simple_workflow_yaml,
        inputs_jsonl_path=batch_inputs_jsonl,
        output_path=str(output_file),
        timeout=120.0,
    )

    assert success, f"Batch execution failed:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    assert batch_stats["completed"] == 5, "Expected all 5 to complete"

    # Verify output file exists and has correct number of results
    assert output_file.exists(), "Output file should exist"

    with open(output_file) as f:
        results = [json.loads(line) for line in f]

    assert len(results) == 5, "Expected 5 results in output file"

    # Expected inputs and outputs: [1, 2, 3, 5, 8] -> [2, 4, 6, 10, 16]
    expected_outputs = [2, 4, 6, 10, 16]

    # Verify results have expected structure and correct values
    # Batch results have format:
    #   {"index": N, "status": "completed/failed", "result": {...}}
    for i, result in enumerate(results):
        assert "status" in result, "Result should have status field"
        assert result["status"] in [
            "completed",
            "failed",
        ], "Status should be completed or failed"
        assert "index" in result, "Result should have index field"
        assert "result" in result, "Result should have result field"

        if result["status"] == "completed":
            # Check that result has the workflow execution result
            assert result["result"].get("outcome") == "success"

            # Verify the actual computed value is correct (x * 2)
            # The workflow output is result["result"]["result"]["result"]
            # where the first "result" is the batch result field,
            # the second "result" is the outcome wrapper,
            # and the third "result" is the workflow output field name
            workflow_output = result["result"]["result"]
            assert workflow_output is not None, f"Workflow result is None at index {i}"
            actual_result = workflow_output["result"]
            expected_result = expected_outputs[i]
            assert actual_result == expected_result, (
                f"Index {i}: Expected {expected_result}, got {actual_result}"
            )


def test_batch_execution_with_failures(
    stepflow_runner, batch_server, tmp_path, simple_workflow_yaml
):
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

    inputs_file = tmp_path / "mixed_inputs.jsonl"
    with open(inputs_file, "w") as f:
        for input_data in inputs:
            f.write(json.dumps(input_data) + "\n")

    output_file = tmp_path / "mixed_output.jsonl"

    _success, batch_stats, _stdout, _stderr = stepflow_runner.submit_batch(
        server=batch_server,
        workflow_yaml=simple_workflow_yaml,
        inputs_jsonl_path=str(inputs_file),
        output_path=str(output_file),
        timeout=120.0,
    )

    # Batch command returns non-zero exit code when there are failures,
    # but we still get results
    assert batch_stats["total"] == 5, "Expected 5 total executions"
    assert batch_stats["completed"] == 3, "Expected 3 successful executions"
    assert batch_stats["failed"] == 2, "Expected 2 failed executions"

    # Verify output file contains results for all inputs
    assert output_file.exists(), "Output file should exist"

    with open(output_file) as f:
        results = [json.loads(line) for line in f]

    assert len(results) == 5, "Expected 5 results (including failures)"

    # Count completed vs failed
    completed_count = sum(1 for r in results if r["status"] == "completed")
    failed_count = sum(1 for r in results if r["status"] == "failed")

    assert completed_count == batch_stats["completed"], "Completed count mismatch"
    assert failed_count == batch_stats["failed"], "Failed count mismatch"

    # Verify specific results
    # Index 0: x=1, should succeed with result=2
    assert results[0]["status"] == "completed"
    assert results[0]["result"]["result"]["result"] == 2

    # Index 1: x missing, should fail
    assert results[1]["status"] == "failed"

    # Index 2: x=2, should succeed with result=4
    assert results[2]["status"] == "completed"
    assert results[2]["result"]["result"]["result"] == 4

    # Index 3: x="not_a_number", should fail
    assert results[3]["status"] == "failed"

    # Index 4: x=3, should succeed with result=6
    assert results[4]["status"] == "completed"
    assert results[4]["result"]["result"]["result"] == 6


def test_batch_execution_empty_inputs(
    stepflow_runner, batch_server, simple_workflow_yaml, tmp_path
):
    """Test batch execution with empty input file."""
    empty_inputs_file = tmp_path / "empty_inputs.jsonl"
    empty_inputs_file.write_text("")

    success, batch_stats, stdout, stderr = stepflow_runner.submit_batch(
        server=batch_server,
        workflow_yaml=simple_workflow_yaml,
        inputs_jsonl_path=str(empty_inputs_file),
        timeout=30.0,
    )

    assert success, f"Batch execution failed:\nSTDOUT:\n{stdout}\nSTDERR:\n{stderr}"
    assert batch_stats["total"] == 0, "Expected 0 total executions"
    assert batch_stats["completed"] == 0, "Expected 0 completed"
    assert batch_stats["failed"] == 0, "Expected 0 failed"
