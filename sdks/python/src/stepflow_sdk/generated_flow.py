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

# Auto-generated flow types from schemas/flow.json
# To regenerate this file, run:
#   uv run python generate.py

from __future__ import annotations

from enum import Enum
from typing import Annotated, Any, Dict, List, Literal

from msgspec import Meta, Struct, field


class Schema(Struct, kw_only=True):
    pass


Component = Annotated[
    str,
    Meta(
        description='Identifies a specific plugin and atomic functionality to execute.'
    ),
]


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


Value = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


class OnErrorFail(Struct, kw_only=True):
    action: Literal['fail']


class OnErrorSkip(Struct, kw_only=True):
    action: Literal['skip']


class OnErrorRetry(Struct, kw_only=True):
    action: Literal['retry']


class Success(Struct, kw_only=True):
    result: Value
    outcome: Literal['success']


class Skipped(Struct, kw_only=True):
    outcome: Literal['skipped']


class FlowError(Struct, kw_only=True):
    code: int
    message: str
    data: Value | None = None


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


class EscapedLiteral(Struct, kw_only=True):
    field_literal: Annotated[
        Value,
        Meta(
            description='A literal value that should not be expanded for expressions.\nThis allows creating JSON values that contain `$from` without expansion.'
        ),
    ] = field(name='$literal')


class WorkflowReference(Struct, kw_only=True):
    workflow: WorkflowRef


BaseRef = Annotated[
    WorkflowReference | StepReference,
    Meta(
        description='An expression that can be either a literal value or a template expression.'
    ),
]


class OnSkipDefault(Struct, kw_only=True):
    action: Literal['useDefault']
    defaultValue: Value | None = None


SkipAction = OnSkipSkip | OnSkipDefault


class Failed(Struct, kw_only=True):
    error: FlowError
    outcome: Literal['failed']


FlowResult = Annotated[
    Success | Skipped | Failed, Meta(description='The results of a step execution.')
]


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


class OnErrorDefault(Struct, kw_only=True):
    action: Literal['useDefault']
    defaultValue: ValueTemplate | None = None


ErrorAction = OnErrorFail | OnErrorSkip | OnErrorDefault | OnErrorRetry


class TestConfig(Struct, kw_only=True):
    stepflowConfig: (
        Annotated[Any, Meta(description='Stepflow configuration specific to tests.')]
        | None
    ) = None
    cases: (
        Annotated[List[TestCase], Meta(description='Test cases for the workflow.')]
        | None
    ) = None


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
