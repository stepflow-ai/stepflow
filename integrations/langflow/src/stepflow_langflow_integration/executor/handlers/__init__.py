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

"""Input and output handlers for Langflow type transformations.

Input handlers dispatch on template field metadata and/or runtime value content
to transform parameter values before component execution.

Output handlers dispatch on Python type to serialize execution results back to
JSON-compatible formats with type markers.
"""

from .base import InputHandler, OutputHandler
from .base_model import BaseModelInputHandler, BaseModelOutputHandler
from .dataframe import DataFrameConversionInputHandler, DataFrameOutputHandler
from .langflow_types import LangflowTypeInputHandler, LangflowTypeOutputHandler
from .string_coercion import StringCoercionInputHandler
from .tool_wrapper import ToolWrapperInputHandler

__all__ = [
    "InputHandler",
    "OutputHandler",
    "BaseModelInputHandler",
    "BaseModelOutputHandler",
    "DataFrameConversionInputHandler",
    "DataFrameOutputHandler",
    "LangflowTypeInputHandler",
    "LangflowTypeOutputHandler",
    "StringCoercionInputHandler",
    "ToolWrapperInputHandler",
]
