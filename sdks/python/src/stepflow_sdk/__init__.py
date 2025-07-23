# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

from .context import StepflowContext
from .flow_builder import FlowBuilder, StepHandle
from .generated_protocol import (
    ErrorAction,
    OnErrorDefault,
    OnErrorFail,
    OnErrorRetry,
    OnErrorSkip,
    OnSkipDefault,
    OnSkipSkip,
    SkipAction,
)
from .stdio_server import StepflowStdioServer
from .value import JsonPath, StepReference, Valuable, Value, WorkflowInput

__all__ = [
    # Core classes
    "StepflowStdioServer",
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
    # Error and Skip Action types
    "ErrorAction",
    "OnErrorFail",
    "OnErrorSkip",
    "OnErrorRetry",
    "OnErrorDefault",
    "SkipAction",
    "OnSkipSkip",
    "OnSkipDefault",
]

if __name__ == "__main__":
    from . import main

    main.main()
