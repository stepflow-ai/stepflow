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

"""Stepflow configuration types.

These types are generated from schemas/stepflow-config.json and can be used
to programmatically create configuration for the Stepflow orchestrator.

Note: Some helper types are manually defined here because the code generator
doesn't properly handle allOf references that should merge fields. These types
combine the transport/plugin fields with their discriminator tags.
"""

from __future__ import annotations

from msgspec import Struct

from stepflow_py.config._generated_config import (
    BuiltinPluginConfig,
    GrpcPluginConfig,
    InputCondition,
    McpPluginConfig,
    MockPlugin,
    NatsPluginConfig,
    RouteRule,
    SqlStateStoreConfig,
    StorageConfig,
    StoreConfig,
    SupportedPluginConfig,
)
from stepflow_py.config._generated_config import (
    InMemoryStore as InMemoryStoreConfig,
)
from stepflow_py.config._generated_config import (
    PostgresStore as PostgresStoreConfig,
)
from stepflow_py.config._generated_config import (
    SqliteStore as SqliteStoreConfig,
)

# Backward-compatible alias
SqliteStateStoreConfig = SqlStateStoreConfig

# Type alias for all plugin configs that can be used in StepflowConfig.plugins
PluginConfig = (
    GrpcPluginConfig
    | BuiltinPluginConfig
    | McpPluginConfig
    | MockPlugin
    | NatsPluginConfig
)


# ============================================================================
# StepflowConfig (re-defined with proper plugin types)
# ============================================================================


class StepflowConfig(Struct, kw_only=True):
    """Complete Stepflow server configuration.

    Example:
        config = StepflowConfig(
            plugins={
                "builtin": BuiltinPluginConfig(),
                "python": GrpcPluginConfig(
                    command="uv",
                    args=["--project", "/path/to/sdk", "run", "stepflow_worker"],
                    queueName="python",
                ),
            },
            routes={
                "/builtin": [RouteRule(plugin="builtin")],
                "/python": [RouteRule(plugin="python")],
            },
            storageConfig=InMemoryStoreConfig(),
        )
    """

    plugins: dict[str, PluginConfig]
    routes: dict[str, list[RouteRule]]
    workingDirectory: str | None = None
    storageConfig: StorageConfig | None = None


__all__ = [
    # Main config
    "StepflowConfig",
    # Storage config
    "StorageConfig",
    "StoreConfig",
    "InMemoryStoreConfig",
    "SqliteStoreConfig",
    "PostgresStoreConfig",
    "SqlStateStoreConfig",
    "SqliteStateStoreConfig",
    # Routing
    "RouteRule",
    "InputCondition",
    # Plugins
    "PluginConfig",
    "GrpcPluginConfig",
    "BuiltinPluginConfig",
    "McpPluginConfig",
    "MockPlugin",
    "NatsPluginConfig",
    # Lower-level types (from generated code)
    "SupportedPluginConfig",
]
