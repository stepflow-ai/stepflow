"""Pytest configuration and fixtures."""

import json
import pytest
from pathlib import Path
from typing import Dict, Any

from stepflow_langflow_integration.converter.translator import LangflowConverter


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
def simple_langflow_workflow() -> Dict[str, Any]:
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
                                    "info": "Message to be passed as input"
                                }
                            }
                        },
                        "outputs": [
                            {
                                "name": "message",
                                "method": "message_response",
                                "types": ["Message"]
                            }
                        ]
                    }
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
                                    "info": "Message to be passed as output"
                                }
                            }
                        },
                        "outputs": [
                            {
                                "name": "message",
                                "method": "message_response", 
                                "types": ["Message"]
                            }
                        ]
                    }
                }
            ],
            "edges": [
                {
                    "source": "ChatInput-1",
                    "target": "ChatOutput-2",
                    "source_handle": "message",
                    "target_handle": "input_value"
                }
            ]
        }
    }


@pytest.fixture
def converter() -> LangflowConverter:
    """LangflowConverter instance for testing."""
    return LangflowConverter()


@pytest.fixture
def validating_converter() -> LangflowConverter:
    """LangflowConverter with validation enabled."""
    return LangflowConverter(validate_schemas=True)