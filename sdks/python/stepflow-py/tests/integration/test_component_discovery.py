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

"""Integration tests for component discovery.

Verifies that the orchestrator can discover components from connected
workers via ListComponentsRequest tasks, and that the ComponentsService
API returns aggregated results from all plugins.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from stepflow_py import StepflowClient


@pytest.mark.asyncio
async def test_list_components_returns_builtins(
    stepflow_client: StepflowClient,
) -> None:
    """Builtin components should always be present."""
    response = await stepflow_client.list_components()
    component_paths = [c.component for c in response.components]

    # All plugins should succeed
    assert response.complete, (
        f"Expected complete=True, failed_plugins={response.failed_plugins}"
    )

    # Builtin plugin should provide at least these core components
    assert any("openai" in p for p in component_paths), (
        f"Expected an openai component in {component_paths}"
    )
    assert any("eval" in p for p in component_paths), (
        f"Expected an eval component in {component_paths}"
    )


@pytest.mark.asyncio
async def test_list_components_includes_python_worker(
    stepflow_client: StepflowClient,
) -> None:
    """Components from the Python HTTP worker should be discovered."""
    response = await stepflow_client.list_components()
    component_paths = [c.component for c in response.components]

    # The integration config routes /python/* to the Python worker.
    # The default StepflowServer includes builtins like "echo".
    # After route resolution, these appear as /python/echo etc.
    assert any("/python/" in p for p in component_paths), (
        f"Expected Python worker components (with /python/ prefix) in {component_paths}"
    )


@pytest.mark.asyncio
async def test_list_components_exclude_schemas(
    stepflow_client: StepflowClient,
) -> None:
    """When exclude_schemas=True, schemas should be omitted."""
    response = await stepflow_client.list_components(exclude_schemas=True)

    assert len(response.components) > 0, "Should have at least one component"

    for component in response.components:
        assert not component.HasField("input_schema"), (
            f"Component {component.component} should not have input_schema "
            f"when exclude_schemas=True"
        )
        assert not component.HasField("output_schema"), (
            f"Component {component.component} should not have output_schema "
            f"when exclude_schemas=True"
        )


@pytest.mark.asyncio
async def test_list_components_include_schemas(
    stepflow_client: StepflowClient,
) -> None:
    """When schemas are included (default), at least some should be present."""
    response = await stepflow_client.list_components(exclude_schemas=False)

    assert len(response.components) > 0, "Should have at least one component"

    # At least one component should have a schema defined
    has_schema = any(
        component.HasField("input_schema") or component.HasField("output_schema")
        for component in response.components
    )
    assert has_schema, (
        "At least one component should have a schema when schemas are included"
    )


@pytest.mark.asyncio
async def test_list_components_sorted(stepflow_client: StepflowClient) -> None:
    """Components should be returned sorted by path."""
    response = await stepflow_client.list_components()
    paths = [c.component for c in response.components]

    assert paths == sorted(paths), f"Components should be sorted, got: {paths}"
