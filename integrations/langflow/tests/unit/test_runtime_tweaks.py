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

"""Tests for runtime tweaks converter."""

import pytest
from stepflow_py import Flow, FlowBuilder

from stepflow_langflow_integration.converter.runtime_tweaks import (
    convert_langflow_tweaks_to_overrides,
    convert_langflow_tweaks_to_overrides_dict,
)


def test_convert_langflow_tweaks_to_overrides_empty():
    """Test conversion with no tweaks."""
    # Mock flow (validation not used with empty tweaks)
    builder = FlowBuilder()
    builder.set_output({"result": "test"})
    flow = builder.build()
    
    result = convert_langflow_tweaks_to_overrides(flow, {})
    assert result == {"stepOverrides": {}}
    
    result = convert_langflow_tweaks_to_overrides(flow, None)
    assert result == {"stepOverrides": {}}


def test_convert_langflow_tweaks_to_overrides_basic():
    """Test basic tweak to override conversion."""
    # Mock flow with langflow UDF step (no validation for this test)
    tweaks = {
        "LanguageModelComponent-kBOja": {
            "api_key": "new_test_key",
            "temperature": 0.8,
            "model": "gpt-4"
        },
        "ProcessorComponent-xyz": {
            "batch_size": 32,
            "debug": True
        }
    }
    
    # Test without validation to focus on conversion logic
    builder = FlowBuilder()
    builder.set_output({"result": "test"})
    flow = builder.build()
    result = convert_langflow_tweaks_to_overrides(flow, tweaks, validate=False)
    
    expected = {
        "stepOverrides": {
            "langflow_LanguageModelComponent-kBOja": {
                "input.api_key": "new_test_key",
                "input.temperature": 0.8,
                "input.model": "gpt-4"
            },
            "langflow_ProcessorComponent-xyz": {
                "input.batch_size": 32,
                "input.debug": True
            }
        }
    }
    
    assert result == expected


def test_convert_langflow_tweaks_to_overrides_dict():
    """Test conversion for workflow dict format."""
    workflow_dict = {
        "steps": [
            {
                "id": "langflow_TestComponent-abc",
                "component": "/langflow/udf_executor",
                "input": {"input": {"original_param": "original_value"}}
            }
        ]
    }
    
    tweaks = {
        "TestComponent-abc": {
            "new_param": "tweaked_value",
            "override_param": 42
        }
    }
    
    result = convert_langflow_tweaks_to_overrides_dict(workflow_dict, tweaks, validate=False)
    
    expected = {
        "stepOverrides": {
            "langflow_TestComponent-abc": {
                "input.new_param": "tweaked_value", 
                "input.override_param": 42
            }
        }
    }
    
    assert result == expected


def test_validation_missing_components():
    """Test validation fails for missing components."""
    workflow_dict = {
        "steps": [
            {
                "id": "langflow_ExistingComponent-abc",
                "component": "/langflow/udf_executor"
            }
        ]
    }
    
    tweaks = {
        "ExistingComponent-abc": {"param": "value"},  # This exists
        "MissingComponent-xyz": {"param": "value"}    # This doesn't exist
    }
    
    with pytest.raises(ValueError) as exc_info:
        convert_langflow_tweaks_to_overrides_dict(workflow_dict, tweaks, validate=True)
    
    error_msg = str(exc_info.value)
    assert "MissingComponent-xyz" in error_msg
    assert "ExistingComponent-abc" in error_msg
    assert "Missing components:" in error_msg
    assert "Available components:" in error_msg


def test_validation_success():
    """Test validation passes when all components exist."""
    workflow_dict = {
        "steps": [
            {
                "id": "langflow_ComponentA-abc",
                "component": "/langflow/udf_executor"
            },
            {
                "id": "langflow_ComponentB-xyz",
                "component": "/langflow/udf_executor"
            },
            {
                "id": "non_langflow_step",
                "component": "/builtin/openai"
            }
        ]
    }
    
    tweaks = {
        "ComponentA-abc": {"param1": "value1"},
        "ComponentB-xyz": {"param2": "value2"}
    }
    
    # Should not raise an exception
    result = convert_langflow_tweaks_to_overrides_dict(workflow_dict, tweaks, validate=True)
    
    expected = {
        "stepOverrides": {
            "langflow_ComponentA-abc": {
                "input.param1": "value1"
            },
            "langflow_ComponentB-xyz": {
                "input.param2": "value2"
            }
        }
    }
    
    assert result == expected


def test_ignore_non_langflow_steps():
    """Test that non-Langflow steps are ignored in validation."""
    workflow_dict = {
        "steps": [
            {
                "id": "langflow_LangflowComponent-abc",
                "component": "/langflow/udf_executor"
            },
            {
                "id": "builtin_step",
                "component": "/builtin/openai"
            },
            {
                "id": "langflow_ComponentB-xyz_blob",  # This is a blob step
                "component": "/langflow/udf_executor"
            }
        ]
    }
    
    tweaks = {
        "LangflowComponent-abc": {"param": "value"}
    }
    
    # Should only find the one valid Langflow UDF step
    result = convert_langflow_tweaks_to_overrides_dict(workflow_dict, tweaks, validate=True)
    
    expected = {
        "stepOverrides": {
            "langflow_LangflowComponent-abc": {
                "input.param": "value"
            }
        }
    }
    
    assert result == expected


def test_conversion_functions_consistency():
    """Test that both dict and Flow-based conversion functions work consistently."""
    from stepflow_py import FlowBuilder
    
    # Create a minimal workflow dict
    workflow_dict = {
        "steps": [
            {
                "id": "langflow_TestComponent-123", 
                "component": "/langflow/udf_executor"
            }
        ]
    }
    
    # Create equivalent Flow object
    builder = FlowBuilder()
    builder.set_output({"result": "test"})
    flow = builder.build()
    
    tweaks = {
        "TestComponent-123": {
            "api_key": "test_key",
            "temperature": 0.5
        }
    }
    
    # Test that both conversion methods produce the same result (when validation is disabled)
    result1 = convert_langflow_tweaks_to_overrides_dict(workflow_dict, tweaks, validate=False)
    result2 = convert_langflow_tweaks_to_overrides(flow, tweaks, validate=False)
    
    assert result1 == result2
    
    expected = {
        "stepOverrides": {
            "langflow_TestComponent-123": {
                "input.api_key": "test_key",
                "input.temperature": 0.5
            }
        }
    }
    
    assert result1 == expected


def test_complex_tweaks():
    """Test tweaks with various value types."""
    tweaks = {
        "ComplexComponent-test": {
            "string_param": "text_value",
            "int_param": 42,
            "float_param": 3.14,
            "bool_param": True,
            "null_param": None,
            "list_param": ["item1", "item2"],
            "dict_param": {"nested": "value"}
        }
    }
    
    builder = FlowBuilder()
    builder.set_output({"result": "test"})
    flow = builder.build()
    result = convert_langflow_tweaks_to_overrides(flow, tweaks, validate=False)
    
    expected = {
        "stepOverrides": {
            "langflow_ComplexComponent-test": {
                "input.string_param": "text_value",
                "input.int_param": 42,
                "input.float_param": 3.14,
                "input.bool_param": True,
                "input.null_param": None,
                "input.list_param": ["item1", "item2"],
                "input.dict_param": {"nested": "value"}
            }
        }
    }
    
    assert result == expected


if __name__ == "__main__":
    pytest.main([__file__])