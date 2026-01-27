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

# Auto-generated stepflow-config types from schemas/stepflow-config.json
# To regenerate this file, run:
#   uv run python generate.py

from __future__ import annotations

from typing import Annotated, Any, ClassVar, Dict, List, Literal

from msgspec import Meta, Struct


class StateStoreConfig1(Struct, kw_only=True):
    type: Literal['inMemory']


BuiltinPluginConfig = Any


class McpPluginConfig(Struct, kw_only=True):
    command: str
    args: List[str]
    env: (
        Annotated[
            Dict[str, str],
            Meta(
                description='Environment variables to pass to the MCP server process.\nValues can contain environment variable references like ${HOME} or ${USER:-default}.'
            ),
        ]
        | None
    ) = None


class StepflowTransport2(Struct, kw_only=True):
    url: str


class HealthCheckConfig(Struct, kw_only=True):
    path: (
        Annotated[
            str, Meta(description='Health check endpoint path. Default: "/health"')
        ]
        | None
    ) = None
    timeoutMs: (
        Annotated[
            int,
            Meta(
                description='Total timeout in milliseconds for the health check to pass. Default: 60000 (60s)',
                ge=0,
            ),
        ]
        | None
    ) = None
    retryDelayMs: (
        Annotated[
            int,
            Meta(
                description='Delay between health check attempts in milliseconds. Default: 100',
                ge=0,
            ),
        ]
        | None
    ) = None


class Schema(Struct, kw_only=True):
    pass


class MockComponentBehavior1(Struct, kw_only=True):
    error: str


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


JsonPath = Annotated[
    str,
    Meta(
        description='JSON path expression to apply to the referenced value. May use `$` to reference the whole value. May also be a bare field name (without the leading $) if the referenced value is an object.',
        examples=['field', '$.field', '$["field"]', '$[0]', '$.field[0].nested'],
    ),
]


class SqliteStateStoreConfig(Struct, kw_only=True):
    databaseUrl: str
    maxConnections: Annotated[int, Meta(ge=0)] | None = None
    autoMigrate: bool | None = None


class StateStoreConfig2(SqliteStateStoreConfig, kw_only=True):
    type: Literal['sqlite']


StateStoreConfig = StateStoreConfig1 | StateStoreConfig2


class SupportedPlugin2(Struct, kw_only=True):
    type: Literal['builtin']


class SupportedPlugin4(McpPluginConfig, kw_only=True):
    type: Literal['mcp']


class StepflowTransport1(Struct, kw_only=True):
    command: str
    args: List[str] | None = None
    env: (
        Annotated[
            Dict[str, str],
            Meta(
                description='Environment variables to pass to the subprocess.\nValues can contain environment variable references like ${HOME} or ${USER:-default}.'
            ),
        ]
        | None
    ) = None
    healthCheck: HealthCheckConfig | None = None


StepflowTransport = Annotated[
    StepflowTransport1 | StepflowTransport2,
    Meta(
        description='Configuration for Stepflow plugin transport.\n\nEither `command` or `url` must be provided (but not both):\n- `command`: Launch a subprocess HTTP server\n- `url`: Connect to an existing HTTP server'
    ),
]


FlowResult = Annotated[
    FlowResultSuccess | FlowResultFailed,
    Meta(description='The results of a step execution.', title='FlowResult'),
]


class InputCondition(Struct, kw_only=True):
    path: Annotated[
        JsonPath,
        Meta(
            description='JSON path expression (e.g., "$.model", "$.config.temperature")'
        ),
    ]
    value: Annotated[
        Value, Meta(description='Value to match against (equality comparison)')
    ]


class StepflowPluginConfig(Struct, kw_only=True):
    pass


MockComponentBehavior = Annotated[
    MockComponentBehavior1 | FlowResult,
    Meta(description='Enumeration of behaviors for the mock components.'),
]


class RouteRule(Struct, kw_only=True):
    plugin: Annotated[str, Meta(description='Plugin name to route to')]
    conditions: (
        Annotated[
            List[InputCondition],
            Meta(
                description='Optional input conditions that must match for this rule to apply'
            ),
        ]
        | None
    ) = None
    componentAllow: (
        Annotated[
            List[str],
            Meta(
                description='Optional component allowlist - only these components are allowed to match this rule\n\nIf omitted, all components are allowed to match.'
            ),
        ]
        | None
    ) = None
    componentDeny: (
        Annotated[
            List[str],
            Meta(
                description='Optional component denylist - these components are blocked from matching this rule\n\nIf omitted, no components are blocked.'
            ),
        ]
        | None
    ) = None
    component: (
        Annotated[
            str | None,
            Meta(
                description='Component name to pass to the plugin.\nDefaults to `/{component}` if not specified, meaning the extracted component name is used.\n\nMay be a pattern referencing path placeholders, e.g., "{component}" or "{*component}".'
            ),
        ]
        | None
    ) = None


class RoutingConfig(Struct, kw_only=True):
    routes: Annotated[
        Dict[str, List[RouteRule]],
        Meta(
            description='Path-to-routing rules mapping\n\nKeys describe paths. For example "/python/{component}" or "/openai/{component}".\nPlaceholders may match a single segment (e.g., "{component}") or multiple segments (e.g., "{*component}").\n\nValue: ordered list of routing rules to apply to that path.\n\nRoutes will be applied in the order they are listed, with the first matching rule being used.'
        ),
    ]


class SupportedPlugin1(StepflowPluginConfig, kw_only=True):
    type: Literal['stepflow']


class MockComponent(Struct, kw_only=True):
    input_schema: Schema
    output_schema: Schema
    behaviors: Dict[str, MockComponentBehavior]


class MockPlugin(Struct, kw_only=True):
    components: Dict[str, MockComponent]


class SupportedPlugin3(MockPlugin, kw_only=True):
    type: Literal['mock']


SupportedPlugin = (
    SupportedPlugin1 | SupportedPlugin2 | SupportedPlugin3 | SupportedPlugin4
)


class SupportedPluginConfig(Struct, kw_only=True):
    pass


class StepflowConfig(RoutingConfig, kw_only=True):
    plugins: Dict[str, SupportedPluginConfig]
    workingDirectory: (
        Annotated[
            str | None,
            Meta(
                description='Working directory for the configuration.\n\nIf not set, this will be the directory containing the config.'
            ),
        ]
        | None
    ) = None
    stateStore: (
        Annotated[
            StateStoreConfig,
            Meta(
                description='State store configuration. If not specified, uses in-memory storage.'
            ),
        ]
        | None
    ) = None
