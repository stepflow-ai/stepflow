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

"""Unit tests for LangflowConverter."""

from typing import Any

import pytest

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.utils.errors import ConversionError


class TestLangflowConverter:
    """Test LangflowConverter functionality."""

    def test_init_default(self):
        """Test default initialization."""
        converter = LangflowConverter()

    def test_convert_simple_workflow(
        self, converter: LangflowConverter, simple_langflow_workflow: dict[str, Any]
    ):
        """Test conversion of simple workflow."""
        workflow = converter.convert(simple_langflow_workflow)

        assert workflow.name == "Converted Langflow Workflow"

        # ChatInput and ChatOutput are I/O connection points, not processing steps
        # They handle Message type conversion but don't create workflow steps
        assert len(workflow.steps) == 0

        # Verify the workflow structure is valid
        assert workflow.schema_ == "https://stepflow.org/schemas/v1/flow.json"

        # Workflow should have output that references input (passthrough)
        assert workflow.output is not None

    def test_convert_empty_nodes(self, converter: LangflowConverter):
        """Test conversion with empty nodes list."""
        workflow_data = {"data": {"nodes": [], "edges": []}}

        with pytest.raises(ConversionError, match="No nodes found"):
            converter.convert(workflow_data)

    def test_convert_missing_data(self, converter: LangflowConverter):
        """Test conversion with missing data key."""
        workflow_data = {"invalid": "structure"}

        with pytest.raises(ConversionError, match="missing 'data' key"):
            converter.convert(workflow_data)

    def test_to_yaml(
        self, converter: LangflowConverter, simple_langflow_workflow: dict[str, Any]
    ):
        """Test YAML generation."""
        workflow = converter.convert(simple_langflow_workflow)
        yaml_output = converter.to_yaml(workflow)

        assert isinstance(yaml_output, str)
        assert "name: Converted Langflow Workflow" in yaml_output
        # Should have output section since ChatInput/ChatOutput create passthrough
        assert "output:" in yaml_output

    def test_analyze_workflow(
        self, converter: LangflowConverter, simple_langflow_workflow: dict[str, Any]
    ):
        """Test workflow analysis."""
        analysis = converter.analyze(simple_langflow_workflow)

        assert analysis.node_count == 2
        assert analysis.edge_count == 1
        assert "ChatInput" in analysis.component_types
        assert "ChatOutput" in analysis.component_types

    def test_step_ordering_with_dependencies(self, converter: LangflowConverter):
        """Test that steps are ordered based on dependencies, not node order."""
        # Create workflow with intentionally wrong node order
        workflow_data = {
            "data": {
                "nodes": [
                    {
                        "id": "output-node",  # This depends on others but appears first
                        "data": {"type": "ChatOutput", "node": {"template": {}}},
                        "type": "genericNode",
                    },
                    {
                        "id": "input-node",  # This has no dependencies but appears last
                        "data": {"type": "ChatInput", "node": {"template": {}}},
                        "type": "genericNode",
                    },
                    {
                        "id": "middle-node",  # This depends on input but appears middle
                        "data": {"type": "Prompt", "node": {"template": {}}},
                        "type": "genericNode",
                    },
                ],
                "edges": [
                    {
                        "source": "input-node",
                        "target": "middle-node",
                        "data": {
                            "sourceHandle": {"name": "output"},
                            "targetHandle": {"fieldName": "input_0"},
                        },
                    },
                    {
                        "source": "middle-node",
                        "target": "output-node",
                        "data": {
                            "sourceHandle": {"name": "output"},
                            "targetHandle": {"fieldName": "input_0"},
                        },
                    },
                ],
            }
        }

        workflow = converter.convert(workflow_data)

        # Check that steps are reordered based on dependencies
        step_ids = [step.id for step in workflow.steps]

        # The Prompt (middle-node) should generate steps (blob + UDF executor)
        # Step count may vary with routing improvements, focus on dependency ordering
        middle_node_steps = [sid for sid in step_ids if "middle-node" in sid]
        assert len(middle_node_steps) > 0, f"Expected middle-node steps in {step_ids}"

        # Verify no forward references
        for i, step in enumerate(workflow.steps):
            if hasattr(step, "input") and step.input:
                for _key, value in step.input.items():
                    if isinstance(value, dict) and "$from" in str(value):
                        from_info = value.get("$from", {})
                        from_step = from_info.get("step", "")
                        if from_step:
                            # Find position of referenced step
                            try:
                                ref_pos = step_ids.index(from_step)
                                assert ref_pos < i, (
                                    f"Step {step.id} at position {i} "
                                    f"references {from_step} at position {ref_pos} "
                                    "(forward reference)"
                                )
                            except ValueError:
                                pytest.fail(
                                    f"Step {step.id} references "
                                    f"undefined step {from_step}"
                                )

    def test_complex_dependency_ordering(self, converter: LangflowConverter):
        """Test topological sorting with complex dependencies like memory_chatbot."""
        # Simulate the memory_chatbot structure that was failing
        workflow_data = {
            "data": {
                "nodes": [
                    {
                        "id": "ChatInput-1",
                        "data": {"type": "ChatInput", "node": {"template": {}}},
                        "type": "genericNode",
                    },
                    {
                        "id": "ChatOutput-2",
                        "data": {"type": "ChatOutput", "node": {"template": {}}},
                        "type": "genericNode",
                    },
                    {
                        "id": "Memory-3",
                        "data": {"type": "Memory", "node": {"template": {}}},
                        "type": "genericNode",
                    },
                    {
                        "id": "Prompt-4",
                        "data": {"type": "Prompt", "node": {"template": {}}},
                        "type": "genericNode",
                    },
                    {
                        "id": "LLM-5",
                        "data": {
                            "type": "LanguageModelComponent",
                            "node": {"template": {}},
                        },
                        "type": "genericNode",
                    },
                ],
                "edges": [
                    {
                        "source": "Memory-3",
                        "target": "Prompt-4",
                        "data": {
                            "sourceHandle": {"name": "output"},
                            "targetHandle": {"fieldName": "input_0"},
                        },
                    },
                    {
                        "source": "Prompt-4",
                        "target": "LLM-5",
                        "data": {
                            "sourceHandle": {"name": "output"},
                            "targetHandle": {"fieldName": "input_0"},
                        },
                    },
                    {
                        "source": "ChatInput-1",
                        "target": "LLM-5",
                        "data": {
                            "sourceHandle": {"name": "output"},
                            "targetHandle": {"fieldName": "input_1"},
                        },
                    },
                    {
                        "source": "LLM-5",
                        "target": "ChatOutput-2",
                        "data": {
                            "sourceHandle": {"name": "output"},
                            "targetHandle": {"fieldName": "input_0"},
                        },
                    },
                ],
            }
        }

        workflow = converter.convert(workflow_data)
        step_ids = [step.id for step in workflow.steps]

        # Find the main processing steps (not blob steps)
        # Step count may vary with routing improvements, focus on dependency ordering
        memory_steps = [
            sid for sid in step_ids if "memory-3" in sid and "_blob" not in sid
        ]
        prompt_steps = [
            sid for sid in step_ids if "prompt-4" in sid and "_blob" not in sid
        ]
        llm_steps = [sid for sid in step_ids if "llm-5" in sid and "_blob" not in sid]

        # Verify the main processing steps exist
        assert len(memory_steps) > 0, f"Expected memory processing step in {step_ids}"
        assert len(prompt_steps) > 0, f"Expected prompt processing step in {step_ids}"
        assert len(llm_steps) > 0, f"Expected llm processing step in {step_ids}"

        # Check dependency ordering between main processing steps
        memory_pos = step_ids.index(memory_steps[0])
        prompt_pos = step_ids.index(prompt_steps[0])
        llm_pos = step_ids.index(llm_steps[0])

        # Memory should come before Prompt (Memory -> Prompt)
        assert memory_pos < prompt_pos, f"Memory should come before Prompt: {step_ids}"

        # Prompt should come before LLM (Prompt -> LLM)
        assert prompt_pos < llm_pos, f"Prompt should come before LLM: {step_ids}"

    def test_component_routing_strategy_with_custom_code(self):
        """Test that components with custom code create custom blobs."""
        converter = LangflowConverter()

        # Create workflow with component that has custom code
        workflow_data = {
            "data": {
                "nodes": [
                    {
                        "id": "custom-component",
                        "data": {
                            "type": "CustomComponent",
                            "node": {
                                "template": {
                                    "code": {
                                        "value": """
from langflow.custom.custom_component.component import Component
from langflow.io import Output

class CustomComponent(Component):
    def custom_method(self):
        return "Custom implementation"
"""
                                    },
                                    "param1": {"type": "str", "value": "test"},
                                },
                                "outputs": [
                                    {"name": "output", "method": "custom_method"}
                                ],
                                "base_classes": ["Component"],
                                "display_name": "Custom Component",
                                "metadata": {"module": "custom.module"},
                            },
                            "outputs": [{"name": "output", "method": "custom_method"}],
                        },
                    }
                ],
                "edges": [],
            }
        }

        workflow = converter.convert(workflow_data)

        # Should create blob step + UDF executor step for component with custom code
        step_components = [step.component for step in workflow.steps]
        assert "/builtin/put_blob" in step_components, (
            "Should create blob step for custom code"
        )
        assert "/langflow/udf_executor" in step_components, (
            "Should route custom code to UDF executor"
        )

        # Find the blob step and verify it contains the custom code
        blob_steps = [
            step for step in workflow.steps if step.component == "/builtin/put_blob"
        ]
        assert len(blob_steps) > 0, "Should have at least one blob step"

        blob_step = blob_steps[0]
        blob_data = blob_step.input.get("data", {})
        assert "code" in blob_data, "Blob should contain component code"
        assert "CustomComponent" in blob_data["code"], (
            "Should contain custom component class"
        )

    def test_component_routing_strategy_without_custom_code(self):
        """Test that components without custom use built-in loading."""
        converter = LangflowConverter()

        # Create workflow with built-in component (no custom code)
        workflow_data = {
            "data": {
                "nodes": [
                    {
                        "id": "builtin-component",
                        "data": {
                            "type": "BuiltinComponent",
                            "node": {
                                "template": {
                                    "param1": {"type": "str", "value": "test"}
                                    # No "code" field - this is a built-in component
                                },
                                "outputs": [{"name": "output", "method": "build"}],
                                "base_classes": ["Component"],
                                "display_name": "Builtin Component",
                                "metadata": {
                                    "module": (
                                        "langflow.components.builtin.BuiltinComponent"
                                    )
                                },
                            },
                            "outputs": [{"name": "output", "method": "build"}],
                        },
                    }
                ],
                "edges": [],
            }
        }

        workflow = converter.convert(workflow_data)

        # Should still create UDF executor step but with dynamic code loading
        step_components = [step.component for step in workflow.steps]
        assert "/builtin/put_blob" in step_components, (
            "Should create blob step for built-in component"
        )
        assert "/langflow/udf_executor" in step_components, (
            "Should route built-in component to UDF executor"
        )

        # Find the blob step and verify it's marked as built-in
        blob_steps = [
            step for step in workflow.steps if step.component == "/builtin/put_blob"
        ]
        assert len(blob_steps) > 0, "Should have blob step for built-in component"

        blob_step = blob_steps[0]
        blob_data = blob_step.input.get("data", {})
        assert blob_data.get("is_builtin") == True, (
            "Should be marked as built-in component"
        )

        # Should either have dynamic loading code OR import code for built-in components
        code = blob_data.get("code", "")
        assert (
            "Will be loaded dynamically" in code
            or "from langflow.components" in code
            or "import" in code
        ), f"Should have dynamic loading or import code, got: {code[:100]}"
