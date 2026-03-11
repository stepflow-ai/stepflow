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

"""Test worker for gRPC pull transport end-to-end tests.

Registers multiple components for testing different aspects of the
gRPC pull transport. Used with `type: pull` plugin config.

Environment variables used:
  STEPFLOW_TASKS_URL       TasksService gRPC address (default: localhost:7837)
  STEPFLOW_QUEUE_NAME      Queue name from orchestrator config (default: python_grpc)
  STEPFLOW_MAX_CONCURRENT  Max concurrent task executions (default: 2)
"""

from stepflow_py.worker.context import StepflowContext
from stepflow_py.worker.main import main, server


@server.component(name="echo")
def echo(input: dict) -> dict:
    """Echo component that returns input with processed=true."""
    return {**input, "processed": True}


@server.component(name="error_test")
def error_test(input: dict) -> dict:
    """Component that raises errors based on input mode."""
    mode = input.get("mode", "success")
    message = input.get("message", "unknown error")

    if mode == "value_error":
        raise ValueError(f"Intentional error: {message}")
    elif mode == "runtime_error":
        raise RuntimeError(f"Runtime failure: {message}")

    return {"result": "success", "mode": mode}


@server.component(name="blob_roundtrip")
async def blob_roundtrip(input: dict, context: StepflowContext) -> dict:
    """Component that stores data as a blob and retrieves it back."""
    data = input.get("data", {})

    # Store as blob
    blob_id = await context.put_blob(data)

    # Retrieve it back
    retrieved = await context.get_blob(blob_id)

    return {
        "blob_id": blob_id,
        "retrieved": retrieved,
        "matches": retrieved == data,
    }


@server.component(name="transform")
def transform(input: dict) -> dict:
    """Component that transforms input data."""
    text = input.get("text", "")
    operation = input.get("operation", "upper")

    if operation == "upper":
        return {"result": text.upper()}
    elif operation == "lower":
        return {"result": text.lower()}
    elif operation == "reverse":
        return {"result": text[::-1]}
    else:
        return {"result": text}


@server.component(name="sub_run")
async def sub_run(input: dict, context: StepflowContext) -> dict:
    """Component that submits a sub-run using context.submit_run."""
    inner_flow = {
        "name": "inner_echo",
        "steps": [
            {
                "id": "inner_step",
                "component": "/builtin/create_messages",
                "input": {
                    "user_prompt": {"$input": "message"},
                },
            }
        ],
        "output": {"$step": "inner_step"},
    }

    result = await context.submit_run(
        flow=inner_flow,
        inputs=[{"message": input.get("message", "hello")}],
        wait=True,
        timeout_secs=30,
    )

    return {
        "sub_run_id": result.get("run_id", ""),
        "sub_run_status": result.get("status", ""),
        "has_results": len(result.get("results", [])) > 0,
    }


if __name__ == "__main__":
    main()
