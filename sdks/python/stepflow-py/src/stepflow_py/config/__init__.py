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

from typing import Literal

from msgspec import Struct

from stepflow_py.config._generated_config import (
    GrpcPluginConfig,
    InputCondition,
    McpPluginConfig,
    MockComponent,
    MockComponentBehavior,
    MockPlugin,
    RouteRule,
    SqliteStateStoreConfig,
    StorageConfig,
    StoreConfig,
    SupportedPluginConfig,
)
from stepflow_py.config._generated_config import (
    InMemoryStore as InMemoryStoreConfig,
)
from stepflow_py.config._generated_config import (
    SqliteStore as SqliteStoreConfig,
)

# ============================================================================
# Plugin Config Helper Types
# ============================================================================
# These types properly combine transport/plugin fields with discriminator tags.
# The code generator creates empty classes for allOf references, so we define
# proper types here with all required fields.


class BuiltinPluginConfig(Struct, kw_only=True):
    """Built-in plugin configuration."""

    type: Literal["builtin"] = "builtin"


class McpPluginConfigTagged(Struct, kw_only=True):
    """MCP (Model Context Protocol) plugin configuration."""

    type: Literal["mcp"] = "mcp"
    command: str
    args: list[str]
    env: dict[str, str] | None = None


class MockPluginConfig(Struct, kw_only=True):
    """Mock plugin configuration (for testing)."""

    type: Literal["mock"] = "mock"
    components: dict[str, MockComponent]


# Type alias for all plugin configs that can be used in StepflowConfig.plugins
PluginConfig = (
    GrpcPluginConfig | BuiltinPluginConfig | McpPluginConfigTagged | MockPluginConfig
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
    "SqliteStateStoreConfig",
    # Routing
    "RouteRule",
    "InputCondition",
    # Plugins (helper types with proper fields)
    "PluginConfig",
    "GrpcPluginConfig",
    "BuiltinPluginConfig",
    "McpPluginConfigTagged",
    "MockPluginConfig",
    # Lower-level types (from generated code)
    "SupportedPluginConfig",
    "McpPluginConfig",
    # Mock
    "MockPlugin",
    "MockComponent",
    "MockComponentBehavior",
]
