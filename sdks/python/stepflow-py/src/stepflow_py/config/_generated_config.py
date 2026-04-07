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

from typing import Annotated, Any, TypeAlias

from msgspec import UNSET, Meta, Struct, UnsetType, convert, field


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


class GrpcPluginConfig(Struct, kw_only=True, tag_field='type', tag='grpc'):
    command: (
        Annotated[
            str | None,
            Meta(
                description='Command to launch the worker subprocess.\nIf not set, the plugin expects an external worker to connect.'
            ),
        ]
        | UnsetType
    ) = UNSET
    args: (
        Annotated[
            list[str], Meta(description='Arguments for the worker subprocess command.')
        ]
        | UnsetType
    ) = UNSET
    env: (
        Annotated[
            dict[str, str],
            Meta(description='Environment variables for the worker subprocess.'),
        ]
        | UnsetType
    ) = UNSET
    queueName: (
        Annotated[
            str | None,
            Meta(
                description="Queue name the worker uses to receive tasks. Required — must match\n`STEPFLOW_QUEUE_NAME` in the worker's environment."
            ),
        ]
        | UnsetType
    ) = UNSET
    queueTimeoutSecs: (
        Annotated[
            int,
            Meta(
                description='Maximum time (in seconds) a task can wait in the queue for a\nworker to send its first heartbeat. If no worker picks up the task\nwithin this window, it is treated as failed. Must be greater than 0.\n\nDefaults to 30 seconds.',
            ),
        ]
        | UnsetType
    ) = 30
    executionTimeoutSecs: (
        Annotated[
            int | None,
            Meta(
                description='Maximum time (in seconds) from first heartbeat to `CompleteTask`.\nIf the worker does not complete within this window, the task is\ntreated as failed. Heartbeat-based crash detection (5s timeout)\nprovides faster detection of hard worker crashes.\n\nDefaults to `null` (no execution timeout — relies on heartbeat\ncrash detection only).',
            ),
        ]
        | UnsetType
    ) = UNSET


class NatsPluginConfig(Struct, kw_only=True, tag_field='type', tag='nats'):
    url: Annotated[
        str, Meta(description='NATS server URL (e.g., "nats://localhost:4222").')
    ]
    stream: (
        Annotated[
            str | None,
            Meta(
                description='Default JetStream stream name. Can be overridden per-route via\nthe `stream` route param. At least one of plugin-level or\nroute-level `stream` must be set.'
            ),
        ]
        | UnsetType
    ) = UNSET
    consumer: (
        Annotated[
            str | None,
            Meta(
                description='Durable consumer name for NATS JetStream. Required — workers use\nthis to create/resume a durable pull consumer within the stream.'
            ),
        ]
        | UnsetType
    ) = UNSET
    command: (
        Annotated[
            str | None,
            Meta(
                description='Command to launch the worker subprocess.\nIf not set, the plugin expects an external worker to connect.'
            ),
        ]
        | UnsetType
    ) = UNSET
    args: (
        Annotated[
            list[str], Meta(description='Arguments for the worker subprocess command.')
        ]
        | UnsetType
    ) = UNSET
    env: (
        Annotated[
            dict[str, str],
            Meta(description='Environment variables for the worker subprocess.'),
        ]
        | UnsetType
    ) = UNSET
    queueTimeoutSecs: (
        Annotated[
            int,
            Meta(
                description='Maximum time (in seconds) a task can wait in the queue for a\nworker to send its first heartbeat. Defaults to 30 seconds.',
            ),
        ]
        | UnsetType
    ) = 30
    executionTimeoutSecs: (
        Annotated[
            int | None,
            Meta(
                description='Maximum time (in seconds) from first heartbeat to `CompleteTask`.\nDefaults to `null` (no execution timeout).',
            ),
        ]
        | UnsetType
    ) = UNSET


JsonPath: TypeAlias = Annotated[
    str, Meta(description='JSON path expression to apply to the referenced value.')
]


Value: TypeAlias = Annotated[
    Any,
    Meta(
        description='Any JSON value (object, array, string, number, boolean, or null)'
    ),
]


class SqlStateStoreConfig(Struct, kw_only=True):
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
            ),
        ]
        | UnsetType
    ) = 30
    maxStartupRecovery: (
        Annotated[
            int,
            Meta(
                description='Maximum number of runs to recover on startup.\n\nLimits how many interrupted runs are recovered when the orchestrator\nstarts. Set to 0 to disable startup recovery. Default: 100.',
            ),
        ]
        | UnsetType
    ) = 100
    maxClaimsPerCheck: (
        Annotated[
            int,
            Meta(
                description='Maximum number of orphaned runs to claim per check interval.\n\nLimits how many runs are claimed in each periodic check to avoid\noverwhelming a single orchestrator. Default: 10.',
            ),
        ]
        | UnsetType
    ) = 10
    leaseTtlSecs: (
        Annotated[
            int,
            Meta(
                description='TTL in seconds for the orchestrator lease and heartbeats.\n\nThe heartbeat interval is automatically set to `lease_ttl_secs / 3`.\nIf an orchestrator stops sending heartbeats, its lease expires after this\nduration and its runs become eligible for recovery. Default: 30 seconds.',
            ),
        ]
        | UnsetType
    ) = 30
    checkpointInterval: (
        Annotated[
            int,
            Meta(
                description='Number of journal entries between checkpoints.\n\nThe executor periodically serializes execution state so that recovery\nonly needs to replay events after the checkpoint instead of from the\nbeginning. Set to 0 to disable. Default: 1000.',
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
            ),
        ]
        | UnsetType
    ) = UNSET


class BuiltinPluginConfig(Struct, kw_only=True, tag_field='type', tag='builtin'):
    pass


class MockPlugin(Struct, kw_only=True, tag_field='type', tag='mock'):
    pass


class InMemoryStore(Struct, kw_only=True, tag_field='type', tag='inMemory'):
    pass


class SqliteStore(Struct, kw_only=True, tag_field='type', tag='sqlite'):
    databaseUrl: str
    maxConnections: Annotated[int, Meta(ge=0)] | UnsetType = 10
    autoMigrate: bool | UnsetType = True


class PostgresStore(Struct, kw_only=True, tag_field='type', tag='postgres'):
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


SupportedPluginConfig: TypeAlias = (
    BuiltinPluginConfig
    | MockPlugin
    | McpPluginConfig
    | GrpcPluginConfig
    | NatsPluginConfig
)


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
    InMemoryStore | SqliteStore | PostgresStore | FilesystemStore,
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


BackoffConfig: TypeAlias = Annotated[
    BackoffConfigConstant | BackoffConfigExponential | BackoffConfigFibonacci,
    Meta(description='Backoff strategy for retry delays.'),
]


class RouteRule(Struct, kw_only=True):
    plugin: Annotated[str, Meta(description='Plugin name to route to.')]
    conditions: (
        Annotated[
            list[InputCondition],
            Meta(
                description='Optional input conditions that must match for this rule to apply.'
            ),
        ]
        | UnsetType
    ) = UNSET
    componentAllow: (
        Annotated[
            list[str],
            Meta(
                description='Optional component allowlist — only these component IDs are allowed.\n\nIf omitted, all components are allowed.'
            ),
        ]
        | UnsetType
    ) = UNSET
    componentDeny: (
        Annotated[
            list[str],
            Meta(
                description='Optional component denylist — these component IDs are blocked.\n\nIf omitted, no components are blocked.'
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
        description='Storage configuration supporting both simple and expanded forms.\n\n# Simple form (all stores share one backend)\n```yaml\nstorageConfig:\n  type: sqlite\n  databaseUrl: "sqlite:workflow_state.db"\n```\n\n# Expanded form (individual configs per store)\n```yaml\nstorageConfig:\n  metadata:\n    type: sqlite\n    databaseUrl: "sqlite:workflow_state.db"\n  blobs:\n    type: sqlite\n    databaseUrl: "sqlite:workflow_state.db"\n  journal:\n    type: inMemory\n```'
    ),
]


class RetryConfig(Struct, kw_only=True):
    transportMaxRetries: (
        Annotated[
            int,
            Meta(
                description="Maximum number of retries for transport errors (default: 3).\n\nTransport errors are infrastructure-level failures — subprocess crashes,\nnetwork timeouts, connection refused — where the component never ran or\ndidn't complete.",
            ),
        ]
        | UnsetType
    ) = 3
    backoff: (
        Annotated[
            BackoffConfig | None,
            Meta(
                description='Backoff strategy for all retry delays (default: fibonacci with 1s min, 10s max).\n\nThis backoff applies to both transport error retries and component error\nretries. The same delay progression is used regardless of retry reason.'
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


class StepflowConfig(Struct, kw_only=True):
    plugins: dict[str, SupportedPluginConfig]
    routes: Annotated[
        dict[str, list[RouteRule]],
        Meta(
            description='Prefix-to-routing rules mapping.\n\nKeys must be either "/" (catch-all) or a single-segment prefix like\n"/python", "/builtin". Multi-segment prefixes are rejected at build time.\nEach plugin\'s registered component paths are mounted under the prefix.\n\nValue: ordered list of routing rules. When multiple rules exist for a prefix,\nthey are evaluated in order — the first rule whose conditions match is used.'
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
            StorageConfig | None,
            Meta(
                description='Storage configuration. If not specified, uses in-memory storage.'
            ),
        ]
        | UnsetType
    ) = field(default_factory=lambda: {'type': 'inMemory'})
    leaseManager: (
        Annotated[
            LeaseManagerConfig | None,
            Meta(
                description='Lease manager configuration for distributed coordination.\nIf not specified, uses no-op (single orchestrator mode).'
            ),
        ]
        | UnsetType
    ) = field(default_factory=lambda: {'type': 'noOp'})
    recovery: (
        Annotated[
            RecoveryConfig | None,
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
            BlobApiConfig | None,
            Meta(
                description='Blob API configuration.\nControls whether the orchestrator serves blob endpoints and the URL workers use.'
            ),
        ]
        | UnsetType
    ) = field(default_factory=lambda: convert({'enabled': True}, type=BlobApiConfig))
    retry: (
        Annotated[
            RetryConfig | None,
            Meta(
                description='Retry configuration.\nControls backoff for all retries and the retry limit for transport errors\n(subprocess crash, network timeout, connection refused).'
            ),
        ]
        | UnsetType
    ) = field(
        default_factory=lambda: convert(
            {
                'transportMaxRetries': 3,
                'backoff': {
                    'type': 'fibonacci',
                    'minDelayMs': 1000,
                    'maxDelayMs': 10000,
                },
            },
            type=RetryConfig,
        )
    )
