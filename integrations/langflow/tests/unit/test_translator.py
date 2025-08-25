# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
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
        assert not converter.validate_schemas
    
    def test_init_with_validation(self):
        """Test initialization with validation enabled."""
        converter = LangflowConverter(validate_schemas=True)
        assert converter.validate_schemas
    
    def test_convert_simple_workflow(
        self, 
        converter: LangflowConverter, 
        simple_langflow_workflow: Dict[str, Any]
    ):
        """Test conversion of simple workflow."""
        workflow = converter.convert(simple_langflow_workflow)
        
        assert workflow.name == "Converted Langflow Workflow"
        assert len(workflow.steps) == 2
        
        # Check step IDs (now use full node ID with suffix for uniqueness)
        step_ids = [step.id for step in workflow.steps]
        assert "langflow_chatinput-1" in step_ids
        assert "langflow_chatoutput-2" in step_ids
        
        # Check components - they should be mapped to specific Langflow components  
        for step in workflow.steps:
            assert step.component.startswith("/langflow/")
            # Built-in Langflow components don't need blob_id, custom UDF components do
            if step.component == "/langflow/udf_executor":
                assert "blob_id" in step.input
    
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
        self, 
        converter: LangflowConverter, 
        simple_langflow_workflow: Dict[str, Any]
    ):
        """Test YAML generation."""
        workflow = converter.convert(simple_langflow_workflow)
        yaml_output = converter.to_yaml(workflow)
        
        assert isinstance(yaml_output, str)
        assert "name: Converted Langflow Workflow" in yaml_output
        assert "steps:" in yaml_output
        assert "/langflow/" in yaml_output
    
    def test_analyze_workflow(
        self, 
        converter: LangflowConverter, 
        simple_langflow_workflow: Dict[str, Any]
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
                        "data": {
                            "type": "ChatOutput",
                            "node": {"template": {}}
                        },
                        "type": "genericNode"
                    },
                    {
                        "id": "input-node",   # This has no dependencies but appears last
                        "data": {
                            "type": "ChatInput", 
                            "node": {"template": {}}
                        },
                        "type": "genericNode"
                    },
                    {
                        "id": "middle-node",  # This depends on input but appears middle
                        "data": {
                            "type": "Prompt",
                            "node": {"template": {}}
                        },
                        "type": "genericNode"
                    }
                ],
                "edges": [
                    {
                        "source": "input-node",
                        "target": "middle-node", 
                        "data": {
                            "sourceHandle": {"name": "output"},
                            "targetHandle": {"fieldName": "input_0"}
                        }
                    },
                    {
                        "source": "middle-node",
                        "target": "output-node",
                        "data": {
                            "sourceHandle": {"name": "output"}, 
                            "targetHandle": {"fieldName": "input_0"}
                        }
                    }
                ]
            }
        }
        
        workflow = converter.convert(workflow_data)
        
        # Check that steps are reordered based on dependencies
        step_ids = [step.id for step in workflow.steps]
        
        # input-node should come first (no dependencies)
        # middle-node should come second (depends on input-node)
        # output-node should come last (depends on middle-node)
        input_pos = step_ids.index("langflow_input-node")
        middle_pos = step_ids.index("langflow_middle-node") 
        output_pos = step_ids.index("langflow_output-node")
        
        assert input_pos < middle_pos, f"Input step should come before middle step: {step_ids}"
        assert middle_pos < output_pos, f"Middle step should come before output step: {step_ids}"
        
        # Verify no forward references
        for i, step in enumerate(workflow.steps):
            if hasattr(step, 'input') and step.input:
                for key, value in step.input.items():
                    if isinstance(value, dict) and '$from' in str(value):
                        from_info = value.get('$from', {})
                        referenced_step = from_info.get('step', '')
                        if referenced_step:
                            # Find position of referenced step
                            try:
                                ref_pos = step_ids.index(referenced_step)
                                assert ref_pos < i, f"Step {step.id} at position {i} references {referenced_step} at position {ref_pos} (forward reference)"
                            except ValueError:
                                pytest.fail(f"Step {step.id} references undefined step {referenced_step}")
    
    def test_complex_dependency_ordering(self, converter: LangflowConverter):
        """Test topological sorting with complex dependencies like memory_chatbot."""
        # Simulate the memory_chatbot structure that was failing
        workflow_data = {
            "data": {
                "nodes": [
                    {"id": "ChatInput-1", "data": {"type": "ChatInput", "node": {"template": {}}}, "type": "genericNode"},
                    {"id": "ChatOutput-2", "data": {"type": "ChatOutput", "node": {"template": {}}}, "type": "genericNode"}, 
                    {"id": "Memory-3", "data": {"type": "Memory", "node": {"template": {}}}, "type": "genericNode"},
                    {"id": "Prompt-4", "data": {"type": "Prompt", "node": {"template": {}}}, "type": "genericNode"},
                    {"id": "LLM-5", "data": {"type": "LanguageModelComponent", "node": {"template": {}}}, "type": "genericNode"}
                ],
                "edges": [
                    {"source": "Memory-3", "target": "Prompt-4", "data": {"sourceHandle": {"name": "output"}, "targetHandle": {"fieldName": "input_0"}}},
                    {"source": "Prompt-4", "target": "LLM-5", "data": {"sourceHandle": {"name": "output"}, "targetHandle": {"fieldName": "input_0"}}},
                    {"source": "ChatInput-1", "target": "LLM-5", "data": {"sourceHandle": {"name": "output"}, "targetHandle": {"fieldName": "input_1"}}},
                    {"source": "LLM-5", "target": "ChatOutput-2", "data": {"sourceHandle": {"name": "output"}, "targetHandle": {"fieldName": "input_0"}}}
                ]
            }
        }
        
        workflow = converter.convert(workflow_data)
        step_ids = [step.id for step in workflow.steps]
        
        # Expected order: Memory-3, ChatInput-1, Prompt-4, LLM-5, ChatOutput-2
        memory_pos = step_ids.index("langflow_memory-3")
        input_pos = step_ids.index("langflow_chatinput-1") 
        prompt_pos = step_ids.index("langflow_prompt-4")
        llm_pos = step_ids.index("langflow_llm-5")
        output_pos = step_ids.index("langflow_chatoutput-2")
        
        # Memory should come before Prompt (Memory -> Prompt)
        assert memory_pos < prompt_pos, f"Memory should come before Prompt: {step_ids}"
        
        # Prompt should come before LLM (Prompt -> LLM) 
        assert prompt_pos < llm_pos, f"Prompt should come before LLM: {step_ids}"
        
        # ChatInput should come before LLM (ChatInput -> LLM)
        assert input_pos < llm_pos, f"ChatInput should come before LLM: {step_ids}"
        
        # LLM should come before ChatOutput (LLM -> ChatOutput)
        assert llm_pos < output_pos, f"LLM should come before ChatOutput: {step_ids}"