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

from enum import StrEnum
from typing import Annotated, Any, Literal, TypeAlias

from msgspec import UNSET, Meta, Struct, UnsetType, field

Component: TypeAlias = Annotated[
    str,
    Meta(
        description="Identifies a specific plugin and atomic functionality to execute. Use component name for builtins (e.g., 'eval') or path format for plugins (e.g., '/python/udf').",
        examples=['/builtin/eval', '/mcpfs/list_files', '/python/udf'],
    ),
]


Value: TypeAlias = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


BlobId: TypeAlias = Annotated[
    str,
    Meta(
        description='A SHA-256 hash of the blob content, represented as a hexadecimal string.'
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
    output_schema: (
        Annotated[
            Schema | None,
            Meta(
                description='The inferred output schema, or None if the component cannot determine it.'
            ),
        ]
        | UnsetType
    ) = UNSET


class OverrideType(StrEnum):
    merge_patch = 'merge_patch'
    json_patch = 'json_patch'


class ResultOrder(StrEnum):
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


class ExecutionStatus(StrEnum):
    running = 'running'
    completed = 'completed'
    failed = 'failed'
    cancelled = 'cancelled'
    paused = 'paused'
    recoveryFailed = 'recoveryFailed'


class FlowResultSuccess(Struct, kw_only=True, tag_field='outcome', tag='success'):
    result: Value


class FlowError(Struct, kw_only=True):
    code: int
    message: str
    data: Value | None | UnsetType = UNSET


class RuntimeCapabilities(Struct, kw_only=True):
    blobApiUrl: (
        Annotated[
            str | None,
            Meta(
                description='Base URL for the Blob HTTP API.\n\nWhen provided, component servers should use direct HTTP requests\n(`GET {blob_api_url}/{blob_id}`, `POST {blob_api_url}`) for blob operations\ninstead of SSE bidirectional protocol.'
            ),
        ]
        | UnsetType
    ) = UNSET
    blobThreshold: (
        Annotated[
            int | None,
            Meta(
                description='Byte size threshold for automatic blobification.\n\nWhen set to a non-zero value, the orchestrator may replace large input fields\nwith `$blob` references. Component servers that report `supports_blob_refs`\nshould resolve these references before processing. Component servers should\nalso blobify output fields exceeding this threshold.\n\nA value of 0 or `None` means automatic blobification is disabled.'
            ),
        ]
        | UnsetType
    ) = UNSET


class InitializeResult(Struct, kw_only=True):
    serverProtocolVersion: Annotated[
        int,
        Meta(
            description='Version of the protocol being used by the component server.',
            ge=0,
        ),
    ]
    supportsBlobRefs: (
        Annotated[
            bool,
            Meta(
                description='Whether this component server supports `$blob` references in inputs/outputs.\n\nWhen `true`, the orchestrator may send `$blob` references in component inputs\nand expects the server to resolve them. The server may also return `$blob`\nreferences in outputs for the orchestrator to resolve.\n\nWhen `false` (default), the orchestrator will not send blob refs and will\nresolve any refs before delivering input to this server.'
            ),
        ]
        | UnsetType
    ) = UNSET


class Initialized(Struct, kw_only=True):
    pass


JsonRpc: TypeAlias = Annotated[
    Literal['2.0'], Meta(description='The version of the JSON-RPC protocol.')
]


RequestId: TypeAlias = Annotated[
    str | int,
    Meta(
        description='The identifier for a JSON-RPC request. Can be either a string or an integer.\nThe RequestId is used to match method responses to corresponding requests.\nIt should not be set on notifications.'
    ),
]


class Method(StrEnum):
    initialize = 'initialize'
    initialized = 'initialized'
    components_list = 'components/list'
    components_info = 'components/info'
    components_execute = 'components/execute'
    components_infer_schema = 'components/infer_schema'
    runs_submit = 'runs/submit'
    runs_get = 'runs/get'


class MethodSuccess(Struct, kw_only=True):
    id: RequestId
    result: Annotated[
        Any, Meta(description='The result of a successful method execution.')
    ]
    jsonrpc: JsonRpc | UnsetType = '2.0'


class Error(Struct, kw_only=True):
    code: Annotated[int, Meta(description='A numeric code indicating the error type.')]
    message: Annotated[
        str, Meta(description='Concise, single-sentence description of the error.')
    ]
    data: (
        Annotated[
            Any,
            Meta(
                description='Primitive or structured value that contains additional information about the error.'
            ),
        ]
        | UnsetType
    ) = UNSET


class Notification(Struct, kw_only=True):
    method: Annotated[str, Meta(description='The notification method being called.')]
    jsonrpc: JsonRpc | UnsetType = '2.0'
    params: (
        Annotated[Any, Meta(description='The parameters for the notification.')]
        | UnsetType
    ) = UNSET


class ObservabilityContext(Struct, kw_only=True):
    trace_id: (
        Annotated[
            str | None,
            Meta(
                description='OpenTelemetry trace ID (128-bit, hex encoded).\n\nPresent when tracing is enabled, None otherwise.\nUsed to correlate all operations within a single trace.'
            ),
        ]
        | UnsetType
    ) = UNSET
    span_id: (
        Annotated[
            str | None,
            Meta(
                description='OpenTelemetry span ID (64-bit, hex encoded).\n\nUsed to establish parent-child span relationships.\nComponent servers should use this as the parent span when creating their spans.'
            ),
        ]
        | UnsetType
    ) = UNSET
    run_id: (
        Annotated[
            str | None,
            Meta(
                description='The ID of the workflow run.\n\nPresent for workflow execution requests, None for initialization/discovery.\nUsed for filtering logs and associating operations with specific workflow runs.'
            ),
        ]
        | UnsetType
    ) = UNSET
    flow_id: (
        Annotated[
            BlobId | None,
            Meta(
                description='The ID of the flow being executed.\n\nPresent for workflow execution requests, None for initialization/discovery.\nUsed for filtering logs and understanding which workflow is being executed.'
            ),
        ]
        | UnsetType
    ) = UNSET
    step_id: (
        Annotated[
            str | None,
            Meta(
                description='The ID of the step being executed.\n\nPresent for step-level execution, None for workflow-level operations.\nUsed for filtering logs and associating operations with specific workflow steps.'
            ),
        ]
        | UnsetType
    ) = UNSET


class ComponentInfo(Struct, kw_only=True):
    component: Annotated[Component, Meta(description='The component ID.')]
    description: (
        Annotated[
            str | None, Meta(description='Optional description of the component.')
        ]
        | UnsetType
    ) = UNSET
    input_schema: (
        Annotated[
            Schema | None,
            Meta(
                description='The input schema for the component.\n\nCan be any valid JSON schema (object, primitive, array, etc.).'
            ),
        ]
        | UnsetType
    ) = UNSET
    output_schema: (
        Annotated[
            Schema | None,
            Meta(
                description='The output schema for the component.\n\nCan be any valid JSON schema (object, primitive, array, etc.).'
            ),
        ]
        | UnsetType
    ) = UNSET


class ListComponentsResult(Struct, kw_only=True):
    components: Annotated[
        list[ComponentInfo], Meta(description='A list of all available components.')
    ]


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
        | UnsetType
    ) = field(name='$type', default='merge_patch')


class GetRunProtocolParams(Struct, kw_only=True):
    runId: Annotated[str, Meta(description='The run ID to query.')]
    wait: (
        Annotated[
            bool, Meta(description='If true, wait for run completion before returning.')
        ]
        | UnsetType
    ) = False
    includeResults: (
        Annotated[
            bool, Meta(description='If true, include item results in the response.')
        ]
        | UnsetType
    ) = False
    resultOrder: (
        Annotated[
            ResultOrder, Meta(description='Order of results (byIndex or byCompletion).')
        ]
        | UnsetType
    ) = 'by_index'
    timeoutSecs: (
        Annotated[
            int | None,
            Meta(
                description='Maximum seconds to wait when wait=true (default 300). If the timeout elapses,\nreturns the current run status rather than an error.',
                ge=0,
            ),
        ]
        | UnsetType
    ) = UNSET
    observability: (
        Annotated[
            ObservabilityContext | None,
            Meta(description='Observability context for tracing.'),
        ]
        | UnsetType
    ) = UNSET


class FlowResultFailed(Struct, kw_only=True, tag_field='outcome', tag='failed'):
    error: FlowError


class InitializeParams(Struct, kw_only=True):
    runtimeProtocolVersion: Annotated[
        int,
        Meta(
            description='Maximum version of the protocol being used by the Stepflow runtime.',
            ge=0,
        ),
    ]
    observability: (
        Annotated[
            ObservabilityContext | None,
            Meta(
                description='Observability context for tracing initialization (trace context only, no flow/run).'
            ),
        ]
        | UnsetType
    ) = UNSET
    capabilities: (
        Annotated[
            RuntimeCapabilities | None,
            Meta(description='Runtime capabilities provided to the component server.'),
        ]
        | UnsetType
    ) = UNSET


class MethodRequest(Struct, kw_only=True):
    id: RequestId
    method: Annotated[Method, Meta(description='The method being called.')]
    jsonrpc: JsonRpc | UnsetType = '2.0'
    params: (
        Annotated[
            Any,
            Meta(
                description='The parameters for the method call. Set on method requests.'
            ),
        ]
        | UnsetType
    ) = UNSET


class MethodError(Struct, kw_only=True):
    id: RequestId
    error: Annotated[
        Error, Meta(description='An error that occurred during method execution.')
    ]
    jsonrpc: JsonRpc | UnsetType = '2.0'


class ComponentExecuteParams(Struct, kw_only=True):
    component: Annotated[Component, Meta(description='The component to execute.')]
    input: Annotated[Value, Meta(description='The input to the component.')]
    attempt: Annotated[
        int,
        Meta(
            description='The execution attempt number (1-based).\n\nA monotonically increasing counter that increments on every re-execution\nof this step, regardless of the reason:\n- **Transport error**: The subprocess crashed or a network failure occurred.\n- **Component error**: The component returned an error and the step has\n  `onError: { action: retry }`.\n- **Orchestrator recovery**: The orchestrator crashed and is re-executing\n  tasks that were in-flight.\n\nComponents can use this to implement idempotency guards or progressive\nfallback strategies.',
            ge=0,
        ),
    ]
    observability: Annotated[
        ObservabilityContext,
        Meta(description='Observability context for tracing and logging.'),
    ]


class ComponentInfoResult(Struct, kw_only=True):
    info: Annotated[ComponentInfo, Meta(description='Information about the component.')]


class WorkflowOverrides(Struct, kw_only=True):
    steps: Annotated[
        dict[str, StepOverride],
        Meta(description='Map of step ID to override specification'),
    ]


FlowResult: TypeAlias = FlowResultSuccess | FlowResultFailed


MethodResponse: TypeAlias = Annotated[
    MethodSuccess | MethodError,
    Meta(
        description="Response to a method request. Success responses have a 'result' field while error responses have an 'error' field."
    ),
]


Message: TypeAlias = Annotated[
    MethodRequest | MethodResponse | Notification,
    Meta(
        description='The messages supported by the Stepflow protocol (JSON-RPC 2.0).',
        title='Message',
    ),
]


class SubmitRunProtocolParams(Struct, kw_only=True):
    flowId: Annotated[
        BlobId, Meta(description='The ID of the flow to execute (blob ID).')
    ]
    inputs: Annotated[
        list[Value], Meta(description='Input values for each item in the run.')
    ]
    subflowKey: Annotated[
        str,
        Meta(
            description="Client-provided key for subflow deduplication during recovery.\n\nThis key uniquely identifies a subflow submission within the scope\nof the executing step. The orchestrator records this key in the\njournal. If the parent step re-executes after a crash, the\norchestrator matches new submissions by key and returns the existing\nsubflow's run ID instead of creating a duplicate.\n\nThe client must generate the same key on re-execution for recovery\nto work. A common approach is to derive a deterministic UUID from a\nmonotonic counter scoped to the step execution."
        ),
    ]
    wait: (
        Annotated[
            bool, Meta(description='If true, wait for completion before returning.')
        ]
        | UnsetType
    ) = False
    maxConcurrency: (
        Annotated[
            int | None,
            Meta(description='Maximum number of concurrent executions.', ge=0),
        ]
        | UnsetType
    ) = UNSET
    overrides: (
        Annotated[
            WorkflowOverrides | None,
            Meta(description='Optional workflow overrides to apply.'),
        ]
        | UnsetType
    ) = UNSET
    timeoutSecs: (
        Annotated[
            int | None,
            Meta(
                description='Maximum seconds to wait when wait=true (default 300). If the timeout elapses,\nreturns the current run status rather than an error.',
                ge=0,
            ),
        ]
        | UnsetType
    ) = UNSET
    observability: (
        Annotated[
            ObservabilityContext | None,
            Meta(description='Observability context for tracing.'),
        ]
        | UnsetType
    ) = UNSET


class ItemResult(Struct, kw_only=True):
    itemIndex: Annotated[
        int, Meta(description='Index of this item in the input array (0-based).', ge=0)
    ]
    status: Annotated[
        ExecutionStatus, Meta(description='Execution status of this item.')
    ]
    result: (
        Annotated[
            FlowResult | None, Meta(description='Result of this item, if completed.')
        ]
        | UnsetType
    ) = UNSET
    completedAt: (
        Annotated[
            str | None, Meta(description='When this item completed (if completed).')
        ]
        | UnsetType
    ) = UNSET


class RunStatusProtocol(Struct, kw_only=True):
    runId: str
    flowId: BlobId
    status: str
    items: ItemStatistics
    createdAt: str
    flowName: str | None | UnsetType = UNSET
    completedAt: str | None | UnsetType = UNSET
    results: list[ItemResult] | UnsetType = UNSET
