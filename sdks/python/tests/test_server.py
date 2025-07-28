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

"""Tests for the core StepflowServer component management and protocol logic.

This test file focuses on testing the core server functionality:
- Component registration and management
- requires_context() logic
- Protocol method handling (handle_message)
- Component execution with/without context
"""

import msgspec
import pytest

from stepflow_py import StepflowContext
from stepflow_py.generated_protocol import (
    ComponentExecuteParams,
    ComponentInfoParams,
    ComponentListParams,
    InitializeParams,
    Method,
    MethodRequest,
)
from stepflow_py.server import ComponentEntry, StepflowServer


# Test message classes
class ValidInput(msgspec.Struct):
    name: str
    age: int


class ValidOutput(msgspec.Struct):
    greeting: str
    age_next_year: int


class InvalidInput:
    pass


class InvalidOutput:
    pass


@pytest.fixture
def server():
    return StepflowServer()


def test_component_registration(server):
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    assert "/test_component" in server.get_components()
    component = server.get_component("/test_component")
    assert isinstance(component, ComponentEntry)
    assert component.name == "/test_component"
    assert component.input_type == ValidInput
    assert component.output_type == ValidOutput


def test_component_with_custom_name(server):
    @server.component(name="custom_name")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    assert "/custom_name" in server.get_components()
    component = server.get_component("/custom_name")
    assert isinstance(component, ComponentEntry)
    assert component.name == "/custom_name"
    assert component.input_type == ValidInput
    assert component.output_type == ValidOutput


def test_component_execution(server):
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    component = server.get_component("/test_component")
    assert component is not None
    result = component.function(ValidInput(name="Alice", age=25))
    assert isinstance(result, ValidOutput)
    assert result.greeting == "Hello Alice!"
    assert result.age_next_year == 26


def test_list_components(server):
    @server.component(name="component1")
    def test_component1(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    @server.component(name="component2")
    def test_component2(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    components = server.get_components()
    assert len(components) == 2
    assert "/component1" in components
    assert "/component2" in components

    for name, component in components.items():
        assert isinstance(component, ComponentEntry)
        assert component.name == name
        assert component.input_type == ValidInput
        assert component.output_type == ValidOutput


def test_initialization_state(server):
    """Test server initialization state management."""
    # Initially not initialized
    assert not server.is_initialized()

    # Set initialized state
    server.set_initialized(True)
    assert server.is_initialized()

    # Reset initialization state
    server.set_initialized(False)
    assert not server.is_initialized()


@pytest.mark.asyncio
async def test_handle_component_info(server):
    """Test component info retrieval via handle_message."""

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    server.set_initialized(True)

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-1",
        method=Method.components_info,
        params=ComponentInfoParams(component="/test_component"),
    )
    response = await server.handle_message(request)
    assert response.id == request.id
    assert hasattr(response, "result")
    assert response.result.info.input_schema == msgspec.json.schema(ValidInput)
    assert response.result.info.output_schema == msgspec.json.schema(ValidOutput)


@pytest.mark.asyncio
async def test_handle_component_info_not_found(server):
    """Test component info for non-existent component."""
    server.set_initialized(True)

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-1",
        method=Method.components_info,
        params=ComponentInfoParams(component="/non_existent"),
    )
    response = await server.handle_message(request)
    assert hasattr(response, "error")
    assert "not found" in response.error.message


@pytest.mark.asyncio
async def test_handle_component_execute(server):
    """Test component execution via handle_message."""
    server.set_initialized(True)

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-1",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="/test_component", input={"name": "Alice", "age": 25}
        ),
    )
    response = await server.handle_message(request)
    assert response.id == request.id
    assert hasattr(response, "result")
    output = response.result.output
    assert output.greeting == "Hello Alice!"
    assert output.age_next_year == 26


@pytest.mark.asyncio
async def test_handle_component_execute_invalid_input(server):
    """Test component execution with invalid input."""
    server.set_initialized(True)

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-1",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="/test_component", input={"invalid": "input"}
        ),
    )

    response = await server.handle_message(request)
    assert hasattr(response, "error")
    assert "execution failed" in response.error.message.lower()


@pytest.mark.asyncio
async def test_handle_list_components(server):
    """Test component listing via handle_message."""
    server.set_initialized(True)

    @server.component(name="component1")
    def test_component1(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    @server.component(name="component2")
    def test_component2(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-1",
        method=Method.components_list,
        params=ComponentListParams(),
    )
    response = await server.handle_message(request)
    assert response.id == request.id
    assert hasattr(response, "result")
    assert len(response.result.components) == 2
    component_urls = [comp.component for comp in response.result.components]
    assert "/component1" in component_urls
    assert "/component2" in component_urls


@pytest.mark.asyncio
async def test_handle_unknown_method(server):
    """Test handling of unknown methods."""
    server.set_initialized(True)

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-1",
        method="unknown_method",  # type: ignore[arg-type]  # intentionally invalid
        params=ComponentListParams(),
    )

    response = await server.handle_message(request)
    assert hasattr(response, "error")
    assert response.error.code == -32601  # Method not found
    assert "Method not found" in response.error.message


@pytest.mark.asyncio
async def test_component_execute_with_context(server):
    """Test component execution that requires context."""
    server.set_initialized(True)

    @server.component(name="context_component")
    def context_component(
        input_data: ValidInput, context: StepflowContext
    ) -> ValidOutput:
        return ValidOutput(
            greeting=f"Context: {input_data.name}!", age_next_year=input_data.age + 1
        )

    request = MethodRequest(
        jsonrpc="2.0",
        id="test-1",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="/context_component", input={"name": "Alice", "age": 25}
        ),
    )

    # Create a mock context for testing
    class MockContext:
        pass

    context = MockContext()
    response = await server.handle_message(request, context)
    assert response.id == request.id
    assert hasattr(response, "result")
    output = response.result.output
    assert output.greeting == "Context: Alice!"
    assert output.age_next_year == 26


@pytest.mark.asyncio
async def test_server_responses_include_jsonrpc(server):
    """Test that server responses include jsonrpc field in JSON encoding."""
    server.set_initialized(True)

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    request = MethodRequest(
        jsonrpc="2.0",
        id="jsonrpc-test",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="/test_component", input={"name": "Test", "age": 30}
        ),
    )

    response = await server.handle_message(request)

    # Encode the response to JSON and verify jsonrpc field is present
    import json

    response_json = json.loads(msgspec.json.encode(response))

    assert "jsonrpc" in response_json
    assert response_json["jsonrpc"] == "2.0"
    assert response_json["id"] == "jsonrpc-test"
    assert "result" in response_json


def test_requires_context():
    """Test requires_context method for determining bidirectional communication."""

    # Test message classes
    class SimpleInput(msgspec.Struct):
        message: str

    class SimpleOutput(msgspec.Struct):
        result: str

    # Create server and register components
    server = StepflowServer()

    def simple_component(input: SimpleInput) -> SimpleOutput:
        """Component that doesn't need context"""
        return SimpleOutput(result=f"Processed: {input.message}")

    async def context_component(
        input: SimpleInput, context: StepflowContext
    ) -> SimpleOutput:
        """Component that needs context for bidirectional communication"""
        return SimpleOutput(result=f"Context processed: {input.message}")

    # Register components
    server.component(simple_component, name="simple", description="Simple component")
    server.component(context_component, name="context", description="Context component")

    # Test cases
    test_cases = [
        # Test 1: initialize method should not require context
        {
            "name": "initialize method",
            "request": MethodRequest(
                jsonrpc="2.0",
                id="test1",
                method=Method.initialize,
                params=InitializeParams(
                    runtime_protocol_version=1, protocol_prefix="python"
                ),
            ),
            "expected": False,
        },
        # Test 2: components/list should not require context
        {
            "name": "components/list method",
            "request": MethodRequest(
                jsonrpc="2.0",
                id="test2",
                method=Method.components_list,
                params=ComponentListParams(),
            ),
            "expected": False,
        },
        # Test 3: simple component execution should not require context
        {
            "name": "simple component execution",
            "request": MethodRequest(
                jsonrpc="2.0",
                id="test3",
                method=Method.components_execute,
                params=ComponentExecuteParams(
                    component="/simple", input={"message": "test"}
                ),
            ),
            "expected": False,
        },
        # Test 4: context component execution should require context
        {
            "name": "context component execution",
            "request": MethodRequest(
                jsonrpc="2.0",
                id="test4",
                method=Method.components_execute,
                params=ComponentExecuteParams(
                    component="/context", input={"message": "test"}
                ),
            ),
            "expected": True,
        },
        # Test 5: non-existent component should not require context (will error later)
        {
            "name": "non-existent component execution",
            "request": MethodRequest(
                jsonrpc="2.0",
                id="test5",
                method=Method.components_execute,
                params=ComponentExecuteParams(
                    component="/nonexistent", input={"message": "test"}
                ),
            ),
            "expected": False,
        },
    ]

    # Run tests
    for test_case in test_cases:
        request = test_case["request"]
        assert isinstance(request, MethodRequest)
        actual = server.requires_context(request)
        expected = test_case["expected"]
        assert actual == expected, (
            f"{test_case['name']}: got {actual}, expected {expected}"
        )


def test_component_context_detection():
    """Test that components are correctly identified as needing context or not."""
    server = StepflowServer()

    class TestInput(msgspec.Struct):
        value: str

    class TestOutput(msgspec.Struct):
        result: str

    # Register a component without context
    @server.component(name="no_context")
    def no_context_component(input: TestInput) -> TestOutput:
        return TestOutput(result=f"No context: {input.value}")

    # Register a component with context
    @server.component(name="with_context")
    def with_context_component(
        input: TestInput, context: StepflowContext
    ) -> TestOutput:
        return TestOutput(result=f"With context: {input.value}")

    # Test component registration
    components = server.get_components()
    assert "/no_context" in components
    assert "/with_context" in components

    # Test context detection via function attributes
    no_context_func = components["/no_context"].function
    with_context_func = components["/with_context"].function

    assert not getattr(no_context_func, "_expects_context", False)
    assert getattr(with_context_func, "_expects_context", False)
