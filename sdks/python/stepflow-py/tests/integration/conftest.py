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

"""Pytest configuration for integration tests.

These tests require a running stepflow-server and are skipped if
STEPFLOW_DEV_BINARY environment variable is not set.
"""

import asyncio
import os
from pathlib import Path

import pytest
import pytest_asyncio

from stepflow_py import StepflowClient
from stepflow_py.config import (
    BuiltinPluginConfig,
    InMemoryStateStoreConfig,
    RouteRule,
    StepflowConfig,
    StepflowSubprocessPluginConfig,
)


# Configure pytest-asyncio to use module-scoped event loops
@pytest.fixture(scope="module")
def event_loop():
    """Create an event loop for module-scoped async fixtures."""
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


@pytest.fixture(scope="module")
def integration_config():
    """Create configuration for integration tests.

    Uses the Python SDK worker for UDF components.
    Returns a StepflowConfig object (passed to orchestrator via stdin).
    """
    # Get path to Python SDK for UDF component
    python_sdk_path = Path(__file__).parent.parent.parent.parent

    return StepflowConfig(
        plugins={
            "builtin": BuiltinPluginConfig(),
            "python": StepflowSubprocessPluginConfig(
                command="uv",
                args=[
                    "--project",
                    str(python_sdk_path),
                    "run",
                    "stepflow_worker",
                ],
            ),
        },
        routes={
            "/builtin/{*component}": [RouteRule(plugin="builtin")],
            "/python/{*component}": [RouteRule(plugin="python")],
        },
        stateStore=InMemoryStateStoreConfig(type="inMemory"),
    )


@pytest_asyncio.fixture(scope="module")
async def stepflow_client(integration_config):
    """Start local orchestrator and return connected StepflowClient.

    Requires STEPFLOW_DEV_BINARY environment variable to be set.
    """
    # Check that STEPFLOW_DEV_BINARY is set
    dev_binary = os.environ.get("STEPFLOW_DEV_BINARY")
    if not dev_binary:
        pytest.skip(
            "STEPFLOW_DEV_BINARY environment variable not set. "
            "Set it to the path of the stepflow-server binary."
        )

    # Use StepflowClient.local() which handles orchestrator lifecycle
    async with StepflowClient.local(integration_config, startup_timeout=60.0) as client:
        yield client
