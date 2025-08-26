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

import pytest
from typing import Dict, Any

from stepflow_langflow_integration.converter.translator import LangflowConverter
from stepflow_langflow_integration.utils.errors import ConversionError


class TestLangflowConverter:
    """Test LangflowConverter functionality."""

    def test_init_default(self):
        """Test default initialization."""
        converter = LangflowConverter()

    def test_convert_simple_workflow(
        self, converter: LangflowConverter, simple_langflow_workflow: Dict[str, Any]
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
        self, converter: LangflowConverter, simple_langflow_workflow: Dict[str, Any]
    ):
        """Test YAML generation."""
        workflow = converter.convert(simple_langflow_workflow)
        yaml_output = converter.to_yaml(workflow)

        assert isinstance(yaml_output, str)
        assert "name: Converted Langflow Workflow" in yaml_output
        # Should have output section since ChatInput/ChatOutput create passthrough
        assert "output:" in yaml_output

    def test_analyze_workflow(
        self, converter: LangflowConverter, simple_langflow_workflow: Dict[str, Any]
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

        # Only the Prompt (middle-node) should generate a step since it's actual processing
        # ChatInput and ChatOutput are logical I/O connection points
        assert len(step_ids) == 1
        assert "langflow_middle-node" in step_ids

        # Verify no forward references
        for i, step in enumerate(workflow.steps):
            if hasattr(step, "input") and step.input:
                for key, value in step.input.items():
                    if isinstance(value, dict) and "$from" in str(value):
                        from_info = value.get("$from", {})
                        referenced_step = from_info.get("step", "")
                        if referenced_step:
                            # Find position of referenced step
                            try:
                                ref_pos = step_ids.index(referenced_step)
                                assert (
                                    ref_pos < i
                                ), f"Step {step.id} at position {i} references {referenced_step} at position {ref_pos} (forward reference)"
                            except ValueError:
                                pytest.fail(
                                    f"Step {step.id} references undefined step {referenced_step}"
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

        # Expected processing steps: Memory-3, Prompt-4, LLM-5
        # ChatInput and ChatOutput are logical I/O connection points
        assert len(step_ids) == 3

        memory_pos = step_ids.index("langflow_memory-3")
        prompt_pos = step_ids.index("langflow_prompt-4")
        llm_pos = step_ids.index("langflow_llm-5")

        # Memory should come before Prompt (Memory -> Prompt)
        assert memory_pos < prompt_pos, f"Memory should come before Prompt: {step_ids}"

        # Prompt should come before LLM (Prompt -> LLM)
        assert prompt_pos < llm_pos, f"Prompt should come before LLM: {step_ids}"
