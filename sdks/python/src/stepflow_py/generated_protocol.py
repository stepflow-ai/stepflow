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
    flows_evaluate = 'flows/evaluate'
    flows_get_metadata = 'flows/get_metadata'
    flows_submit_batch = 'flows/submit_batch'
    flows_get_batch = 'flows/get_batch'


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


class GetFlowMetadataResult(Struct, kw_only=True):
    flow_metadata: Annotated[
        Dict[str, Any],
        Meta(
            description='Metadata for the current flow.\n\nThis always contains the flow-level metadata defined in the workflow file.\nCommon fields include name, description, version, but can contain any\narbitrary JSON structure defined by the workflow author.'
        ),
    ]
    step_metadata: (
        Annotated[
            Dict[str, Any] | None,
            Meta(
                description='Metadata for the specified step (only present if step_id was provided and found).\n\nThis contains step-specific metadata defined in the workflow file.\nWill be None if no step_id was provided in the request'
            ),
        ]
        | None
    ) = None


class SubmitBatchResult(Struct, kw_only=True):
    batch_id: Annotated[str, Meta(description='The batch ID (UUID).')]
    total_runs: Annotated[
        int, Meta(description='Total number of runs in the batch.', ge=0)
    ]


class BatchDetails(Struct, kw_only=True):
    batch_id: Annotated[str, Meta(description='The batch ID.')]
    flow_id: Annotated[BlobId, Meta(description='The flow ID.')]
    total_runs: Annotated[
        int, Meta(description='Total number of runs in the batch.', ge=0)
    ]
    status: Annotated[str, Meta(description='Batch status (running | cancelled).')]
    created_at: Annotated[
        str, Meta(description='Timestamp when the batch was created.')
    ]
    completed_runs: Annotated[int, Meta(description='Statistics for the batch.', ge=0)]
    running_runs: Annotated[int, Meta(ge=0)]
    failed_runs: Annotated[int, Meta(ge=0)]
    cancelled_runs: Annotated[int, Meta(ge=0)]
    paused_runs: Annotated[int, Meta(ge=0)]
    flow_name: (
        Annotated[str | None, Meta(description='The flow name (optional).')] | None
    ) = None
    completed_at: (
        Annotated[
            str | None, Meta(description='Completion timestamp (if all runs complete).')
        ]
        | None
    ) = None


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


FlowResult = Annotated[
    FlowResultSuccess | FlowResultFailed,
    Meta(description='The results of a step execution.', title='FlowResult'),
]


class EvaluateFlowResult(Struct, kw_only=True):
    result: Annotated[
        FlowResult, Meta(description='The result of the flow evaluation.')
    ]


class GetFlowMetadataParams(Struct, kw_only=True):
    flow_id: Annotated[BlobId, Meta(description='The flow to retrieve metadata for.')]
    step_id: (
        Annotated[
            str | None,
            Meta(
                description="The ID of the step to get metadata for (optional).\n\nIf not provided, only flow-level metadata is returned.\nIf provided, both flow metadata and the specified step's metadata are returned.\nIf the step_id doesn't exist, step_metadata will be None in the response."
            ),
        ]
        | None
    ) = None
    observability: ObservabilityContext | None = None


class GetBatchParams(Struct, kw_only=True):
    batch_id: Annotated[str, Meta(description='The batch ID to query.')]
    wait: (
        Annotated[
            bool,
            Meta(description='If true, wait for batch completion before returning.'),
        ]
        | None
    ) = None
    include_results: (
        Annotated[bool, Meta(description='If true, include full outputs in response.')]
        | None
    ) = None
    observability: ObservabilityContext | None = None


class BatchOutputInfo(Struct, kw_only=True):
    batch_input_index: Annotated[
        int, Meta(description='Position in the batch input array.', ge=0)
    ]
    status: Annotated[str, Meta(description='The execution status.')]
    result: FlowResult | None = None


class GetBatchResult(Struct, kw_only=True):
    details: Annotated[
        BatchDetails,
        Meta(
            description='Always included: batch details with metadata and statistics.'
        ),
    ]
    outputs: (
        Annotated[
            List[BatchOutputInfo],
            Meta(description='Only included if include_results=true.'),
        ]
        | None
    ) = None


class WorkflowOverrides(Struct, kw_only=True):
    steps: Annotated[
        Dict[str, StepOverride],
        Meta(description='Map of step ID to override specification'),
    ]


class EvaluateFlowParams(Struct, kw_only=True):
    flow_id: Annotated[
        BlobId,
        Meta(description='The ID of the flow to evaluate (blob ID of the flow).'),
    ]
    input: Annotated[Value, Meta(description='The input to provide to the flow.')]
    overrides: WorkflowOverrides | None = None
    observability: ObservabilityContext | None = None


class SubmitBatchParams(Struct, kw_only=True):
    flow_id: Annotated[
        BlobId,
        Meta(description='The ID of the flow to evaluate (blob ID of the flow).'),
    ]
    inputs: Annotated[
        List[Value], Meta(description='The inputs to provide to the flow for each run.')
    ]
    overrides: WorkflowOverrides | None = None
    max_concurrency: (
        Annotated[
            int | None,
            Meta(
                description='Maximum number of concurrent executions (defaults to number of inputs if not specified).',
                ge=0,
            ),
        ]
        | None
    ) = None
    observability: ObservabilityContext | None = None
