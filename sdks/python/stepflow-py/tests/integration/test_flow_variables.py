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

"""Integration tests for flow variables endpoint.

Tests that storing a flow with env_var annotations and then fetching
variables via GET /flows/{id}/variables returns the correct env_vars mapping.
"""

from __future__ import annotations

import pytest

from stepflow_py import StepflowClient
from stepflow_py.worker import FlowBuilder, Value


def _build_flow_with_env_vars() -> dict:
    """Build a simple flow with env_var annotated variables."""
    builder = FlowBuilder(name="test_env_vars_flow")

    builder.set_variables_schema(
        {
            "type": "object",
            "properties": {
                "OPENAI_API_KEY": {
                    "type": ["string", "null"],
                    "default": None,
                    "env_var": "OPENAI_API_KEY",
                    "is_secret": True,
                },
                "temperature": {
                    "type": "number",
                    "default": 0.7,
                },
                "DB_URL": {
                    "type": "string",
                    "env_var": "DATABASE_URL",
                },
            },
        }
    )

    builder.add_step(
        id="test_step",
        component="/builtin/eval",
        input_data={"code": Value.literal("return 'ok'")},
    )
    builder.set_output(Value.step("test_step"))

    return builder.build()


@pytest.mark.asyncio(loop_scope="module")
async def test_get_flow_variables_roundtrip(
    stepflow_client: StepflowClient,
):
    """Store a flow with env_var annotations, verify get_flow_variables."""
    flow = _build_flow_with_env_vars()

    # Store the flow
    store_response = await stepflow_client.store_flow(flow)
    assert store_response.stored
    flow_id = store_response.flow_id

    # Fetch variables endpoint
    response = await stepflow_client._flow_api.get_flow_variables(flow_id)

    # Verify env_vars mapping
    assert response.flow_id == flow_id
    assert response.env_vars == {
        "OPENAI_API_KEY": "OPENAI_API_KEY",
        "DB_URL": "DATABASE_URL",
    }
    # temperature has no env_var annotation
    assert "temperature" not in response.env_vars

    # Verify schema is present
    assert response.var_schema is not None
    props = response.var_schema.get("properties", {})
    assert "OPENAI_API_KEY" in props
    assert "temperature" in props
    assert "DB_URL" in props
