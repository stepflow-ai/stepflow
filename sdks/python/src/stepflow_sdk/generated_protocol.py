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
from typing import Annotated, Any, ClassVar, Dict, List, Literal

from msgspec import Meta, Struct, field

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
    protocol_prefix: Annotated[
        str,
        Meta(
            description='The protocol prefix for components served by this plugin (e.g., "python", "typescript")'
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


class PutBlobParams(Struct, kw_only=True):
    data: Value


class Schema(Struct, kw_only=True):
    pass


class EscapedLiteral(Struct, kw_only=True):
    field_literal: Annotated[
        Value,
        Meta(
            description='A literal value that should not be expanded for expressions.\nThis allows creating JSON values that contain `$from` without expansion.'
        ),
    ] = field(name='$literal')


class StepReference(Struct, kw_only=True):
    step: str


class WorkflowRef(Enum):
    input = 'input'


JsonPath = Annotated[
    str,
    Meta(
        description='JSON path expression to apply to the referenced value. May use `$` to reference the whole value. May also be a bare field name (without the leading $) if the referenced value is an object.',
        examples=['field', '$.field', '$["field"]', '$[0]', '$.field[0].nested'],
    ),
]


class OnSkipSkip(Struct, kw_only=True):
    action: Literal['skip']


class OnSkipDefault(Struct, kw_only=True):
    action: Literal['useDefault']
    defaultValue: Value | None = None


SkipAction = OnSkipSkip | OnSkipDefault


class OnErrorFail(Struct, kw_only=True):
    action: Literal['fail']


class OnErrorSkip(Struct, kw_only=True):
    action: Literal['skip']


class OnErrorRetry(Struct, kw_only=True):
    action: Literal['retry']


class TestServerHealthCheck(Struct, kw_only=True):
    path: Annotated[
        str, Meta(description='Path for health check endpoint (e.g., "/health").')
    ]
    timeoutMs: (
        Annotated[
            int,
            Meta(
                description='Timeout for health check requests (in milliseconds).', ge=0
            ),
        ]
        | None
    ) = 5000
    retryAttempts: (
        Annotated[
            int, Meta(description='Number of retry attempts for health checks.', ge=0)
        ]
        | None
    ) = 3
    retryDelayMs: (
        Annotated[
            int,
            Meta(description='Delay between retry attempts (in milliseconds).', ge=0),
        ]
        | None
    ) = 1000


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


class ExampleInput(Struct, kw_only=True):
    name: Annotated[
        str, Meta(description='Name of the example input for display purposes.')
    ]
    input: Annotated[Value, Meta(description='The input data for this example.')]
    description: (
        Annotated[
            str | None,
            Meta(description='Optional description of what this example demonstrates.'),
        ]
        | None
    ) = None


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


class GetBlobResult(Struct, kw_only=True):
    data: Value


class PutBlobResult(Struct, kw_only=True):
    blob_id: BlobId


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


class WorkflowReference(Struct, kw_only=True):
    workflow: WorkflowRef


BaseRef = Annotated[
    WorkflowReference | StepReference,
    Meta(
        description='An expression that can be either a literal value or a template expression.'
    ),
]


class TestServerConfig(Struct, kw_only=True):
    command: Annotated[str, Meta(description='Command to start the server.')]
    args: (
        Annotated[List[str], Meta(description='Arguments for the server command.')]
        | None
    ) = None
    env: (
        Annotated[
            Dict[str, str],
            Meta(
                description='Environment variables for the server process.\nValues can contain placeholders like {port} which will be substituted.'
            ),
        ]
        | None
    ) = None
    workingDirectory: (
        Annotated[
            str | None, Meta(description='Working directory for the server process.')
        ]
        | None
    ) = None
    portRange: (
        Annotated[
            List[Any] | None,
            Meta(
                description='Port range for automatic port allocation.\nIf not specified, a random available port will be used.'
            ),
        ]
        | None
    ) = None
    healthCheck: (
        Annotated[
            TestServerHealthCheck | None,
            Meta(description='Health check configuration.'),
        ]
        | None
    ) = None
    startupTimeoutMs: (
        Annotated[
            int,
            Meta(
                description='Maximum time to wait for server startup (in milliseconds).',
                ge=0,
            ),
        ]
        | None
    ) = 10000
    shutdownTimeoutMs: (
        Annotated[
            int,
            Meta(
                description='Maximum time to wait for server shutdown (in milliseconds).',
                ge=0,
            ),
        ]
        | None
    ) = 5000


FlowResult = Annotated[
    FlowResultSuccess | FlowResultSkipped | FlowResultFailed,
    Meta(description='The results of a step execution.', title='FlowResult'),
]


class ComponentInfoResult(Struct, kw_only=True):
    info: Annotated[ComponentInfo, Meta(description='Information about the component.')]


class EvaluateFlowResult(Struct, kw_only=True):
    result: Annotated[
        FlowResult, Meta(description='The result of the flow evaluation.')
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


class Reference(Struct, kw_only=True):
    field_from: Annotated[BaseRef, Meta(description='The source of the reference.')] = (
        field(name='$from')
    )
    path: (
        Annotated[
            JsonPath,
            Meta(
                description='JSON path expression to apply to the referenced value.\n\nDefaults to `$` (the whole referenced value).\nMay also be a bare field name (without the leading $) if\nthe referenced value is an object.'
            ),
        ]
        | None
    ) = None
    onSkip: SkipAction | None = None


Expr = Annotated[
    Reference | EscapedLiteral | Value,
    Meta(
        description='An expression that can be either a literal value or a template expression.'
    ),
]


ValueTemplate = Annotated[
    Expr | bool | float | str | List['ValueTemplate'] | Dict[str, 'ValueTemplate'] | None,
    Meta(
        description='A value that can be either a literal JSON value or an expression that references other values using the $from syntax'
    ),
]


class TestCase(Struct, kw_only=True):
    name: Annotated[str, Meta(description='Unique identifier for the test case.')]
    input: Annotated[
        Value, Meta(description='Input data for the workflow in this test case.')
    ]
    description: (
        Annotated[
            str | None,
            Meta(description='Optional description of what this test case verifies.'),
        ]
        | None
    ) = None
    output: (
        Annotated[
            FlowResult | None,
            Meta(description='Expected output from the workflow for this test case.'),
        ]
        | None
    ) = None


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


class OnErrorDefault(Struct, kw_only=True):
    action: Literal['useDefault']
    defaultValue: ValueTemplate | None = None


ErrorAction = OnErrorFail | OnErrorSkip | OnErrorDefault | OnErrorRetry


class TestConfig(Struct, kw_only=True):
    servers: (
        Annotated[
            Dict[str, TestServerConfig],
            Meta(
                description='Test servers to start before running tests.\nKey is the server name, value is the server configuration.'
            ),
        ]
        | None
    ) = None
    config: (
        Annotated[
            Any,
            Meta(
                description='Stepflow configuration specific to tests.\nCan reference server URLs using placeholders like {server_name.url}.'
            ),
        ]
        | None
    ) = None
    cases: (
        Annotated[List[TestCase], Meta(description='Test cases for the workflow.')]
        | None
    ) = None


MethodResponse = Annotated[
    MethodSuccess | MethodError, Meta(description='Response to a method request.')
]


class Step(Struct, kw_only=True):
    id: Annotated[str, Meta(description='Identifier for the step')]
    component: Annotated[
        Component, Meta(description='The component to execute in this step')
    ]
    inputSchema: (
        Annotated[Schema | None, Meta(description='The input schema for this step.')]
        | None
    ) = None
    outputSchema: (
        Annotated[Schema | None, Meta(description='The output schema for this step.')]
        | None
    ) = None
    skipIf: (
        Annotated[
            Expr | None,
            Meta(
                description='If set and the referenced value is truthy, this step will be skipped.'
            ),
        ]
        | None
    ) = None
    onError: ErrorAction | None = None
    input: (
        Annotated[
            ValueTemplate,
            Meta(description='Arguments to pass to the component for this step'),
        ]
        | None
    ) = None


class Flow(Struct, kw_only=True):
    steps: Annotated[List[Step], Meta(description='The steps to execute for the flow.')]
    name: Annotated[str | None, Meta(description='The name of the flow.')] | None = None
    description: (
        Annotated[str | None, Meta(description='The description of the flow.')] | None
    ) = None
    version: (
        Annotated[str | None, Meta(description='The version of the flow.')] | None
    ) = None
    inputSchema: (
        Annotated[Schema | None, Meta(description='The input schema of the flow.')]
        | None
    ) = None
    outputSchema: (
        Annotated[Schema | None, Meta(description='The output schema of the flow.')]
        | None
    ) = None
    output: (
        Annotated[
            ValueTemplate,
            Meta(
                description='The outputs of the flow, mapping output names to their values.'
            ),
        ]
        | None
    ) = None
    test: (
        Annotated[
            TestConfig | None, Meta(description='Test configuration for the flow.')
        ]
        | None
    ) = None
    examples: (
        Annotated[
            List[ExampleInput],
            Meta(
                description='Example inputs for the workflow that can be used for testing and UI dropdowns.'
            ),
        ]
        | None
    ) = None


class EvaluateFlowParams(Struct, kw_only=True):
    flow: Annotated[Flow, Meta(description='The flow to evaluate.')]
    input: Annotated[Value, Meta(description='The input to provide to the flow.')]


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


Message = Annotated[
    MethodRequest | MethodSuccess | MethodError | Notification,
    Meta(
        description='The messages supported by the StepFlow protocol. These correspond to JSON-RPC 2.0 messages.\n\nNote that this defines a superset containing both client-sent and server-sent messages.',
        title='Message',
    ),
]
