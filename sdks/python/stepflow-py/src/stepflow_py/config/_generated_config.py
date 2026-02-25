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

from typing import Annotated, Any, Literal, TypeAlias

from msgspec import UNSET, Meta, Struct, UnsetType, convert, field


class HealthCheckConfig(Struct, kw_only=True):
    path: (
        Annotated[
            str, Meta(description='Health check endpoint path. Default: "/health"')
        ]
        | UnsetType
    ) = '/health'
    timeoutMs: (
        Annotated[
            int,
            Meta(
                description='Total timeout in milliseconds for the health check to pass. Default: 60000 (60s)',
                ge=0,
            ),
        ]
        | UnsetType
    ) = 60000
    retryDelayMs: (
        Annotated[
            int,
            Meta(
                description='Delay between health check attempts in milliseconds. Default: 100',
                ge=0,
            ),
        ]
        | UnsetType
    ) = 100


class Schema(Struct, kw_only=True):
    pass


class MockComponentError(Struct, kw_only=True):
    error: str


Value: TypeAlias = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


class FlowError(Struct, kw_only=True):
    code: int
    message: str
    data: Value | None | UnsetType = UNSET


class McpPluginConfig(Struct, kw_only=True, tag_field='type', tag='mcp'):
    command: str
    args: list[str]
    env: (
        Annotated[
            dict[str, str],
            Meta(
                description='Environment variables to pass to the MCP server process.\nValues can contain environment variable references like ${HOME} or ${USER:-default}.'
            ),
        ]
        | UnsetType
    ) = UNSET


JsonPath: TypeAlias = Annotated[
    str, Meta(description='JSON path expression to apply to the referenced value.')
]


class SqliteStateStoreConfig(Struct, kw_only=True):
    databaseUrl: str
    maxConnections: Annotated[int, Meta(ge=0)] | UnsetType = 10
    autoMigrate: bool | UnsetType = True


class FilesystemBlobStoreConfig(Struct, kw_only=True):
    directory: (
        Annotated[
            str | None,
            Meta(
                description='Directory path for storing blobs. If not specified, a temporary directory is used.'
            ),
        ]
        | UnsetType
    ) = UNSET


class EtcdLeaseManagerConfig(Struct, kw_only=True):
    endpoints: Annotated[
        list[str],
        Meta(description='etcd endpoints (e.g., `["http://localhost:2379"]`).'),
    ]
    key_prefix: (
        Annotated[str, Meta(description='Key prefix for all stepflow lease keys.')]
        | UnsetType
    ) = '/stepflow/leases'


class RecoveryConfig(Struct, kw_only=True):
    enabled: (
        Annotated[
            bool,
            Meta(
                description='Whether to enable periodic orphan claiming during execution.\n\nWhen enabled, the orchestrator will periodically check for orphaned\nruns (from crashed orchestrators) and claim them for execution.\nDefault: true'
            ),
        ]
        | UnsetType
    ) = True
    checkIntervalSecs: (
        Annotated[
            int,
            Meta(
                description='Interval in seconds between orphan check attempts.\n\nOnly used when `enabled` is true. Lower values mean faster recovery\nbut more overhead. Default: 30 seconds.',
                ge=0,
            ),
        ]
        | UnsetType
    ) = 30
    maxStartupRecovery: (
        Annotated[
            int,
            Meta(
                description='Maximum number of runs to recover on startup.\n\nLimits how many interrupted runs are recovered when the orchestrator\nstarts. Set to 0 to disable startup recovery. Default: 100.',
                ge=0,
            ),
        ]
        | UnsetType
    ) = 100
    maxClaimsPerCheck: (
        Annotated[
            int,
            Meta(
                description='Maximum number of orphaned runs to claim per check interval.\n\nLimits how many runs are claimed in each periodic check to avoid\noverwhelming a single orchestrator. Default: 10.',
                ge=0,
            ),
        ]
        | UnsetType
    ) = 10
    leaseTtlSecs: (
        Annotated[
            int,
            Meta(
                description='TTL in seconds for the orchestrator lease and heartbeats.\n\nThe heartbeat interval is automatically set to `lease_ttl_secs / 3`.\nIf an orchestrator stops sending heartbeats, its lease expires after this\nduration and its runs become eligible for recovery. Default: 30 seconds.',
                ge=0,
            ),
        ]
        | UnsetType
    ) = 30
    checkpointInterval: (
        Annotated[
            int,
            Meta(
                description='Number of journal entries between checkpoints.\n\nThe executor periodically serializes execution state so that recovery\nonly needs to replay events after the checkpoint instead of from the\nbeginning. Set to 0 to disable. Default: 1000.',
                ge=0,
            ),
        ]
        | UnsetType
    ) = 1000


class BlobApiConfig(Struct, kw_only=True):
    enabled: (
        Annotated[
            bool,
            Meta(
                description='Whether the orchestrator serves blob API endpoints.\n\nSet to `false` when running a separate blob service.\nDefault: `true`'
            ),
        ]
        | UnsetType
    ) = True
    url: (
        Annotated[
            str | None,
            Meta(
                description="URL workers use to access the blob API.\n\nIf not set, defaults to `http://localhost:{port}/api/v1/blobs` where `{port}`\nis the server's bound port.\n\nThis value should be the base blobs endpoint URL. Workers will:\n- `POST {url}` to create blobs\n- `GET {url}/{blob_id}` to fetch blobs\n\nExamples:\n- Local dev: omit (auto-detected)\n- K8s with orchestrator blobs: `http://orchestrator-service/api/v1/blobs`\n- K8s with separate blob service: `http://blob-service/api/v1/blobs`"
            ),
        ]
        | UnsetType
    ) = UNSET
    blobThreshold: (
        Annotated[
            int | None,
            Meta(
                description="Byte size threshold for automatic blobification of component inputs/outputs.\n\nWhen a top-level field in a component's input or output exceeds this size\n(in bytes of JSON serialization), it is automatically stored as a blob and\nreplaced with a `$blob` reference.\n\nSet to `0` to disable automatic blobification.\nDefault: 1 MB (1048576 bytes)",
                ge=0,
            ),
        ]
        | UnsetType
    ) = UNSET


class BuiltinPluginConfig(Struct, kw_only=True, tag_field='type', tag='builtin'):
    pass


class BackoffConfigConstant(Struct, kw_only=True, tag_field='type', tag='constant'):
    delayMs: (
        Annotated[int, Meta(description='Delay in milliseconds (default: 1000).', ge=0)]
        | UnsetType
    ) = 1000


class BackoffConfigExponential(
    Struct, kw_only=True, tag_field='type', tag='exponential'
):
    minDelayMs: (
        Annotated[
            int,
            Meta(description='Starting delay in milliseconds (default: 1000).', ge=0),
        ]
        | UnsetType
    ) = 1000
    maxDelayMs: (
        Annotated[
            int,
            Meta(
                description='Maximum delay cap in milliseconds (default: 10000).', ge=0
            ),
        ]
        | UnsetType
    ) = 10000
    factor: (
        Annotated[float, Meta(description='Multiplier per attempt (default: 2.0).')]
        | UnsetType
    ) = 2.0


class BackoffConfigFibonacci(Struct, kw_only=True, tag_field='type', tag='fibonacci'):
    minDelayMs: (
        Annotated[
            int,
            Meta(description='Starting delay in milliseconds (default: 1000).', ge=0),
        ]
        | UnsetType
    ) = 1000
    maxDelayMs: (
        Annotated[
            int,
            Meta(
                description='Maximum delay cap in milliseconds (default: 10000).', ge=0
            ),
        ]
        | UnsetType
    ) = 10000


class InMemoryStore(Struct, kw_only=True, tag_field='type', tag='inMemory'):
    pass


class SqliteStore(Struct, kw_only=True, tag_field='type', tag='sqlite'):
    databaseUrl: str
    maxConnections: Annotated[int, Meta(ge=0)] | UnsetType = 10
    autoMigrate: bool | UnsetType = True


class FilesystemStore(Struct, kw_only=True, tag_field='type', tag='filesystem'):
    directory: (
        Annotated[
            str | None,
            Meta(
                description='Directory path for storing blobs. If not specified, a temporary directory is used.'
            ),
        ]
        | UnsetType
    ) = UNSET


class NoOpLeaseManager(Struct, kw_only=True, tag_field='type', tag='noOp'):
    pass


class EtcdLeaseManager(Struct, kw_only=True, tag_field='type', tag='etcd'):
    endpoints: Annotated[
        list[str],
        Meta(description='etcd endpoints (e.g., `["http://localhost:2379"]`).'),
    ]
    key_prefix: (
        Annotated[str, Meta(description='Key prefix for all stepflow lease keys.')]
        | UnsetType
    ) = '/stepflow/leases'


BackoffConfig: TypeAlias = Annotated[
    BackoffConfigConstant | BackoffConfigExponential | BackoffConfigFibonacci,
    Meta(
        description='Backoff strategy for retry delays. Each variant carries only its relevant parameters.'
    ),
]


class FlowResultSuccess(Struct, kw_only=True, tag_field='outcome', tag='success'):
    result: Value


class FlowResultFailed(Struct, kw_only=True, tag_field='outcome', tag='failed'):
    error: FlowError


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


StoreConfig: TypeAlias = Annotated[
    InMemoryStore | SqliteStore | FilesystemStore,
    Meta(
        description='Configuration for a single storage backend.\n\nEach variant documents which store types it supports:\n- **metadata**: Flow and run metadata storage\n- **blobs**: Content-addressable blob storage\n- **journal**: Execution journal for recovery'
    ),
]


LeaseManagerConfig: TypeAlias = Annotated[
    NoOpLeaseManager | EtcdLeaseManager,
    Meta(
        description='Configuration for the lease manager used in distributed deployments.\n\nThe lease manager handles run ownership in multi-orchestrator scenarios,\nensuring only one orchestrator executes a given run at a time.'
    ),
]


class RetryConfig(Struct, kw_only=True):
    maxAttempts: (
        Annotated[
            int | None,
            Meta(
                description='Maximum number of retries due to transport errors — subprocess crashes,\nnetwork timeouts, connection failures (default: 3).',
                ge=0,
            ),
        ]
        | UnsetType
    ) = UNSET
    backoff: (
        Annotated[
            BackoffConfig,
            Meta(
                description='Backoff strategy and parameters (default: fibonacci with 1s min, 10s max).\nUsed by the orchestrator to compute delay between retry attempts.'
            ),
        ]
        | UnsetType
    ) = field(
        default_factory=lambda: {
            'type': 'fibonacci',
            'minDelayMs': 1000,
            'maxDelayMs': 10000,
        }
    )


class StepflowSubprocessConfig(Struct, kw_only=True):
    type: Literal['stepflow']
    command: str
    retry: (
        Annotated[
            RetryConfig | None,
            Meta(
                description='Retry configuration for component execution failures (default: 3 attempts, fibonacci backoff).'
            ),
        ]
        | UnsetType
    ) = UNSET
    args: list[str] | UnsetType = UNSET
    env: (
        Annotated[
            dict[str, str],
            Meta(
                description='Environment variables to pass to the subprocess.\nValues can contain environment variable references like ${HOME} or ${USER:-default}.'
            ),
        ]
        | UnsetType
    ) = UNSET
    healthCheck: (
        Annotated[
            HealthCheckConfig | None,
            Meta(description='Health check configuration for the subprocess server.'),
        ]
        | UnsetType
    ) = UNSET


class StepflowRemoteConfig(Struct, kw_only=True):
    type: Literal['stepflow']
    url: str
    retry: (
        Annotated[
            RetryConfig | None,
            Meta(
                description='Retry configuration for component execution failures (default: 3 attempts, fibonacci backoff).'
            ),
        ]
        | UnsetType
    ) = UNSET


StepflowPluginConfig: TypeAlias = Annotated[
    StepflowSubprocessConfig | StepflowRemoteConfig,
    Meta(
        description='Configuration for Stepflow plugin transport.\n\nEither `command` or `url` must be provided (but not both):\n- `command`: Launch a subprocess HTTP server\n- `url`: Connect to an existing HTTP server'
    ),
]


MockComponentResult: TypeAlias = Annotated[
    FlowResultSuccess | FlowResultFailed,
    Meta(
        description='Return the given result (success or flow-error).',
        title='MockComponentResult',
    ),
]


MockComponentBehavior: TypeAlias = Annotated[
    MockComponentError | MockComponentResult,
    Meta(description='Enumeration of behaviors for the mock components.'),
]


class RouteRule(Struct, kw_only=True):
    plugin: Annotated[str, Meta(description='Plugin name to route to')]
    conditions: (
        Annotated[
            list[InputCondition],
            Meta(
                description='Optional input conditions that must match for this rule to apply'
            ),
        ]
        | UnsetType
    ) = UNSET
    componentAllow: (
        Annotated[
            list[str],
            Meta(
                description='Optional component allowlist - only these components are allowed to match this rule\n\nIf omitted, all components are allowed to match.'
            ),
        ]
        | UnsetType
    ) = UNSET
    componentDeny: (
        Annotated[
            list[str],
            Meta(
                description='Optional component denylist - these components are blocked from matching this rule\n\nIf omitted, no components are blocked.'
            ),
        ]
        | UnsetType
    ) = UNSET
    component: (
        Annotated[
            str | None,
            Meta(
                description='Component name to pass to the plugin.\nDefaults to `/{component}` if not specified, meaning the extracted component name is used.\n\nMay be a pattern referencing path placeholders, e.g., "{component}" or "{*component}".'
            ),
        ]
        | UnsetType
    ) = UNSET


class ExpandedStorageConfig(Struct, kw_only=True):
    metadata: Annotated[
        StoreConfig, Meta(description='Configuration for the metadata store')
    ]
    blobs: (
        Annotated[
            StoreConfig | None,
            Meta(
                description='Configuration for the blob store (defaults to metadata config if not specified)'
            ),
        ]
        | UnsetType
    ) = UNSET
    journal: (
        Annotated[
            StoreConfig | None,
            Meta(
                description='Configuration for the execution journal (defaults to metadata config if not specified)'
            ),
        ]
        | UnsetType
    ) = UNSET


StorageConfig: TypeAlias = Annotated[
    ExpandedStorageConfig | StoreConfig,
    Meta(
        description='Storage configuration supporting both simple and expanded forms.\n\n# Simple form (all stores share one backend)\n```yaml\nstorageConfig:\n  type: sqlite\n  databaseUrl: "sqlite:workflow_state.db"\n```\n\n# Expanded form (individual configs per store)\n```yaml\nstorageConfig:\n  metadata:\n    type: sqlite\n    databaseUrl: "sqlite:workflow_state.db"\n  blobs:\n    type: sqlite\n    databaseUrl: "sqlite:workflow_state.db"\n  journal:\n    type: inMemory\n```\n\nWhen multiple stores have identical configurations, they will share\na single backend instance (smart deduplication).'
    ),
]


class MockComponent(Struct, kw_only=True):
    input_schema: Schema
    output_schema: Schema
    behaviors: dict[str, MockComponentBehavior]


class MockPlugin(Struct, kw_only=True, tag_field='type', tag='mock'):
    components: dict[str, MockComponent]


SupportedPluginConfig: TypeAlias = (
    StepflowPluginConfig | BuiltinPluginConfig | MockPlugin | McpPluginConfig
)


class StepflowConfig(Struct, kw_only=True):
    plugins: dict[str, SupportedPluginConfig]
    routes: Annotated[
        dict[str, list[RouteRule]],
        Meta(
            description='Path-to-routing rules mapping\n\nKeys describe paths. For example "/python/{component}" or "/openai/{component}".\nPlaceholders may match a single segment (e.g., "{component}") or multiple segments (e.g., "{*component}").\n\nValue: ordered list of routing rules to apply to that path.\n\nRoutes will be applied in the order they are listed, with the first matching rule being used.'
        ),
    ]
    workingDirectory: (
        Annotated[
            str | None,
            Meta(
                description='Working directory for the configuration.\n\nIf not set, this will be the directory containing the config.'
            ),
        ]
        | UnsetType
    ) = UNSET
    storageConfig: (
        Annotated[
            StorageConfig,
            Meta(
                description='Storage configuration. If not specified, uses in-memory storage.'
            ),
        ]
        | UnsetType
    ) = field(default_factory=lambda: {'type': 'inMemory'})
    leaseManager: (
        Annotated[
            LeaseManagerConfig,
            Meta(
                description='Lease manager configuration for distributed coordination.\nIf not specified, uses no-op (single orchestrator mode).'
            ),
        ]
        | UnsetType
    ) = field(default_factory=lambda: {'type': 'noOp'})
    recovery: (
        Annotated[
            RecoveryConfig,
            Meta(description='Recovery configuration for handling interrupted runs.'),
        ]
        | UnsetType
    ) = field(
        default_factory=lambda: convert(
            {
                'enabled': True,
                'checkIntervalSecs': 30,
                'maxStartupRecovery': 100,
                'maxClaimsPerCheck': 10,
                'leaseTtlSecs': 30,
                'checkpointInterval': 1000,
            },
            type=RecoveryConfig,
        )
    )
    blobApi: (
        Annotated[
            BlobApiConfig,
            Meta(
                description='Blob API configuration.\nControls whether the orchestrator serves blob endpoints and the URL workers use.'
            ),
        ]
        | UnsetType
    ) = field(default_factory=lambda: convert({'enabled': True}, type=BlobApiConfig))
