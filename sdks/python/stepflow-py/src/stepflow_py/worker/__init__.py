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

from stepflow_py.api.models import (
    ErrorAction,
    Flow,
    FlowSchema,
    OnErrorDefault,
    OnErrorFail,
    OnErrorRetry,
    Step,
)

from .context import StepflowContext
from .expressions import ValueExpr
from .flow_builder import Component, FlowBuilder, StepHandle
from .server import StepflowServer
from .value import JsonPath, StepReference, Valuable, Value, WorkflowInput

__all__ = [
    # Core classes
    "StepflowServer",
    "StepflowContext",
    "FlowBuilder",
    # Value API for cleaner workflow definitions
    "Value",
    "Valuable",
    # Helper classes for type hints and intermediate objects
    "JsonPath",
    "StepHandle",
    "StepReference",
    "WorkflowInput",
    # Flow and workflow types (re-exported from API models)
    "Flow",
    "FlowSchema",
    "Step",
    "Component",
    # ValueExpr builder for $step, $input, $variable expressions
    "ValueExpr",
    # Error Action types
    "ErrorAction",
    "OnErrorFail",
    "OnErrorRetry",
    "OnErrorDefault",
]

# Add LangChain exports if available
# LangChain integration (optional)
try:
    from .langchain_integration import (
        InvokeNamedInput,  # noqa: F401
        clear_import_cache,  # noqa: F401
        create_invoke_named_component,  # noqa: F401
        get_runnable_from_import_path,  # noqa: F401
        invoke_named_runnable,  # noqa: F401
    )

    __all__.extend(
        [
            "get_runnable_from_import_path",
            "invoke_named_runnable",
            "clear_import_cache",
            "create_invoke_named_component",
            "InvokeNamedInput",
        ]
    )
except ImportError:
    pass

if __name__ == "__main__":
    from . import main

    main.main()
