"""Custom WorkflowOverrides with transparent serialization.

The server uses #[serde(transparent)] for WorkflowOverrides, meaning it expects
the step map directly without the "steps" wrapper.
"""

from typing import Annotated, Any

from pydantic import BaseModel, Field

from .generated import StepOverride


class WorkflowOverrides(BaseModel):
    """Workflow overrides with transparent serialization.

    When serialized, returns just the step map without the "steps" wrapper
    to match the server's serde(transparent) expectation.
    """

    steps: Annotated[
        dict[str, StepOverride],
        Field(description="Map of step ID to override specification"),
    ]

    def to_dict(self) -> dict[str, Any]:
        """Serialize to dict with transparent representation.

        Returns just the step map, not {"steps": ...}, because the server
        uses serde(transparent).
        """
        return {
            step_id: override.model_dump(mode="json", by_alias=True, exclude_unset=True)
            for step_id, override in self.steps.items()
        }


__all__ = ["WorkflowOverrides"]
