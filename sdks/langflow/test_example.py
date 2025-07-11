#!/usr/bin/env python3
"""Test the example langflow component server."""

import sys
import os
sys.path.insert(0, '/Users/jordan.frazier/Documents/StepFlow/stepflow/examples/langflow-components')

from langflow_component_server import server

def test_example_server():
    """Test the example langflow component server."""
    print("Testing example langflow component server...")
    
    print(f"Server has {len(server._components)} components registered")
    
    # List components
    for name, component in server._components.items():
        print(f"  - {name}: {component.description or 'No description'}")
    
    # Test specific components
    components_to_test = [
        "langflow://udf",
        "langflow://langflow_udf", 
        "langflow://text_processor",
        "langflow://data_transformer",
        "langflow://data_storage"
    ]
    
    for comp_name in components_to_test:
        comp = server.get_component(comp_name)
        if comp:
            print(f"✓ {comp_name} is registered")
        else:
            print(f"✗ {comp_name} not found")
    
    print("Test completed!")

if __name__ == "__main__":
    test_example_server()