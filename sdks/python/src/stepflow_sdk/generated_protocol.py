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

# Auto-generated protocol types from schemas/protocol.json
# To regenerate this file, run:
#   uv run generate-protocol

from __future__ import annotations

from enum import Enum
from typing import Annotated, Any, Dict, List, Literal

from msgspec import Meta, Struct

JsonRpc = Annotated[
    Literal['2.0'],
    Meta(description='The version of the JSON-RPC protocol.', title='JSON-RPC Version'),
]


RequestId = Annotated[
    str | int,
    Meta(
        description='The identifier for a JSON-RPC request. Can be either a string or an integer.\nThe RequestId is used to match method responses to corresponding requests.\nIt should not be set on notifications.',
        title='JSON-RPC Request ID.',
    ),
]


class Method(Enum):
    initialize = 'initialize'
    initialized = 'initialized'
    components_list = 'components/list'
    components_info = 'components/info'
    components_execute = 'components/execute'
    blobs_put = 'blobs/put'
    blobs_get = 'blobs/get'


class InitializeParams(Struct):
    runtime_protocol_version: Annotated[
        int,
        Meta(
            description='Maximum version of the protocol being used by the StepFlow runtime.',
            ge=0,
        ),
    ]
    protocol_prefix: Annotated[
        str,
        Meta(
            description='The protocol prefix for components served by this plugin (e.g., "python", "typescript")'
        ),
    ]


Component = Annotated[
    str,
    Meta(
        description='Identifies a specific plugin and atomic functionality to execute.'
    ),
]


Value = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


class ComponentInfoParams(Struct):
    component: Annotated[
        Component, Meta(description='The component to get information about.')
    ]


class ComponentListParams(Struct):
    pass


BlobId = Annotated[
    str,
    Meta(
        description='A SHA-256 hash of the blob content, represented as a hexadecimal string.',
        title='Blob ID',
    ),
]


class PutBlobParams(Struct):
    data: Value


class InitializeResult(Struct):
    server_protocol_version: Annotated[
        int,
        Meta(
            description='Version of the protocol being used by the component server.',
            ge=0,
        ),
    ]


class ComponentExecuteResult(Struct):
    output: Annotated[Value, Meta(description='The result of the component execution.')]


Schema = Dict[str, Any] | bool


class GetBlobResult(Struct):
    data: Value


class PutBlobResult(Struct):
    blob_id: BlobId


class Error(Struct):
    code: Annotated[
        int,
        Meta(
            description='A numeric code indicating the error type.', title='Error code'
        ),
    ]
    message: Annotated[
        str,
        Meta(
            description='Concise, single-sentence description of the error.',
            title='Error message',
        ),
    ]
    data: (
        Annotated[
            Value | None,
            Meta(
                description='Primitive or structured value that contains additional information about the error.',
                title='Error data',
            ),
        ]
        | None
    ) = None


class Initialized(Struct):
    pass


class ComponentExecuteParams(Struct):
    component: Annotated[Component, Meta(description='The component to execute.')]
    input: Annotated[Value, Meta(description='The input to the component.')]


class GetBlobParams(Struct):
    blob_id: Annotated[BlobId, Meta(description='The ID of the blob to retrieve.')]


SchemaRef = Annotated[
    Schema, Meta(description='Type alias for a shared schema reference.')
]


class MethodError(Struct):
    id: RequestId
    error: Annotated[
        Error, Meta(description='An error that occurred during method execution.')
    ]
    jsonrpc: JsonRpc | None = '2.0'


class Notification(Struct):
    method: Annotated[
        Method,
        Meta(description='The notification method being called.', title='Method'),
    ]
    params: Annotated[
        Initialized,
        Meta(description='The parameters for the notification.', title='Parameters'),
    ]
    jsonrpc: JsonRpc | None = '2.0'


class MethodRequest(Struct):
    id: RequestId
    method: Annotated[
        Method, Meta(description='The method being called.', title='Method')
    ]
    params: Annotated[
        InitializeParams
        | ComponentExecuteParams
        | ComponentInfoParams
        | ComponentListParams
        | GetBlobParams
        | PutBlobParams,
        Meta(
            description='The parameters for the method call. Set on method requests.',
            title='Parameters',
        ),
    ]
    jsonrpc: JsonRpc | None = '2.0'


class ComponentInfo(Struct):
    component: Annotated[Component, Meta(description='The component ID.')]
    description: (
        Annotated[
            str | None, Meta(description='Optional description of the component.')
        ]
        | None
    ) = None
    input_schema: (
        Annotated[
            SchemaRef | None,
            Meta(
                description='The input schema for the component.\n\nCan be any valid JSON schema (object, primitive, array, etc.).'
            ),
        ]
        | None
    ) = None
    output_schema: (
        Annotated[
            SchemaRef | None,
            Meta(
                description='The output schema for the component.\n\nCan be any valid JSON schema (object, primitive, array, etc.).'
            ),
        ]
        | None
    ) = None


class ListComponentsResult(Struct):
    components: Annotated[
        List[ComponentInfo], Meta(description='A list of all available components.')
    ]


class ComponentInfoResult(Struct):
    info: Annotated[ComponentInfo, Meta(description='Information about the component.')]


class MethodSuccess(Struct):
    id: RequestId
    result: Annotated[
        InitializeResult
        | ComponentExecuteResult
        | ComponentInfoResult
        | ListComponentsResult
        | GetBlobResult
        | PutBlobResult,
        Meta(
            description='The result of a successful method execution.',
            title='Method Result',
        ),
    ]
    jsonrpc: JsonRpc | None = '2.0'


MethodResponse = Annotated[
    MethodSuccess | MethodError, Meta(description='Response to a method request.')
]


Message = Annotated[
    MethodRequest | MethodResponse | Notification,
    Meta(
        description='The messages supported by the StepFlow protocol. These correspond to JSON-RPC 2.0 messages.\n\nNote that this defines a superset containing both client-sent and server-sent messages.',
        title='Message',
    ),
]
