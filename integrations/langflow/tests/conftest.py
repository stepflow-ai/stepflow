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

"""Pytest configuration and fixtures."""

from pathlib import Path
from typing import Any

import pytest

from stepflow_langflow_integration.converter.translator import LangflowConverter
from tests.helpers.testing.stepflow_binary import StepflowBinaryRunner


# Load environment variables from .env file for tests
def pytest_configure(config):
    """Load .env file for test environment."""
    try:
        from dotenv import load_dotenv

        # Load .env from project root
        env_path = Path(__file__).parent.parent / ".env"
        if env_path.exists():
            load_dotenv(env_path)
            print(f"✅ Loaded .env file for tests: {env_path}")
        else:
            print(f"⚠️ No .env file found at: {env_path}")
    except ImportError:
        print("⚠️ python-dotenv not available - environment variables not loaded")


@pytest.fixture
def fixtures_dir() -> Path:
    """Path to test fixtures directory."""
    return Path(__file__).parent / "fixtures"


@pytest.fixture
def langflow_fixtures_dir(fixtures_dir: Path) -> Path:
    """Path to Langflow JSON fixtures."""
    return fixtures_dir / "langflow"


@pytest.fixture
def stepflow_fixtures_dir(fixtures_dir: Path) -> Path:
    """Path to expected Stepflow YAML fixtures."""
    return fixtures_dir / "stepflow"


@pytest.fixture
def simple_langflow_workflow() -> dict[str, Any]:
    """Simple Langflow workflow for testing."""
    return {
        "data": {
            "nodes": [
                {
                    "id": "ChatInput-1",
                    "data": {
                        "type": "ChatInput",
                        "node": {
                            "template": {
                                "input_value": {
                                    "type": "str",
                                    "value": "",
                                    "info": "Message to be passed as input",
                                }
                            }
                        },
                        "outputs": [
                            {
                                "name": "message",
                                "method": "message_response",
                                "types": ["Message"],
                            }
                        ],
                    },
                },
                {
                    "id": "ChatOutput-2",
                    "data": {
                        "type": "ChatOutput",
                        "node": {
                            "template": {
                                "input_value": {
                                    "type": "str",
                                    "value": "",
                                    "info": "Message to be passed as output",
                                }
                            }
                        },
                        "outputs": [
                            {
                                "name": "message",
                                "method": "message_response",
                                "types": ["Message"],
                            }
                        ],
                    },
                },
            ],
            "edges": [
                {
                    "source": "ChatInput-1",
                    "target": "ChatOutput-2",
                    "source_handle": "message",
                    "target_handle": "input_value",
                }
            ],
        }
    }


@pytest.fixture
def converter() -> LangflowConverter:
    """LangflowConverter instance for testing."""
    return LangflowConverter()


@pytest.fixture(scope="module")
def stepflow_runner():
    """Create stepflow binary runner."""
    try:
        runner = StepflowBinaryRunner()
        available, version = runner.check_binary_availability()
        if not available:
            pytest.skip(f"Stepflow binary not available: {version}")
        return runner
    except FileNotFoundError as e:
        pytest.skip(f"Stepflow binary not found: {e}")
