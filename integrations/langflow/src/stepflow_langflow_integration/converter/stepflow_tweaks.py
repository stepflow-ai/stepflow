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

"""Stepflow-level tweaks for Langflow component configuration.

This module provides core utilities for applying tweaks to Stepflow workflows
that were translated from Langflow. Tweaks are applied by modifying the
input fields of Langflow UDF executor steps before execution.

Key features:
- Apply tweaks to translated Stepflow workflows at execution time
- Target UDF executor steps using original Langflow node IDs
- Modify step input fields directly (not blob data)
- Compatible with Langflow API tweak format

Note: Test utilities and environment variable integration are in
tests/helpers/tweaks_builder.py to keep this production code lean.
"""

import copy
from typing import Any

from stepflow_py import Flow


class StepflowTweaks:
    """Apply tweaks to Stepflow workflows translated from Langflow."""

    def __init__(self, tweaks: dict[str, dict[str, Any]] | None = None):
        """Initialize tweaks processor.

        Args:
            tweaks: Dict mapping langflow_node_id -> {field_name: new_value}
                   Node IDs should match original Langflow node IDs (case-insensitive)
        """
        self.tweaks = tweaks or {}
        # Normalize keys to lowercase for matching
        self.normalized_tweaks = {
            node_id.lower(): node_tweaks for node_id, node_tweaks in self.tweaks.items()
        }

    def apply_tweaks(self, flow: Flow) -> Flow:
        """Apply tweaks to a Stepflow workflow.

        Args:
            flow: Original Stepflow workflow (translated from Langflow)

        Returns:
            Modified workflow with tweaks applied

        Note:
            This modifies input fields for UDF executor steps that match
            Langflow patterns.
        """
        if not self.tweaks:
            return flow

        # Deep copy to avoid mutating original
        modified_flow = copy.deepcopy(flow)

        for step in modified_flow.steps:
            # Check if this is a Langflow UDF executor step
            if self._is_langflow_udf_step(step):
                langflow_node_id = self._extract_langflow_node_id(step.id)
                if langflow_node_id and langflow_node_id in self.normalized_tweaks:
                    self._apply_step_tweaks(
                        step, self.normalized_tweaks[langflow_node_id]
                    )

        return modified_flow

    def _is_langflow_udf_step(self, step) -> bool:
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

    def _extract_langflow_node_id(self, step_id: str) -> str | None:
        """Extract original Langflow node ID from Stepflow step ID.

        Args:
            step_id: Stepflow step ID (e.g., "langflow_languagemodelcomponent-kboja")

        Returns:
            Original Langflow node ID (e.g., "languagemodelcomponent-kboja") or None
        """
        if not step_id.startswith("langflow_"):
            return None

        # Extract the part after "langflow_": langflow_X -> X
        node_id = step_id[len("langflow_") :]
        return node_id.lower()

    def _apply_step_tweaks(self, step, step_tweaks: dict[str, Any]) -> None:
        """Apply tweaks to a specific Stepflow UDF executor step.

        Args:
            step: Stepflow step to modify
            step_tweaks: Dict of field_name -> new_value for this step
        """
        # Ensure step.input exists and has an 'input' section
        if not hasattr(step, "input") or step.input is None:
            step.input = {}

        if "input" not in step.input:
            step.input["input"] = {}

        # Apply each tweak as a direct field override
        for field_name, new_value in step_tweaks.items():
            step.input["input"][field_name] = new_value


def apply_stepflow_tweaks(
    flow: Flow,
    tweaks: dict[str, dict[str, Any]] | None = None,
) -> Flow:
    """Apply tweaks to a Stepflow workflow (convenience function).

    Args:
        flow: Original Stepflow workflow (translated from Langflow)
        tweaks: Dict mapping langflow_node_id -> {field_name: new_value}

    Returns:
        Modified workflow with tweaks applied

    Examples:
        >>> tweaks = {
        ...     "LanguageModelComponent-kBOja": {
        ...         "api_key": "new_test_key",
        ...         "temperature": 0.8,
        ...     }
        ... }
        >>> modified_flow = apply_stepflow_tweaks(flow, tweaks)
    """
    stepflow_tweaks = StepflowTweaks(tweaks)
    return stepflow_tweaks.apply_tweaks(flow)


def apply_stepflow_tweaks_to_dict(
    workflow_dict: dict[str, Any],
    tweaks: dict[str, dict[str, Any]] | None = None,
) -> dict[str, Any]:
    """Apply tweaks to a Stepflow workflow dict (for YAML processing).

    Args:
        workflow_dict: Stepflow workflow as dict (from YAML)
        tweaks: Dict mapping langflow_node_id -> {field_name: new_value}

    Returns:
        Modified workflow dict with tweaks applied

    Examples:
        >>> tweaks = {
        ...     "LanguageModelComponent-kBOja": {
        ...         "api_key": "new_test_key",
        ...         "temperature": 0.8,
        ...     }
        ... }
        >>> modified_dict = apply_stepflow_tweaks_to_dict(workflow_dict, tweaks)
    """
    if not tweaks:
        return workflow_dict

    # Deep copy to avoid mutating original
    modified_dict = copy.deepcopy(workflow_dict)

    # Normalize tweaks keys to lowercase
    normalized_tweaks = {
        node_id.lower(): node_tweaks for node_id, node_tweaks in tweaks.items()
    }

    # Apply tweaks to steps
    for step_dict in modified_dict.get("steps", []):
        step_id = step_dict.get("id", "")

        # Check if this is a Langflow UDF executor step
        if (
            step_id.startswith("langflow_")
            and not step_id.endswith("_blob")
            and step_dict.get("component") == "/langflow/udf_executor"
        ):
            # Extract Langflow node ID
            langflow_node_id = step_id[len("langflow_") :].lower()

            if langflow_node_id in normalized_tweaks:
                # Ensure step has input section
                if "input" not in step_dict:
                    step_dict["input"] = {}
                if "input" not in step_dict["input"]:
                    step_dict["input"]["input"] = {}

                # Apply tweaks
                for field_name, new_value in normalized_tweaks[
                    langflow_node_id
                ].items():
                    step_dict["input"]["input"][field_name] = new_value

    return modified_dict


# Export main classes and functions for easy importing
__all__ = [
    "StepflowTweaks",
    "apply_stepflow_tweaks",
    "apply_stepflow_tweaks_to_dict",
]
