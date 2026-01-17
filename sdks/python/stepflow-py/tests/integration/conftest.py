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
from stepflow_orchestrator import OrchestratorConfig, StepflowOrchestrator

from stepflow_py import StepflowClient


# Configure pytest-asyncio to use module-scoped event loops
@pytest.fixture(scope="module")
def event_loop():
    """Create an event loop for module-scoped async fixtures."""
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


@pytest.fixture(scope="module")
def integration_config(tmp_path_factory):
    """Create configuration for integration tests.

    Uses the Python SDK worker for UDF components.
    """
    import tempfile

    import yaml

    # Get path to Python SDK for UDF component
    python_sdk_path = Path(__file__).parent.parent.parent.parent

    config_dict = {
        "plugins": {
            "builtin": {"type": "builtin"},
            "python": {
                "type": "stepflow",
                "command": "uv",
                "args": [
                    "--project",
                    str(python_sdk_path),
                    "run",
                    "stepflow_worker",
                ],
            },
        },
        "routes": {
            "/builtin/{*component}": [{"plugin": "builtin"}],
            "/python/{*component}": [{"plugin": "python"}],
        },
        "stateStore": {"type": "inMemory"},
    }

    # Write config to temporary file
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".yml", delete=False
    ) as config_file:
        yaml.dump(config_dict, config_file, default_flow_style=False, sort_keys=False)
        config_path = Path(config_file.name)

    yield str(config_path)

    # Cleanup
    config_path.unlink(missing_ok=True)


@pytest_asyncio.fixture(scope="module")
async def stepflow_client(integration_config):
    """Start StepflowOrchestrator and return connected StepflowClient.

    Requires STEPFLOW_DEV_BINARY environment variable to be set.
    """
    # Check that STEPFLOW_DEV_BINARY is set
    dev_binary = os.environ.get("STEPFLOW_DEV_BINARY")
    if not dev_binary:
        pytest.skip(
            "STEPFLOW_DEV_BINARY environment variable not set. "
            "Set it to the path of the stepflow-server binary."
        )

    # Create orchestrator config
    orch_config = OrchestratorConfig(
        config_path=Path(integration_config),
        startup_timeout=60.0,
    )

    # Start orchestrator
    orchestrator = StepflowOrchestrator(orch_config)
    await orchestrator._start()

    # Create client
    client = StepflowClient.connect(orchestrator.url)

    yield client

    # Cleanup
    await client.close()
    await orchestrator._stop()
