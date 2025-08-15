#!/usr/bin/env python3
"""Basic conversion example for Langflow to Stepflow integration."""

import json
from pathlib import Path

from stepflow_langflow_integration import LangflowConverter


def main():
    """Demonstrate basic workflow conversion."""
    
    # Sample Langflow workflow
    sample_workflow = {
        "data": {
            "nodes": [
                {
                    "id": "ChatInput-abc123",
                    "data": {
                        "type": "ChatInput",
                        "node": {
                            "template": {
                                "input_value": {
                                    "type": "str", 
                                    "value": "",
                                    "info": "Message to be passed as input"
                                },
                                "sender": {
                                    "type": "dropdown",
                                    "options": ["User", "AI"],
                                    "value": "User",
                                    "info": "Type of sender"
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
                    "id": "OpenAIModel-def456", 
                    "data": {
                        "type": "LanguageModelComponent",
                        "node": {
                            "template": {
                                "api_key": {
                                    "type": "str",
                                    "value": "${OPENAI_API_KEY}",
                                    "_input_type": "SecretStrInput",
                                    "info": "OpenAI API Key"
                                },
                                "model_name": {
                                    "type": "dropdown",
                                    "options": ["gpt-3.5-turbo", "gpt-4"],
                                    "value": "gpt-3.5-turbo",
                                    "info": "Model to use"
                                },
                                "temperature": {
                                    "type": "float",
                                    "value": 0.7,
                                    "info": "Temperature setting"
                                }
                            }
                        },
                        "outputs": [
                            {
                                "name": "response",
                                "method": "generate_response", 
                                "types": ["Message"]
                            }
                        ]
                    }
                }
            ],
            "edges": [
                {
                    "source": "ChatInput-abc123",
                    "target": "OpenAIModel-def456",
                    "source_handle": "message",
                    "target_handle": "input_value"
                }
            ]
        }
    }
    
    print("🔄 Converting Langflow workflow to Stepflow...")
    
    # Create converter
    converter = LangflowConverter(validate_schemas=True)
    
    # Analyze the workflow first
    print("\n📊 Analyzing workflow:")
    analysis = converter.analyze(sample_workflow)
    print(f"  • Nodes: {analysis['node_count']}")
    print(f"  • Edges: {analysis['edge_count']}")
    print(f"  • Component Types: {list(analysis['component_types'].keys())}")
    
    # Convert to Stepflow
    workflow = converter.convert(sample_workflow)
    print(f"\n✅ Converted to Stepflow workflow: '{workflow.name}'")
    print(f"  • Steps: {len(workflow.steps)}")
    
    # Generate YAML
    yaml_output = converter.to_yaml(workflow)
    
    # Save to file
    output_path = Path(__file__).parent / "converted_workflow.yaml"
    with open(output_path, "w") as f:
        f.write(yaml_output)
    
    print(f"💾 Saved to: {output_path}")
    
    # Show preview
    print(f"\n📝 YAML Preview:")
    print("─" * 50)
    print(yaml_output[:500] + "..." if len(yaml_output) > 500 else yaml_output)
    print("─" * 50)
    
    print(f"\n🎉 Conversion complete! Use with Stepflow:")
    print(f"   cargo run -- run --flow {output_path} --input input.json --config stepflow-config.yml")


if __name__ == "__main__":
    main()