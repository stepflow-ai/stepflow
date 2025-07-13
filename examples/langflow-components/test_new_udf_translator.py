#!/usr/bin/env python3
"""
Test script for the new Langflow to StepFlow UDF translator.

This demonstrates the new parsing functions that:
1. Extract component schemas from Langflow templates
2. Generate proper UDF blob creation steps
3. Handle dependencies between components
"""

import json
import yaml
import sys
from pathlib import Path

# Add the path to import from the SDK
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "sdks" / "langflow" / "src"))

# Import the parsing functions directly instead of through the module
import importlib.util
spec = importlib.util.spec_from_file_location(
    "langflow_udf", 
    Path(__file__).parent.parent.parent / "sdks" / "langflow" / "src" / "stepflow_langflow" / "langflow_udf.py"
)
langflow_udf_module = importlib.util.module_from_spec(spec)

# We need to provide the missing dependencies
class MockStepflowContext:
    pass

class MockMessage:
    def __init__(self, text="", sender="user", sender_name="user"):
        self.text = text
        self.sender = sender
        self.sender_name = sender_name

class MockData:
    def __init__(self, data=None):
        self.data = data or {}

class MockDataFrame:
    def __init__(self, data=None):
        self.data = data or []

# Add mock dependencies to the module
langflow_udf_module.StepflowContext = MockStepflowContext
langflow_udf_module.Message = MockMessage  
langflow_udf_module.Data = MockData
langflow_udf_module.DataFrame = MockDataFrame

# Execute the module
spec.loader.exec_module(langflow_udf_module)

# Import the function we need
parse_langflow_to_stepflow_workflow = langflow_udf_module.parse_langflow_to_stepflow_workflow


def test_langflow_translation():
    """Test the new Langflow translation functionality."""
    
    # Load the example Langflow JSON
    langflow_file = Path(__file__).parent / "ExampleLangflow.json"
    
    if not langflow_file.exists():
        print(f"Error: {langflow_file} not found")
        return 1
    
    print(f"Loading Langflow JSON from: {langflow_file}")
    with open(langflow_file, 'r') as f:
        langflow_data = json.load(f)
    
    print(f"Loaded workflow: {langflow_data.get('name', 'Unnamed')}")
    print(f"Description: {langflow_data.get('description', 'No description')}")
    
    # Parse the Langflow JSON into StepFlow workflow steps
    print("\n=== Converting to StepFlow UDF steps ===")
    try:
        workflow_steps = parse_langflow_to_stepflow_workflow(langflow_data)
        
        print(f"Generated {len(workflow_steps)} UDF creation steps:")
        
        for i, step in enumerate(workflow_steps, 1):
            step_id = step.get('id', 'unknown')
            udf_data = step.get('input', {}).get('data', {})
            component_type = udf_data.get('component_type', 'unknown')
            
            print(f"\n{i}. Step ID: {step_id}")
            print(f"   Component Type: {component_type}")
            print(f"   Display Name: {udf_data.get('display_name', 'N/A')}")
            print(f"   Has Input Schema: {bool(udf_data.get('input_schema'))}")
            print(f"   Code Lines: {len(udf_data.get('code', '').splitlines())}")
            
            # Show a snippet of the generated code
            code = udf_data.get('code', '')
            if code:
                code_lines = code.strip().splitlines()
                if len(code_lines) > 3:
                    print(f"   Code snippet: {code_lines[1]} ... (+ {len(code_lines)-3} more lines)")
                else:
                    print(f"   Code snippet: {code_lines[1] if len(code_lines) > 1 else 'N/A'}")
            
            # Show dependencies if any
            if 'depends_on' in step:
                print(f"   Dependencies: {step['depends_on']}")
        
        # Create a complete StepFlow workflow
        complete_workflow = {
            'name': langflow_data.get('name', 'Translated Langflow Workflow'),
            'description': f"Converted from Langflow using UDF-based translation. Original: {langflow_data.get('description', '')}",
            'input_schema': {
                'type': 'object',
                'properties': {
                    'user_input': {
                        'type': 'string',
                        'description': 'User input message'
                    }
                },
                'required': ['user_input']
            },
            'steps': workflow_steps,
            'output': {
                # For now, output the last step's result
                'result': {
                    '$from': {'step': workflow_steps[-1]['id'] if workflow_steps else 'unknown'},
                    'path': 'blob_id'
                }
            }
        }
        
        # Save to a new YAML file
        output_file = Path(__file__).parent / "translated_workflow_new.yaml"
        with open(output_file, 'w') as f:
            yaml.dump(complete_workflow, f, default_flow_style=False, indent=2, sort_keys=False)
        
        print(f"\n✅ Translation completed successfully!")
        print(f"Full workflow saved to: {output_file}")
        
        # Show the complete workflow structure
        print("\n=== Complete StepFlow Workflow Structure ===")
        print(f"Name: {complete_workflow['name']}")
        print(f"Steps: {len(complete_workflow['steps'])}")
        print(f"Has input schema: {bool(complete_workflow.get('input_schema'))}")
        print(f"Has output mapping: {bool(complete_workflow.get('output'))}")
        
        return 0
        
    except Exception as e:
        print(f"❌ Translation failed: {e}")
        import traceback
        traceback.print_exc()
        return 1


def demonstrate_individual_components():
    """Demonstrate parsing of individual components."""
    
    print("\n=== Individual Component Analysis ===")
    
    # Load the example again
    langflow_file = Path(__file__).parent / "ExampleLangflow.json"
    with open(langflow_file, 'r') as f:
        langflow_data = json.load(f)
    
    nodes = langflow_data.get('data', {}).get('nodes', [])
    
    for node in nodes:
        node_data = node.get('data', {})
        node_type = node_data.get('type')
        node_id = node_data.get('id')
        
        # Skip note nodes
        if node_type in ['note', 'noteNode'] or not node_id:
            continue
        
        print(f"\n--- Component: {node_type} (ID: {node_id}) ---")
        
        node_info = node_data.get('node', {})
        template = node_info.get('template', {})
        
        print(f"Display Name: {node_info.get('display_name', 'N/A')}")
        print(f"Description: {node_info.get('description', 'N/A')}")
        print(f"Template fields: {len(template)}")
        
        # Show some template field details
        interesting_fields = ['input_value', 'template', 'model_name', 'provider']
        for field in interesting_fields:
            if field in template:
                field_config = template[field]
                if isinstance(field_config, dict):
                    value = field_config.get('value', 'N/A')
                    field_type = field_config.get('type', 'unknown')
                    print(f"  {field}: {value} (type: {field_type})")


if __name__ == "__main__":
    print("🔄 Testing New Langflow to StepFlow UDF Translator")
    print("=" * 50)
    
    result = test_langflow_translation()
    
    if result == 0:
        demonstrate_individual_components()
    
    print("\n" + "=" * 50)
    print("Test completed!")
    
    sys.exit(result)