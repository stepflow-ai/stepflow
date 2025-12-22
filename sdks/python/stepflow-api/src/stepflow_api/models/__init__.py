"""Generated models for the Stepflow API.

This module re-exports all generated models and adds compatibility
methods (to_dict, from_dict) for backwards compatibility.
"""

from __future__ import annotations

import sys
from typing import Any

# Import all from generated
from .generated import *  # noqa: F401, F403

# Collect all exported names for __all__
__all__: list[str] = []

# Get all classes from generated module
from . import generated

for name in dir(generated):
    obj = getattr(generated, name)
    if isinstance(obj, type):
        __all__.append(name)

# Re-export UNSET from types (add to __all__ first to satisfy linter)
__all__.extend(["UNSET", "Unset", "WorkflowOverrides"])
from ..types import UNSET, Unset  # noqa: E402, F401

# Try to import workflow_overrides if custom version exists
try:
    from .workflow_overrides import WorkflowOverrides  # noqa: E402, F401
except ImportError:
    pass


# Add to_dict and from_dict methods to all pydantic models for compatibility
def _add_compatibility_methods():
    """Add to_dict/from_dict methods to all exported models."""
    from pydantic import BaseModel

    module = sys.modules[__name__]

    for name in __all__:
        cls = getattr(module, name, None)
        if cls is None or not isinstance(cls, type):
            continue
        if not issubclass(cls, BaseModel):
            continue

        # Add to_dict method
        if not hasattr(cls, "to_dict"):

            def make_to_dict(c: type) -> Any:
                def to_dict(self: Any) -> dict[str, Any]:
                    return self.model_dump(mode="json", by_alias=True, exclude_unset=True)

                return to_dict

            cls.to_dict = make_to_dict(cls)

        # Add from_dict class method
        if not hasattr(cls, "from_dict"):

            def make_from_dict(c: type) -> Any:
                @classmethod
                def from_dict(cls: type, data: dict[str, Any]) -> Any:
                    return cls.model_validate(data)

                return from_dict

            cls.from_dict = make_from_dict(cls)


_add_compatibility_methods()
