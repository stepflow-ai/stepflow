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

"""Field handlers for template-field-based value transformations.

Field handlers dispatch on Langflow template field metadata (e.g., ``type``,
``input_types``, ``load_from_db``) to transform parameter values after
deserialization but before component execution.

Each handler declares which fields it matches and can batch-process all matched
fields, enabling parallel I/O (e.g., concurrent blob downloads for a future
FileFieldHandler).
"""

from .base import FieldHandler
from .dataframe import DataFrameFieldHandler
from .env_var import EnvVarFieldHandler
from .string_coercion import StringCoercionFieldHandler

__all__ = [
    "FieldHandler",
    "DataFrameFieldHandler",
    "EnvVarFieldHandler",
    "StringCoercionFieldHandler",
]
