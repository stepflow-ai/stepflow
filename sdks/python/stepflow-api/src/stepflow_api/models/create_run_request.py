"""Custom CreateRunRequest that accepts flexible overrides format."""

from typing import Annotated, Any

from pydantic import BaseModel, Field

from .generated import BlobId, ValueRef


class CreateRunRequest(BaseModel):
    """Custom CreateRunRequest that accepts any overrides format."""

    debug: bool | None = None
    flowId: Annotated[BlobId, Field(description="The flow hash to execute")]
    input: Annotated[ValueRef, Field(description="Input data for the flow")]
    overrides: Any = (
        None  # Accept any format - will be serialized by WorkflowOverrides.to_dict()
    )
    variables: dict[str, ValueRef] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize with custom handling for overrides."""
        result: dict[str, Any] = {}

        if self.debug is not None:
            result["debug"] = self.debug

        # Handle flowId (BlobId is a RootModel)
        if hasattr(self.flowId, "root"):
            result["flowId"] = self.flowId.root
        else:
            result["flowId"] = self.flowId

        # Handle input (ValueRef is a RootModel)
        if hasattr(self.input, "root"):
            result["input"] = self.input.root
        else:
            result["input"] = self.input

        # Handle overrides with custom serialization
        if self.overrides is not None:
            if hasattr(self.overrides, "to_dict"):
                result["overrides"] = self.overrides.to_dict()
            else:
                result["overrides"] = self.overrides

        if self.variables is not None:
            result["variables"] = self.variables

        return result


__all__ = ["CreateRunRequest"]
