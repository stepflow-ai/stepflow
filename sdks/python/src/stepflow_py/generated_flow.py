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

# Auto-generated flow types from schemas/flow.json
# To regenerate this file, run:
#   uv run python generate.py

from __future__ import annotations

from typing import Annotated, Any, ClassVar, Dict, List, Literal

from msgspec import Meta, Struct, field


class Schema(Struct, kw_only=True):
    pass


class StepRef(Struct, kw_only=True):
    field_step: str = field(name='$step')
    path: Annotated[str, Meta(description='JSONPath expression')] | None = None


class InputRef(Struct, kw_only=True):
    field_input: Annotated[str, Meta(description='JSONPath expression')] = field(
        name='$input'
    )


class LiteralExpr(Struct, kw_only=True):
    field_literal: Any = field(name='$literal')


Component = Annotated[
    str,
    Meta(
        description="Identifies a specific plugin and atomic functionality to execute. Use component name for builtins (e.g., 'eval') or path format for plugins (e.g., '/python/udf').",
        examples=['/builtin/eval', '/mcpfs/list_files', '/python/udf'],
    ),
]


class OnErrorFail(Struct, kw_only=True):
    action: Literal['fail']


class OnErrorDefault(Struct, kw_only=True):
    action: Literal['useDefault']
    defaultValue: Any | None = None


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


Value = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


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


ErrorAction = Annotated[
    OnErrorFail | OnErrorDefault | OnErrorRetry,
    Meta(description='Error action determines what happens when a step fails.'),
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
    healthCheck: TestServerHealthCheck | None = None
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
    FlowResultSuccess | FlowResultFailed,
    Meta(description='The results of a step execution.', title='FlowResult'),
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
    output: FlowResult | None = None


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


class FlowV1(Struct, kw_only=True):
    schema_: Literal['https://stepflow.org/schemas/v1/flow.json'] = field(name='schema')
    name: Annotated[str | None, Meta(description='The name of the flow.')] | None = None
    description: (
        Annotated[str | None, Meta(description='The description of the flow.')] | None
    ) = None
    version: (
        Annotated[str | None, Meta(description='The version of the flow.')] | None
    ) = None
    inputSchema: Schema | None = None
    outputSchema: Schema | None = None
    variables: Schema | None = None
    steps: (
        Annotated[List[Step], Meta(description='The steps to execute for the flow.')]
        | None
    ) = None
    output: (
        Annotated[
            ValueExpr,
            Meta(
                description='The outputs of the flow, mapping output names to their values.'
            ),
        ]
        | None
    ) = None
    test: TestConfig | None = None
    examples: (
        Annotated[
            List[ExampleInput],
            Meta(
                description='Example inputs for the workflow that can be used for testing and UI dropdowns.'
            ),
        ]
        | None
    ) = None
    metadata: (
        Annotated[
            Dict[str, Any],
            Meta(
                description='Extensible metadata for the flow that can be used by tools and frameworks.'
            ),
        ]
        | None
    ) = None


Flow = Annotated[
    FlowV1,
    Meta(
        description='A workflow consisting of a sequence of steps and their outputs.\n\nA flow represents a complete workflow that can be executed. It contains:\n- A sequence of steps to execute\n- Named outputs that can reference step outputs\n\nFlows should not be cloned. They should generally be stored and passed as a\nreference or inside an `Arc`.',
        title='Flow',
    ),
]


class FlowV11(Struct, kw_only=True):
    name: Annotated[str | None, Meta(description='The name of the flow.')] | None = None
    description: (
        Annotated[str | None, Meta(description='The description of the flow.')] | None
    ) = None
    version: (
        Annotated[str | None, Meta(description='The version of the flow.')] | None
    ) = None
    inputSchema: Schema | None = None
    outputSchema: Schema | None = None
    variables: Schema | None = None
    steps: (
        Annotated[List[Step], Meta(description='The steps to execute for the flow.')]
        | None
    ) = None
    output: (
        Annotated[
            ValueExpr,
            Meta(
                description='The outputs of the flow, mapping output names to their values.'
            ),
        ]
        | None
    ) = None
    test: TestConfig | None = None
    examples: (
        Annotated[
            List[ExampleInput],
            Meta(
                description='Example inputs for the workflow that can be used for testing and UI dropdowns.'
            ),
        ]
        | None
    ) = None
    metadata: (
        Annotated[
            Dict[str, Any],
            Meta(
                description='Extensible metadata for the flow that can be used by tools and frameworks.'
            ),
        ]
        | None
    ) = None


class Step(Struct, kw_only=True):
    id: Annotated[str, Meta(description='Identifier for the step')]
    component: Annotated[
        Component, Meta(description='The component to execute in this step')
    ]
    inputSchema: Schema | None = None
    outputSchema: Schema | None = None
    onError: ErrorAction | None = None
    input: (
        Annotated[
            ValueExpr,
            Meta(description='Arguments to pass to the component for this step'),
        ]
        | None
    ) = None
    mustExecute: (
        Annotated[
            bool | None,
            Meta(
                description='If true, this step must execute even if its output is not used by the workflow output.\nUseful for steps with side effects (e.g., writing to databases, sending notifications).'
            ),
        ]
        | None
    ) = None
    metadata: (
        Annotated[
            Dict[str, Any],
            Meta(
                description='Extensible metadata for the step that can be used by tools and frameworks.'
            ),
        ]
        | None
    ) = None


class VariableRef(Struct, kw_only=True):
    field_variable: Annotated[
        str, Meta(description='JSONPath expression including variable name')
    ] = field(name='$variable')
    default: ValueExpr | None = None


class If(Struct, kw_only=True):
    field_if: ValueExpr = field(name='$if')
    then: ValueExpr
    else_: ValueExpr | None = field(name='else', default=None)


class Coalesce(Struct, kw_only=True):
    field_coalesce: List['ValueExpr'] = field(name='$coalesce')


ValueExpr = Annotated[
    StepRef
    | InputRef
    | VariableRef
    | LiteralExpr
    | If
    | Coalesce
    | List['ValueExpr']
    | Dict[str, 'ValueExpr']
    | bool
    | float
    | str
    | None,
    Meta(
        description='A value expression that can contain literal data or references to other values'
    ),
]
