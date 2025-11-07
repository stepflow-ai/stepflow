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

"""Runtime tweaks converter for Langflow-to-StepFlow integration.

This module provides utilities to convert Langflow tweaks into StepFlow 
WorkflowOverrides format, allowing tweaks to be applied at runtime rather 
than by modifying the workflow definition.

Key features:
- Convert Langflow tweaks to StepFlow override format
- Runtime application without workflow modification
- Compatible with StepFlow's override system
- Clean integration with StepFlow's native functionality

Architecture:
- Uses runtime overrides instead of workflow modification
- More efficient and cleaner separation of concerns
- Enables server-side override support (when implemented)
"""

import copy
from typing import Any

from stepflow_py import Flow


def convert_langflow_tweaks_to_overrides(
    flow: Flow,
    tweaks: dict[str, dict[str, Any]] | None = None,
    validate: bool = True,
) -> dict[str, Any]:
    """Convert Langflow tweaks to StepFlow WorkflowOverrides format.

    Args:
        flow: Stepflow workflow (for validation when requested)
        tweaks: Dict mapping langflow_node_id -> {field_name: new_value}
        validate: If True, validate that all tweaked components exist in the flow

    Returns:
        StepFlow WorkflowOverrides dict in format:
        {
            "stepOverrides": {
                "langflow_NodeId": {
                    "input.field_name": new_value,
                    ...
                }
            }
        }

    Raises:
        ValueError: If validate=True and tweaks reference non-existent components

    Examples:
        >>> tweaks = {
        ...     "LanguageModelComponent-kBOja": {
        ...         "api_key": "new_test_key", 
        ...         "temperature": 0.8,
        ...     }
        ... }
        >>> overrides = convert_langflow_tweaks_to_overrides(flow, tweaks)
        >>> # Returns:
        >>> # {
        >>> #   "stepOverrides": {
        >>> #     "langflow_LanguageModelComponent-kBOja": {
        >>> #       "input.api_key": "new_test_key",
        >>> #       "input.temperature": 0.8
        >>> #     }
        >>> #   }
        >>> # }
    """
    if not tweaks:
        return {"stepOverrides": {}}

    # Validate tweaks if requested
    if validate:
        _validate_langflow_tweaks(flow, tweaks)

    # Convert tweaks to StepFlow override format
    step_overrides = {}
    for langflow_node_id, node_tweaks in tweaks.items():
        # Convert langflow node ID to stepflow step ID
        stepflow_step_id = f"langflow_{langflow_node_id}"
        
        # Convert field tweaks to input path format
        step_override = {}
        for field_name, new_value in node_tweaks.items():
            # Langflow UDF executor components expect input fields under "input.field_name"
            override_path = f"input.{field_name}"
            step_override[override_path] = new_value
        
        step_overrides[stepflow_step_id] = step_override

    return {"stepOverrides": step_overrides}


def convert_langflow_tweaks_to_overrides_dict(
    workflow_dict: dict[str, Any],
    tweaks: dict[str, dict[str, Any]] | None = None,
    validate: bool = True,
) -> dict[str, Any]:
    """Convert Langflow tweaks to StepFlow WorkflowOverrides for dict workflows.

    Args:
        workflow_dict: Stepflow workflow as dict (from YAML)
        tweaks: Dict mapping langflow_node_id -> {field_name: new_value}
        validate: If True, validate that all tweaked components exist

    Returns:
        StepFlow WorkflowOverrides dict

    Examples:
        >>> tweaks = {"Component-123": {"api_key": "new_key"}}
        >>> overrides = convert_langflow_tweaks_to_overrides_dict(workflow_dict, tweaks)
    """
    if not tweaks:
        return {"stepOverrides": {}}

    # Validate tweaks if requested
    if validate:
        _validate_langflow_tweaks_dict(workflow_dict, tweaks)

    # Convert tweaks to StepFlow override format
    step_overrides = {}
    for langflow_node_id, node_tweaks in tweaks.items():
        # Convert langflow node ID to stepflow step ID  
        stepflow_step_id = f"langflow_{langflow_node_id}"
        
        # Convert field tweaks to input path format
        step_override = {}
        for field_name, new_value in node_tweaks.items():
            # Langflow UDF executor components expect input fields under "input.field_name"
            override_path = f"input.{field_name}"
            step_override[override_path] = new_value
        
        step_overrides[stepflow_step_id] = step_override

    return {"stepOverrides": step_overrides}


def _validate_langflow_tweaks(flow: Flow, tweaks: dict[str, dict[str, Any]]) -> None:
    """Validate that all tweaked components exist in the workflow.

    Args:
        flow: Stepflow workflow to validate against
        tweaks: Langflow tweaks to validate

    Raises:
        ValueError: If any tweaked component doesn't exist in the workflow
    """
    if not tweaks:
        return

    # Collect all available Langflow component IDs from the workflow
    available_components = set()
    for step in flow.steps or []:
        if _is_langflow_udf_step(step):
            langflow_node_id = _extract_langflow_node_id(step.id)
            if langflow_node_id:
                available_components.add(langflow_node_id)

    # Check for tweaks referencing non-existent components
    missing_components = set(tweaks.keys()) - available_components
    
    if missing_components:
        missing_list = sorted(missing_components)
        available_list = sorted(available_components)
        raise ValueError(
            f"Tweaks reference components that don't exist in the workflow:\n"
            f"  Missing components: {missing_list}\n"
            f"  Available components: {available_list}\n\n"
            f"This usually happens when:\n"
            f"  1. The workflow fixture was updated with new component IDs\n"
            f"  2. The component ID has a typo\n"
            f"  3. The wrong workflow is being tested\n\n"
            f"Solution: Update the component IDs in your test to match "
            f"the workflow."
        )


def _validate_langflow_tweaks_dict(
    workflow_dict: dict[str, Any], 
    tweaks: dict[str, dict[str, Any]]
) -> None:
    """Validate tweaks against a workflow dict.

    Args:
        workflow_dict: Stepflow workflow as dict
        tweaks: Langflow tweaks to validate

    Raises:
        ValueError: If any tweaked component doesn't exist in the workflow
    """
    if not tweaks:
        return

    # Collect all available Langflow component IDs from workflow dict
    available_components = set()
    for step_dict in workflow_dict.get("steps", []):
        step_id = step_dict.get("id", "")
        if _is_langflow_udf_step_dict(step_dict):
            langflow_node_id = _extract_langflow_node_id(step_id)
            if langflow_node_id:
                available_components.add(langflow_node_id)

    # Check for tweaks referencing non-existent components
    missing_components = set(tweaks.keys()) - available_components

    if missing_components:
        missing_list = sorted(missing_components)
        available_list = sorted(available_components)
        raise ValueError(
            f"Tweaks reference components that don't exist in the workflow:\n"
            f"  Missing components: {missing_list}\n"
            f"  Available components: {available_list}\n\n"
            f"This usually happens when:\n"
            f"  1. The workflow fixture was updated with new component IDs\n"
            f"  2. The component ID has a typo\n"
            f"  3. The wrong workflow is being tested\n\n"
            f"Solution: Update the component IDs in your test to match "
            f"the workflow."
        )


def _is_langflow_udf_step(step) -> bool:
    """Check if step is a Langflow UDF executor step.

    Args:
        step: Stepflow step to check

    Returns:
        True if this is a langflow UDF executor step
    """
    return (
        step.id.startswith("langflow_")
        and not step.id.endswith("_blob")
        and step.component == "/langflow/udf_executor"
    )


def _is_langflow_udf_step_dict(step_dict: dict[str, Any]) -> bool:
    """Check if step dict is a Langflow UDF executor step.

    Args:
        step_dict: Stepflow step dict to check

    Returns:
        True if this is a langflow UDF executor step
    """
    step_id = step_dict.get("id", "")
    return (
        step_id.startswith("langflow_")
        and not step_id.endswith("_blob")
        and step_dict.get("component") == "/langflow/udf_executor"
    )


def _extract_langflow_node_id(step_id: str) -> str | None:
    """Extract original Langflow node ID from Stepflow step ID.

    Args:
        step_id: Stepflow step ID (e.g., "langflow_LanguageModelComponent-kBOja")

    Returns:
        Original Langflow node ID (e.g., "LanguageModelComponent-kBOja") or None
    """
    if not step_id.startswith("langflow_"):
        return None

    # Extract the part after "langflow_": langflow_X -> X
    # Preserve original case for case-sensitive matching
    node_id = step_id[len("langflow_") :]
    return node_id



# Export main functions for easy importing
__all__ = [
    "convert_langflow_tweaks_to_overrides",
    "convert_langflow_tweaks_to_overrides_dict",
]
