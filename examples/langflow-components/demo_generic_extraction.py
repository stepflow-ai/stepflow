#!/usr/bin/env python3
"""
Demo showing the generic component input extraction functionality.
"""

from langflow_to_stepflow_native import NativeLangflowTranslator

# Sample Langflow node structures for different component types
sample_nodes = {
    "openai_node": {
        "id": "openai-abc123",
        "data": {
            "type": "LanguageModelComponent",
            "node": {
                "template": {
                    "model_name": {"value": "gpt-4o"},
                    "temperature": {"value": "0.7"},
                    "max_tokens": {"value": "1000"},
                    "api_key": {"value": "OPENAI_API_KEY"},  # Placeholder - will be skipped
                    "provider": {"value": "OpenAI"}
                }
            }
        }
    },
    
    "prompt_node": {
        "id": "prompt-xyz789",
        "data": {
            "type": "Prompt",
            "node": {
                "template": {
                    "template": {"value": "You are a {role} assistant. Help with {task}."},
                    "variable_1": {"value": "expert"},
                    "variable_2": {"value": ""}  # Empty - will be skipped
                }
            }
        }
    },
    
    "custom_node": {
        "id": "custom-def456", 
        "data": {
            "type": "TextSplitter",
            "node": {
                "template": {
                    "chunk_size": {"value": "500"},
                    "chunk_overlap": {"value": "50"},
                    "separator": {"value": "\\n\\n"},
                    "keep_separator": {"value": True}
                }
            }
        }
    }
}

def demo_generic_extraction():
    """Demonstrate the generic input extraction."""
    translator = NativeLangflowTranslator()
    
    print("=== Generic Component Input Extraction Demo ===\n")
    
    for node_name, node_data in sample_nodes.items():
        print(f"Node: {node_name}")
        print(f"Component Type: {node_data['data']['type']}")
        
        # Extract inputs using the generic method
        inputs = translator._extract_component_inputs(node_data)
        
        print("Extracted inputs:")
        for key, value in inputs.items():
            print(f"  {key}: {value} ({type(value).__name__})")
        print()

def demo_component_translation():
    """Demonstrate full component translation."""
    translator = NativeLangflowTranslator()
    
    print("=== Generic Component Translation Demo ===\n")
    
    # Translate the custom TextSplitter component (not in known mappings)
    custom_node = sample_nodes["custom_node"]
    dependencies = {}  # No dependencies for this demo
    
    step = translator._translate_generic_component(
        custom_node, 
        dependencies, 
        "text_splitter"
    )
    
    print("Translated TextSplitter component:")
    import json
    print(json.dumps(step, indent=2))

if __name__ == "__main__":
    demo_generic_extraction()
    demo_component_translation()