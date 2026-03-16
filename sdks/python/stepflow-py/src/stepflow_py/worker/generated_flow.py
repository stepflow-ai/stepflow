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

import sys

if sys.version_info >= (3, 11):
    from enum import StrEnum
else:
    from enum import Enum

    class StrEnum(str, Enum):
        def __str__(self) -> str:
            return self.value

from typing import Annotated, Any, TypeAlias

from msgspec import UNSET, Meta, Struct, UnsetType


class FlowSchema(Struct, kw_only=True):
    pass


Component: TypeAlias = Annotated[
    str,
    Meta(
        description="Identifies a specific plugin and atomic functionality to execute. Use component name for builtins (e.g., 'eval') or path format for plugins (e.g., '/python/udf').",
        examples=['/builtin/eval', '/mcpfs/list_files', '/python/udf'],
    ),
]


ValueExpr: TypeAlias = Annotated[
    Any,
    Meta(
        description='A value expression: any JSON value (null, boolean, number, string, array, or object). Objects with reserved $-prefixed keys are interpreted as expression references: {"$step": "id", "path"?: "..."}, {"$input": "path"}, {"$variable": "path", "default"?: ValueExpr}, {"$literal": value}, {"$if": cond, "then": expr, "else"?: expr}, {"$coalesce": [expr, ...]}. See https://stepflow.org/docs/flows/expressions for details.'
    ),
]


Value: TypeAlias = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


class FlowResultSuccess(Struct, kw_only=True, tag_field='outcome', tag='success'):
    result: Value


class TaskErrorCode(StrEnum):
    UNSPECIFIED = 'UNSPECIFIED'
    TIMEOUT = 'TIMEOUT'
    INVALID_INPUT = 'INVALID_INPUT'
    COMPONENT_FAILED = 'COMPONENT_FAILED'
    CANCELLED = 'CANCELLED'
    UNREACHABLE = 'UNREACHABLE'
    COMPONENT_NOT_FOUND = 'COMPONENT_NOT_FOUND'
    RESOURCE_UNAVAILABLE = 'RESOURCE_UNAVAILABLE'
    EXPRESSION_FAILURE = 'EXPRESSION_FAILURE'
    ORCHESTRATOR_ERROR = 'ORCHESTRATOR_ERROR'
    WORKER_ERROR = 'WORKER_ERROR'


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
        | UnsetType
    ) = UNSET


class OnErrorFail(Struct, kw_only=True, tag_field='action', tag='fail'):
    pass


class OnErrorDefault(Struct, kw_only=True, tag_field='action', tag='useDefault'):
    defaultValue: Any | UnsetType = UNSET


class OnErrorRetry(Struct, kw_only=True, tag_field='action', tag='retry'):
    maxRetries: (
        Annotated[
            int | None,
            Meta(
                description='Maximum number of retries due to component errors (default: 3).\n\nTotal attempts for component errors = max_retries + 1 (initial).',
            ),
        ]
        | UnsetType
    ) = UNSET


ErrorAction: TypeAlias = Annotated[
    OnErrorFail | OnErrorDefault | OnErrorRetry,
    Meta(description='Error action determines what happens when a step fails.'),
]


class FlowError(Struct, kw_only=True):
    code: TaskErrorCode
    message: str
    data: Value | None | UnsetType = UNSET


class Step(Struct, kw_only=True):
    id: Annotated[str, Meta(description='Identifier for the step')]
    component: Annotated[
        Component, Meta(description='The component to execute in this step')
    ]
    onError: ErrorAction | None | UnsetType = UNSET
    input: (
        Annotated[
            ValueExpr,
            Meta(description='Arguments to pass to the component for this step'),
        ]
        | UnsetType
    ) = UNSET
    mustExecute: (
        Annotated[
            bool | None,
            Meta(
                description='If true, this step must execute even if its output is not used by the workflow output.\nUseful for steps with side effects (e.g., writing to databases, sending notifications).'
            ),
        ]
        | UnsetType
    ) = UNSET
    metadata: (
        Annotated[
            dict[str, Any] | None,
            Meta(
                description='Extensible metadata for the step that can be used by tools and frameworks.'
            ),
        ]
        | UnsetType
    ) = UNSET


class FlowResultFailed(Struct, kw_only=True, tag_field='outcome', tag='failed'):
    error: FlowError


FlowResult: TypeAlias = FlowResultSuccess | FlowResultFailed


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
        | UnsetType
    ) = UNSET
    output: (
        Annotated[
            FlowResult | None,
            Meta(description='Expected output from the workflow for this test case.'),
        ]
        | UnsetType
    ) = UNSET


class TestConfig(Struct, kw_only=True):
    configFile: (
        Annotated[
            str | None,
            Meta(
                description="Path to an external stepflow config file for tests.\nRelative paths are resolved from the workflow file's directory.\nMutually exclusive with `config` - validated at runtime."
            ),
        ]
        | UnsetType
    ) = UNSET
    config: (
        Annotated[
            Any,
            Meta(
                description='Inline stepflow configuration for tests.\nMutually exclusive with `config_file` - validated at runtime.'
            ),
        ]
        | UnsetType
    ) = UNSET
    cases: (
        Annotated[list[TestCase], Meta(description='Test cases for the workflow.')]
        | UnsetType
    ) = UNSET


class Flow(Struct, kw_only=True):
    name: (
        Annotated[str | None, Meta(description='The name of the flow.')] | UnsetType
    ) = UNSET
    description: (
        Annotated[str | None, Meta(description='The description of the flow.')]
        | UnsetType
    ) = UNSET
    version: (
        Annotated[str | None, Meta(description='The version of the flow.')] | UnsetType
    ) = UNSET
    schemas: (
        Annotated[
            FlowSchema,
            Meta(
                description='Consolidated schema information for the flow.\nContains input/output schemas, step output schemas, and shared `$defs`.'
            ),
        ]
        | UnsetType
    ) = UNSET
    steps: (
        Annotated[list[Step], Meta(description='The steps to execute for the flow.')]
        | UnsetType
    ) = UNSET
    output: (
        Annotated[
            ValueExpr,
            Meta(
                description='The outputs of the flow, mapping output names to their values.'
            ),
        ]
        | UnsetType
    ) = UNSET
    test: (
        Annotated[
            TestConfig | None, Meta(description='Test configuration for the flow.')
        ]
        | UnsetType
    ) = UNSET
    examples: (
        Annotated[
            list[ExampleInput],
            Meta(
                description='Example inputs for the workflow that can be used for testing and UI dropdowns.'
            ),
        ]
        | UnsetType
    ) = UNSET
    metadata: (
        Annotated[
            dict[str, Any] | None,
            Meta(
                description='Extensible metadata for the flow that can be used by tools and frameworks.'
            ),
        ]
        | UnsetType
    ) = UNSET
