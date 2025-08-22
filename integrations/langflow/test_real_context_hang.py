#!/usr/bin/env python3
"""
Test to isolate the real StepflowContext hanging issue.
This test executes the exact simple_agent workflow with minimal debug output
to capture the hanging behavior in real Stepflow runtime context.
"""

import asyncio
import json
import sys
import tempfile
from pathlib import Path
sys.path.append('src')

from stepflow_langflow_integration.testing.stepflow_binary import StepflowBinaryRunner, get_default_stepflow_config, create_test_config_file
from stepflow_langflow_integration.converter.translator import LangflowConverter

async def test_real_stepflow_context_hang():
    """Test the simple_agent workflow to reproduce the exact hanging behavior."""
    print("🔍 TESTING REAL STEPFLOW CONTEXT HANG")
    print("=" * 60)
    
    # Load and convert simple_agent workflow
    with open('tests/fixtures/langflow/simple_agent.json', 'r') as f:
        data = json.load(f)
    
    converter = LangflowConverter()
    workflow = converter.convert(data)
    workflow_yaml = converter.to_yaml(workflow)
    
    # Create test config
    config_content = get_default_stepflow_config()
    config_path = create_test_config_file(config_content)
    
    print("Setting up test environment...")
    
    # Create workflow file
    with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
        f.write(workflow_yaml)
        workflow_path = f.name
    
    print(f"Created workflow file: {workflow_path}")
    print(f"Created config file: {config_path}")
    
    # Run workflow with timeout to catch hanging
    runner = StepflowBinaryRunner()
    input_data = {'message': 'What is 2 + 2?'}
    
    try:
        print("Starting workflow execution with 10 second timeout...")
        success, result_data, stdout, stderr = runner.run_workflow(
            workflow_yaml, input_data, config_path=config_path, timeout=10.0
        )
        
        print("✅ Workflow completed successfully!")
        print(f"Success: {success}")
        print(f"Result: {json.dumps(result_data, indent=2) if result_data else None}")
        
    except Exception as e:
        print(f"❌ Workflow failed with exception: {e}")
        print("This indicates the hanging behavior was reproduced")
    
    finally:
        # Cleanup
        Path(workflow_path).unlink(missing_ok=True)
        Path(config_path).unlink(missing_ok=True)
        
        # Check debug log for hanging point
        if Path('/tmp/udf_debug.log').exists():
            with open('/tmp/udf_debug.log', 'r') as f:
                debug_content = f.read()
                lines = debug_content.strip().split('\n')
                if lines:
                    last_line = lines[-1]
                    print(f"\nLast debug log entry: {last_line}")
                    
                    # Check if it stopped at context.get_blob()
                    if "Starting context.get_blob() with 5-second timeout" in last_line:
                        print("🎯 CONFIRMED: Hanging occurs exactly at context.get_blob()")
                        print("This is a Stepflow runtime communication issue, not Langflow integration")
                    else:
                        print("🤔 Hanging point may be different than expected")

if __name__ == "__main__":
    asyncio.run(test_real_stepflow_context_hang())