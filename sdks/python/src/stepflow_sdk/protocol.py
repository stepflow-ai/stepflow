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

from msgspec import Struct, Raw, field
from typing import Any, Union, TypeAlias
import msgspec

class InitializeRequest(Struct, kw_only=True):
    runtime_protocol_version: int
    protocol_prefix: str

class InitializeResponse(Struct, kw_only=True):
    server_protocol_version: int

class Initialized(Struct, kw_only=True):
    pass

class ListComponentsRequest(Struct, kw_only=True):
    pass

class ListComponentsResponse(Struct, kw_only=True):
    components: list[str]

class ComponentInfoRequest(Struct, kw_only=True):
    component: str
    input_schema: dict[str, Any] = field(default_factory=dict)

class ComponentInfoResponse(Struct, kw_only=True):
    input_schema: dict[str, Any]
    output_schema: dict[str, Any]
    description: str | None = None

class ComponentExecuteRequest(Struct, kw_only=True):
    component: str
    input: msgspec.Raw

class ComponentExecuteResponse(Struct, kw_only=True):
    output: Any