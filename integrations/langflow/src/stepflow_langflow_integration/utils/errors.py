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

"""Custom exception types for Langflow integration."""


class LangflowIntegrationError(Exception):
    """Base exception for Langflow integration errors."""

    pass


class ConversionError(LangflowIntegrationError):
    """Error during Langflow to Stepflow conversion."""

    pass


class ValidationError(LangflowIntegrationError):
    """Error during workflow validation."""

    pass


class ExecutionError(LangflowIntegrationError):
    """Error during component execution."""

    pass
