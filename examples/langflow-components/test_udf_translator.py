#!/usr/bin/env python3
"""
Test script for the UDF-based Langflow to StepFlow translator.
"""

import json
import yaml
from langflow_component_server_udf_only import LangflowUDFTranslator

def test_udf_translator():
    """Test the UDF translator with the example Langflow JSON."""
    
    # Load the example Langflow JSON
    with open('../custom-python-components/ExampleLangflow.json', 'r') as f:
        langflow_data = json.load(f)
    
    # Create the translator
    translator = LangflowUDFTranslator()
    
    # Translate to StepFlow YAML
    try:
        stepflow_data = translator.translate(langflow_data)
        
        # Print the result
        print("=== Translation Successful ===")
        print(yaml.dump(stepflow_data, default_flow_style=False, indent=2))
        
        # Save to file
        with open('translated_udf_workflow.yaml', 'w') as f:
            yaml.dump(stepflow_data, f, default_flow_style=False, indent=2)
        
        print("\n=== Saved to translated_udf_workflow.yaml ===")
        
        # Print some statistics
        print(f"\nTranslation Statistics:")
        print(f"- Original nodes: {len(langflow_data.get('data', {}).get('nodes', []))}")
        print(f"- Translated steps: {len(stepflow_data.get('steps', []))}")
        print(f"- Has input schema: {stepflow_data.get('input_schema') is not None}")
        print(f"- Has output mapping: {stepflow_data.get('output') is not None}")
        
        return True
        
    except Exception as e:
        print(f"=== Translation Failed ===")
        print(f"Error: {e}")
        import traceback
        traceback.print_exc()
        return False

def analyze_langflow_structure():
    """Analyze the Langflow JSON structure to understand component types."""
    
    with open('../custom-python-components/ExampleLangflow.json', 'r') as f:
        langflow_data = json.load(f)
    
    nodes = langflow_data.get('data', {}).get('nodes', [])
    edges = langflow_data.get('data', {}).get('edges', [])
    
    print("=== Langflow Structure Analysis ===")
    print(f"Total nodes: {len(nodes)}")
    print(f"Total edges: {len(edges)}")
    
    print("\nNode Types:")
    for node in nodes:
        node_type = node.get('data', {}).get('type', 'Unknown')
        node_id = node.get('id', 'Unknown')
        
        # Check if it has code
        template = node.get('data', {}).get('node', {}).get('template', {})
        has_code = 'code' in template and template.get('code', {}).get('value', '') != ''
        
        # Check output types
        outputs = node.get('data', {}).get('node', {}).get('outputs', [])
        output_types = outputs[0].get('types', []) if outputs else []
        
        print(f"  - {node_type} ({node_id})")
        print(f"    Has code: {has_code}")
        print(f"    Output types: {output_types}")
        
        if has_code:
            code_preview = template.get('code', {}).get('value', '')[:100] + '...'
            print(f"    Code preview: {code_preview}")
        print()

if __name__ == "__main__":
    print("Testing UDF-based Langflow to StepFlow Translator")
    print("=" * 50)
    
    # First analyze the structure
    analyze_langflow_structure()
    
    # Then test the translation
    success = test_udf_translator()
    
    if success:
        print("\n✅ Translation test passed!")
    else:
        print("\n❌ Translation test failed!") 