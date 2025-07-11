#!/usr/bin/env python3
"""Quick test of langflow SDK components."""

from stepflow_langflow import StepflowLangflowServer, langflow_udf
from stepflow_sdk import StepflowContext
import asyncio

def test_langflow_server():
    """Test that langflow server can be created and components registered."""
    print("Testing langflow server creation...")
    
    # Create server
    server = StepflowLangflowServer(default_protocol_prefix="langflow")
    
    # Check if langflow_udf is available
    print(f"Server has {len(server._components)} components registered")
    
    # List components
    for name, component in server._components.items():
        print(f"  - {name}: {component.description or 'No description'}")
    
    # Test that langflow_udf is accessible
    try:
        langflow_udf_component = server.get_component("langflow://langflow_udf")
        if langflow_udf_component:
            print("✓ langflow_udf component is properly registered")
        else:
            print("✗ langflow_udf component not found")
    except Exception as e:
        print(f"✗ Error accessing langflow_udf: {e}")
    
    print("Test completed!")

if __name__ == "__main__":
    test_langflow_server()