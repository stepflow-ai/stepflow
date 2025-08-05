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
#   uv run python generate.py

from __future__ import annotations

from enum import Enum
from typing import Annotated, Any, ClassVar, List, Literal

from msgspec import Meta, Struct

JsonRpc = Annotated[
    Literal['2.0'], Meta(description='The version of the JSON-RPC protocol.')
]


RequestId = Annotated[
    str | int,
    Meta(
        description='The identifier for a JSON-RPC request. Can be either a string or an integer.\nThe RequestId is used to match method responses to corresponding requests.\nIt should not be set on notifications.'
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
    flows_evaluate = 'flows/evaluate'


class InitializeParams(Struct, kw_only=True):
    runtime_protocol_version: Annotated[
        int,
        Meta(
            description='Maximum version of the protocol being used by the StepFlow runtime.',
            ge=0,
        ),
    ]


Component = Annotated[
    str,
    Meta(
        description="Identifies a specific plugin and atomic functionality to execute. Use component name for builtins (e.g., 'eval') or path format for plugins (e.g., '/python/udf').",
        examples=['/builtin/eval', '/mcpfs/list_files', '/python/udf'],
    ),
]


Value = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


class ComponentInfoParams(Struct, kw_only=True):
    component: Annotated[
        Component, Meta(description='The component to get information about.')
    ]


class ComponentListParams(Struct, kw_only=True):
    pass


BlobId = Annotated[
    str,
    Meta(
        description='A SHA-256 hash of the blob content, represented as a hexadecimal string.'
    ),
]


class BlobType(Enum):
    flow = 'flow'
    data = 'data'


class EvaluateFlowParams(Struct, kw_only=True):
    flow_id: Annotated[
        BlobId,
        Meta(description='The ID of the flow to evaluate (blob ID of the flow).'),
    ]
    input: Annotated[Value, Meta(description='The input to provide to the flow.')]


class InitializeResult(Struct, kw_only=True):
    server_protocol_version: Annotated[
        int,
        Meta(
            description='Version of the protocol being used by the component server.',
            ge=0,
        ),
    ]


class ComponentExecuteResult(Struct, kw_only=True):
    output: Annotated[Value, Meta(description='The result of the component execution.')]


class Schema(Struct, kw_only=True):
    pass


class GetBlobResult(Struct, kw_only=True):
    data: Value
    blob_type: BlobType


class PutBlobResult(Struct, kw_only=True):
    blob_id: BlobId


class FlowError(Struct, kw_only=True):
    code: int
    message: str
    data: Value | None = None


class FlowResultSuccess(Struct, kw_only=True, tag_field='outcome', tag='success'):
    outcome: ClassVar[Annotated[Literal['success'], Meta(title='FlowOutcome')]]
    result: Value


class FlowResultSkipped(Struct, kw_only=True, tag_field='outcome', tag='skipped'):
    outcome: ClassVar[Annotated[Literal['skipped'], Meta(title='FlowOutcome')]]


class FlowResultFailed(Struct, kw_only=True, tag_field='outcome', tag='failed'):
    outcome: ClassVar[Annotated[Literal['failed'], Meta(title='FlowOutcome')]]
    error: FlowError


class Error(Struct, kw_only=True):
    code: Annotated[int, Meta(description='A numeric code indicating the error type.')]
    message: Annotated[
        str, Meta(description='Concise, single-sentence description of the error.')
    ]
    data: (
        Annotated[
            Value | None,
            Meta(
                description='Primitive or structured value that contains additional information about the error.'
            ),
        ]
        | None
    ) = None


class Initialized(Struct, kw_only=True):
    pass


class ComponentExecuteParams(Struct, kw_only=True):
    component: Annotated[Component, Meta(description='The component to execute.')]
    input: Annotated[Value, Meta(description='The input to the component.')]


class GetBlobParams(Struct, kw_only=True):
    blob_id: Annotated[BlobId, Meta(description='The ID of the blob to retrieve.')]


class PutBlobParams(Struct, kw_only=True):
    data: Value
    blob_type: BlobType


class ComponentInfo(Struct, kw_only=True):
    component: Annotated[Component, Meta(description='The component ID.')]
    description: (
        Annotated[
            str | None, Meta(description='Optional description of the component.')
        ]
        | None
    ) = None
    input_schema: (
        Annotated[
            Schema | None,
            Meta(
                description='The input schema for the component.\n\nCan be any valid JSON schema (object, primitive, array, etc.).'
            ),
        ]
        | None
    ) = None
    output_schema: (
        Annotated[
            Schema | None,
            Meta(
                description='The output schema for the component.\n\nCan be any valid JSON schema (object, primitive, array, etc.).'
            ),
        ]
        | None
    ) = None


class ListComponentsResult(Struct, kw_only=True):
    components: Annotated[
        List[ComponentInfo], Meta(description='A list of all available components.')
    ]


FlowResult = Annotated[
    FlowResultSuccess | FlowResultSkipped | FlowResultFailed,
    Meta(description='The results of a step execution.', title='FlowResult'),
]


class MethodError(Struct, kw_only=True):
    id: RequestId
    error: Annotated[
        Error, Meta(description='An error that occurred during method execution.')
    ]
    jsonrpc: JsonRpc | None = '2.0'


class Notification(Struct, kw_only=True):
    method: Annotated[Method, Meta(description='The notification method being called.')]
    params: Annotated[
        Initialized,
        Meta(
            description='The parameters for the notification.',
            title='NotificationParams',
        ),
    ]
    jsonrpc: JsonRpc | None = '2.0'


class MethodRequest(Struct, kw_only=True):
    id: RequestId
    method: Annotated[Method, Meta(description='The method being called.')]
    params: Annotated[
        InitializeParams
        | ComponentExecuteParams
        | ComponentInfoParams
        | ComponentListParams
        | GetBlobParams
        | PutBlobParams
        | EvaluateFlowParams,
        Meta(
            description='The parameters for the method call. Set on method requests.',
            title='MethodParams',
        ),
    ]
    jsonrpc: JsonRpc | None = '2.0'


class ComponentInfoResult(Struct, kw_only=True):
    info: Annotated[ComponentInfo, Meta(description='Information about the component.')]


class EvaluateFlowResult(Struct, kw_only=True):
    result: Annotated[
        FlowResult, Meta(description='The result of the flow evaluation.')
    ]


class MethodSuccess(Struct, kw_only=True):
    id: RequestId
    result: Annotated[
        InitializeResult
        | ComponentExecuteResult
        | ComponentInfoResult
        | ListComponentsResult
        | GetBlobResult
        | PutBlobResult
        | EvaluateFlowResult,
        Meta(
            description='The result of a successful method execution.',
            title='MethodResult',
        ),
    ]
    jsonrpc: JsonRpc | None = '2.0'


Message = Annotated[
    MethodRequest | MethodSuccess | MethodError | Notification,
    Meta(
        description='The messages supported by the StepFlow protocol. These correspond to JSON-RPC 2.0 messages.\n\nNote that this defines a superset containing both client-sent and server-sent messages.',
        title='Message',
    ),
]


MethodResponse = Annotated[
    MethodSuccess | MethodError, Meta(description='Response to a method request.')
]
