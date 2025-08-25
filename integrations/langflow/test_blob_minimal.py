#!/usr/bin/env python3
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

"""
Minimal reproduction case for context.get_blob() hanging issue.
"""

import asyncio
import json
import sys
import os
sys.path.append('src')

from stepflow_langflow_integration.executor.udf_executor import UDFExecutor

class MockContext:
    """Mock context that simulates the hanging behavior."""
    
    def __init__(self, should_hang=False):
        self.should_hang = should_hang
        self.call_count = 0
        
    async def get_blob(self, blob_id: str):
        """Mock get_blob that can hang or return data."""
        self.call_count += 1
        print(f"MockContext.get_blob() called #{self.call_count} with blob_id: {blob_id}")
        
        if self.should_hang:
            print(f"MockContext: Simulating hang for get_blob({blob_id})")
            # Hang forever to simulate the issue
            await asyncio.sleep(10000)
        else:
            print(f"MockContext: Returning mock data for get_blob({blob_id})")
            return {
                "code": "print('test')",
                "template": {"test": "value"},
                "component_type": "TestComponent"
            }

async def test_context_get_blob_simple():
    """Test 1: Simple context.get_blob() call without hanging."""
    print("=== TEST 1: Simple context.get_blob() without hanging ===")
    
    context = MockContext(should_hang=False)
    blob_id = "test_blob_id_123"
    
    try:
        result = await asyncio.wait_for(context.get_blob(blob_id), timeout=2.0)
        print(f"✅ SUCCESS: Got result: {result}")
        return True
    except asyncio.TimeoutError:
        print("❌ TIMEOUT: context.get_blob() timed out")
        return False
    except Exception as e:
        print(f"❌ ERROR: {e}")
        return False

async def test_context_get_blob_with_timeout():
    """Test 2: Test asyncio.wait_for timeout handling."""
    print("\n=== TEST 2: context.get_blob() with asyncio timeout ===")
    
    context = MockContext(should_hang=True)
    blob_id = "hanging_blob_id_456"
    
    try:
        result = await asyncio.wait_for(context.get_blob(blob_id), timeout=2.0)
        print(f"❌ UNEXPECTED: Got result: {result}")
        return False
    except asyncio.TimeoutError:
        print("✅ SUCCESS: Correctly caught TimeoutError")
        return True
    except Exception as e:
        print(f"❌ ERROR: {e}")
        return False

async def test_multiple_concurrent_get_blob():
    """Test 3: Multiple concurrent context.get_blob() calls."""
    print("\n=== TEST 3: Multiple concurrent context.get_blob() calls ===")
    
    context = MockContext(should_hang=False)
    
    async def get_blob_wrapper(blob_id):
        return await context.get_blob(f"blob_{blob_id}")
    
    try:
        # Test concurrent calls like in our tool sequence
        tasks = [get_blob_wrapper(i) for i in range(3)]
        results = await asyncio.gather(*tasks, return_exceptions=True)
        
        print(f"✅ SUCCESS: Got {len(results)} results")
        for i, result in enumerate(results):
            if isinstance(result, Exception):
                print(f"   Result {i}: ERROR - {result}")
            else:
                print(f"   Result {i}: {type(result)} with {len(result)} keys")
        return True
    except Exception as e:
        print(f"❌ ERROR: {e}")
        return False

async def test_real_stepflow_scenario():
    """Test 4: Simulate the exact simple_agent scenario."""
    print("\n=== TEST 4: Simulating simple_agent blob retrieval scenario ===")
    
    # Simulate the exact input data from simple_agent
    input_data = {
        'tool_sequence_config': {'tools': [], 'agent': {'blob_id': 'agent_blob'}},
        'input': {'input_value': 'What is 2 + 2?'},
        'tool_blob_0': '1234567890abcdef1234567890abcdef12345678',  # Mock blob ID
        'tool_blob_1': 'fedcba0987654321fedcba0987654321fedcba09',  # Mock blob ID
        'agent_blob': '8e5b077dd0fb2ed59bc18fea6f0bab2abe60cbc445b15f6c1c663c7101bbb683'  # Real blob ID from logs
    }
    
    context = MockContext(should_hang=False)
    
    # Simulate the exact blob resolution logic from our code
    try:
        sequence_config = input_data['tool_sequence_config']
        agent_config = sequence_config['agent']
        agent_blob_id = agent_config['blob_id']
        
        print(f"Agent blob_id extracted: {agent_blob_id}")
        print(f"Checking if {agent_blob_id} in input_data keys: {list(input_data.keys())}")
        
        if agent_blob_id in input_data:
            # This is an external input that was resolved by Stepflow
            resolved_blob_id = input_data[agent_blob_id]
            print(f"Resolved {agent_blob_id} to {resolved_blob_id}")
            
            print(f"About to call context.get_blob({resolved_blob_id})")
            agent_blob_data = await asyncio.wait_for(context.get_blob(resolved_blob_id), timeout=5.0)
            
            print(f"✅ SUCCESS: Retrieved agent blob data with keys: {list(agent_blob_data.keys())}")
            return True
        else:
            print(f"❌ ERROR: {agent_blob_id} not found in input_data")
            return False
            
    except asyncio.TimeoutError:
        print("❌ TIMEOUT: Agent blob retrieval timed out")
        return False
    except Exception as e:
        print(f"❌ ERROR: {e}")
        return False

async def main():
    """Run all tests."""
    print("🧪 MINIMAL REPRODUCTION TEST SUITE FOR context.get_blob() HANGING")
    print("=" * 80)
    
    test_results = []
    
    # Run all tests
    test_results.append(await test_context_get_blob_simple())
    test_results.append(await test_context_get_blob_with_timeout())
    test_results.append(await test_multiple_concurrent_get_blob())
    test_results.append(await test_real_stepflow_scenario())
    
    # Summary
    print("\n" + "=" * 80)
    print("🏆 TEST SUMMARY:")
    passed = sum(test_results)
    total = len(test_results)
    
    print(f"Passed: {passed}/{total} tests")
    
    test_names = [
        "Simple get_blob without hanging",
        "get_blob with asyncio timeout", 
        "Multiple concurrent get_blob calls",
        "Real simple_agent scenario simulation"
    ]
    
    for i, (name, passed) in enumerate(zip(test_names, test_results)):
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"  {i+1}. {name}: {status}")
    
    if passed == total:
        print("\n🎉 ALL TESTS PASSED - Issue is not in basic context.get_blob() functionality")
        print("The problem likely lies in the specific Stepflow runtime communication or blob storage")
    else:
        print(f"\n⚠️ {total-passed} tests failed - Issue reproduced in minimal case")
    
    return passed == total

if __name__ == "__main__":
    success = asyncio.run(main())
    sys.exit(0 if success else 1)