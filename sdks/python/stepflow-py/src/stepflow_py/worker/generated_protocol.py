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
    components_infer_schema = 'components/infer_schema'
    blobs_put = 'blobs/put'
    blobs_get = 'blobs/get'
    runs_submit = 'runs/submit'
    runs_get = 'runs/get'


Value = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


class MethodRequest(Struct, kw_only=True):
    id: RequestId
    method: Annotated[Method, Meta(description='The method being called.')]
    jsonrpc: JsonRpc | None = '2.0'
    params: Value | None = None


class Error(Struct, kw_only=True):
    code: Annotated[int, Meta(description='A numeric code indicating the error type.')]
    message: Annotated[
        str, Meta(description='Concise, single-sentence description of the error.')
    ]
    data: Value | None = None


class MethodError(Struct, kw_only=True):
    id: RequestId
    error: Annotated[
        Error, Meta(description='An error that occurred during method execution.')
    ]
    jsonrpc: JsonRpc | None = '2.0'


class MethodSuccess(Struct, kw_only=True):
    id: RequestId
    result: Annotated[
        Value, Meta(description='The result of a successful method execution.')
    ]
    jsonrpc: JsonRpc | None = '2.0'


class Notification(Struct, kw_only=True):
    method: Annotated[Any, Meta(description='The notification method being called.')]
    jsonrpc: (
        Annotated[
            Literal['2.0'], Meta(description='The version of the JSON-RPC protocol.')
        ]
        | None
    ) = '2.0'
    params: (
        Annotated[Any, Meta(description='The parameters for the notification.')] | None
    ) = None


BlobId = Annotated[
    str,
    Meta(
        description='A SHA-256 hash of the blob content, represented as a hexadecimal string.'
    ),
]


class InitializeResult(Struct, kw_only=True):
    server_protocol_version: Annotated[
        int,
        Meta(
            description='Version of the protocol being used by the component server.',
            ge=0,
        ),
    ]


class Initialized(Struct, kw_only=True):
    pass


Component = Annotated[
    str,
    Meta(
        description="Identifies a specific plugin and atomic functionality to execute. Use component name for builtins (e.g., 'eval') or path format for plugins (e.g., '/python/udf').",
        examples=['/builtin/eval', '/mcpfs/list_files', '/python/udf'],
    ),
]


class ComponentExecuteResult(Struct, kw_only=True):
    output: Annotated[Value, Meta(description='The result of the component execution.')]


class ComponentInfoParams(Struct, kw_only=True):
    component: Annotated[
        Component, Meta(description='The component to get information about.')
    ]


class Schema(Struct, kw_only=True):
    pass


class ComponentListParams(Struct, kw_only=True):
    pass


class ComponentInferSchemaParams(Struct, kw_only=True):
    component: Annotated[
        Component, Meta(description='The component to infer the schema for.')
    ]
    input_schema: Annotated[
        Schema,
        Meta(
            description='The schema of the input that will be provided to the component.'
        ),
    ]


class ComponentInferSchemaResult(Struct, kw_only=True):
    output_schema: Schema | None = None


class BlobType(Enum):
    flow = 'flow'
    data = 'data'


class GetBlobResult(Struct, kw_only=True):
    data: Value
    blob_type: BlobType


class PutBlobResult(Struct, kw_only=True):
    blob_id: BlobId


class OverrideType(Enum):
    merge_patch = 'merge_patch'
    json_patch = 'json_patch'


class ResultOrder(Enum):
    by_index = 'by_index'
    by_completion = 'by_completion'


class ItemStatistics(Struct, kw_only=True):
    total: Annotated[int, Meta(description='Total number of items in this run.', ge=0)]
    completed: Annotated[int, Meta(description='Number of completed items.', ge=0)]
    running: Annotated[
        int, Meta(description='Number of currently running items.', ge=0)
    ]
    failed: Annotated[int, Meta(description='Number of failed items.', ge=0)]
    cancelled: Annotated[int, Meta(description='Number of cancelled items.', ge=0)]


class ExecutionStatus(Enum):
    running = 'running'
    completed = 'completed'
    failed = 'failed'
    cancelled = 'cancelled'
    paused = 'paused'


class FlowError(Struct, kw_only=True):
    code: int
    message: str
    data: Value | None = None


class FlowResultSuccess(Struct, kw_only=True, tag_field='outcome', tag='success'):
    outcome: ClassVar[Annotated[Literal['success'], Meta(title='FlowOutcome')]]
    result: Value


class FlowResultFailed(Struct, kw_only=True, tag_field='outcome', tag='failed'):
    outcome: ClassVar[Annotated[Literal['failed'], Meta(title='FlowOutcome')]]
    error: FlowError


Message1 = Annotated[
    MethodRequest | MethodSuccess | MethodError | Notification,
    Meta(
        description='The messages supported by the Stepflow protocol. These correspond to JSON-RPC 2.0 messages.'
    ),
]


Message = Annotated[
    MethodRequest | MethodSuccess | MethodError | Notification,
    Meta(
        description='The messages supported by the Stepflow protocol. These correspond to JSON-RPC 2.0 messages.',
        title='Message',
    ),
]


MethodResponse = Annotated[
    MethodSuccess | MethodError,
    Meta(
        description="Response to a method request. This is an untagged union - success responses have a 'result' field while error responses have an 'error' field."
    ),
]


class ObservabilityContext(Struct, kw_only=True):
    trace_id: (
        Annotated[
            str | None,
            Meta(
                description='OpenTelemetry trace ID (128-bit, hex encoded).\n\nPresent when tracing is enabled, None otherwise.\nUsed to correlate all operations within a single trace.'
            ),
        ]
        | None
    ) = None
    span_id: (
        Annotated[
            str | None,
            Meta(
                description='OpenTelemetry span ID (64-bit, hex encoded).\n\nUsed to establish parent-child span relationships.\nComponent servers should use this as the parent span when creating their spans.'
            ),
        ]
        | None
    ) = None
    run_id: (
        Annotated[
            str | None,
            Meta(
                description='The ID of the workflow run.\n\nPresent for workflow execution requests, None for initialization/discovery.\nUsed for filtering logs and associating operations with specific workflow runs.'
            ),
        ]
        | None
    ) = None
    flow_id: BlobId | None = None
    step_id: (
        Annotated[
            str | None,
            Meta(
                description='The ID of the step being executed.\n\nPresent for step-level execution, None for workflow-level operations.\nUsed for filtering logs and associating operations with specific workflow steps.'
            ),
        ]
        | None
    ) = None


class InitializeParams(Struct, kw_only=True):
    runtime_protocol_version: Annotated[
        int,
        Meta(
            description='Maximum version of the protocol being used by the Stepflow runtime.',
            ge=0,
        ),
    ]
    observability: ObservabilityContext | None = None


class ComponentExecuteParams(Struct, kw_only=True):
    component: Annotated[Component, Meta(description='The component to execute.')]
    input: Annotated[Value, Meta(description='The input to the component.')]
    attempt: Annotated[
        int,
        Meta(
            description='The attempt number for this execution (1-based, for retry logic).',
            ge=0,
        ),
    ]
    observability: Annotated[
        ObservabilityContext,
        Meta(description='Observability context for tracing and logging.'),
    ]


class ComponentInfo(Struct, kw_only=True):
    component: Annotated[Component, Meta(description='The component ID.')]
    description: (
        Annotated[
            str | None, Meta(description='Optional description of the component.')
        ]
        | None
    ) = None
    input_schema: Schema | None = None
    output_schema: Schema | None = None


class ComponentInfoResult(Struct, kw_only=True):
    info: Annotated[ComponentInfo, Meta(description='Information about the component.')]


class ListComponentsResult(Struct, kw_only=True):
    components: Annotated[
        List[ComponentInfo], Meta(description='A list of all available components.')
    ]


class GetBlobParams(Struct, kw_only=True):
    blob_id: Annotated[BlobId, Meta(description='The ID of the blob to retrieve.')]
    observability: ObservabilityContext | None = None


class PutBlobParams(Struct, kw_only=True):
    data: Value
    blob_type: BlobType
    observability: ObservabilityContext | None = None


class StepOverride(Struct, kw_only=True):
    value: Annotated[
        Any,
        Meta(
            description='The override value to apply, interpreted based on the override type.'
        ),
    ]
    field_type: (
        Annotated[
            OverrideType,
            Meta(
                description='The type of override to apply. Defaults to "merge_patch" if not specified.'
            ),
        ]
        | None
    ) = field(name='$type', default=None)


class GetRunProtocolParams(Struct, kw_only=True):
    runId: Annotated[str, Meta(description='The run ID to query.')]
    wait: (
        Annotated[
            bool, Meta(description='If true, wait for run completion before returning.')
        ]
        | None
    ) = None
    includeResults: (
        Annotated[
            bool, Meta(description='If true, include item results in the response.')
        ]
        | None
    ) = None
    resultOrder: (
        Annotated[
            ResultOrder, Meta(description='Order of results (byIndex or byCompletion).')
        ]
        | None
    ) = None
    observability: ObservabilityContext | None = None


FlowResult = Annotated[
    FlowResultSuccess | FlowResultFailed,
    Meta(description='The results of a step execution.', title='FlowResult'),
]


class WorkflowOverrides(Struct, kw_only=True):
    steps: Annotated[
        Dict[str, StepOverride],
        Meta(description='Map of step ID to override specification'),
    ]


class SubmitRunProtocolParams(Struct, kw_only=True):
    flowId: Annotated[
        BlobId, Meta(description='The ID of the flow to execute (blob ID).')
    ]
    inputs: Annotated[
        List[Value], Meta(description='Input values for each item in the run.')
    ]
    wait: (
        Annotated[
            bool, Meta(description='If true, wait for completion before returning.')
        ]
        | None
    ) = None
    maxConcurrency: (
        Annotated[
            int | None,
            Meta(description='Maximum number of concurrent executions.', ge=0),
        ]
        | None
    ) = None
    overrides: WorkflowOverrides | None = None
    observability: ObservabilityContext | None = None


class ItemResult(Struct, kw_only=True):
    itemIndex: Annotated[
        int, Meta(description='Index of this item in the input array (0-based).', ge=0)
    ]
    status: Annotated[
        ExecutionStatus, Meta(description='Execution status of this item.')
    ]
    result: FlowResult | None = None
    completedAt: (
        Annotated[
            str | None, Meta(description='When this item completed (if completed).')
        ]
        | None
    ) = None


class RunStatusProtocol(Struct, kw_only=True):
    runId: str
    flowId: BlobId
    status: str
    items: ItemStatistics
    createdAt: str
    flowName: str | None = None
    completedAt: str | None = None
    results: List[ItemResult] | None = None
