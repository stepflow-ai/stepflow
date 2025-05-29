import pytest
from dataclasses import dataclass
from typing import Optional
import msgspec
import asyncio
from uuid import UUID

from stepflow_sdk.server import StepflowStdioServer, ComponentEntry
from stepflow_sdk.transport import Incoming, MethodResponse
from stepflow_sdk.protocol import ComponentInfoRequest, ComponentInfoResponse, ComponentExecuteRequest, ComponentExecuteResponse

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
    return StepflowStdioServer()

def test_component_registration(server):
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!",
            age_next_year=input_data.age + 1
        )
    
    assert "test_component" in server._components
    component = server.get_component("python://test_component")
    assert isinstance(component, ComponentEntry)
    assert component.name == "test_component"
    assert component.input_type == ValidInput
    assert component.output_type == ValidOutput

def test_component_with_custom_name(server):
    @server.component(name="custom_name")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!",
            age_next_year=input_data.age + 1
        )
    
    assert "custom_name" in server._components
    component = server.get_component("python://custom_name")
    assert isinstance(component, ComponentEntry)
    assert component.name == "custom_name"
    assert component.input_type == ValidInput
    assert component.output_type == ValidOutput

def test_invalid_component_types(server):
    with pytest.raises(TypeError):
        @server.component
        def invalid_component(input_data: InvalidInput) -> ValidOutput:
            return ValidOutput(greeting="", age_next_year=0)
    
    with pytest.raises(TypeError):
        @server.component
        def invalid_component(input_data: ValidInput) -> InvalidOutput:
            return InvalidOutput()

def test_component_execution(server):
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!",
            age_next_year=input_data.age + 1
        )
    
    component = server.get_component("python://test_component")
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
    
    components = server._components
    assert len(components) == 2
    assert "component1" in components
    assert "component2" in components
    
    for name, component in components.items():
        assert isinstance(component, ComponentEntry)
        assert component.name == name
        assert component.input_type == ValidInput
        assert component.output_type == ValidOutput 

@pytest.mark.asyncio
async def test_handle_initialize(server):
    # Runtime -> Server: Initialize method request.
    request = Incoming(
        id=UUID(int=1),
        method="initialize",
        params={}
    )
    # Runtime <- Server. Initialize method response.
    response = await server._handle_message(request)
    assert response.id == request.id
    assert response.result == {"server_protocol_version": 1}

    assert server._initialized == False

    # Runtime -> Server: Initialized notification.
    notification = Incoming(
        method="initialized",
        params={}
    )
    response = await server._handle_message(notification)
    assert response is None

    assert server._initialized == True

@pytest.mark.asyncio
async def test_handle_component_info(server):
    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)
    server._initialized = True

    request = Incoming(
        id=UUID(int=1),
        method="component_info",
        params=msgspec.json.encode(ComponentInfoRequest(component = "python://test_component")),
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    assert response.result is not None
    assert response.result.input_schema == msgspec.json.schema(ValidInput)
    assert response.result.output_schema == msgspec.json.schema(ValidOutput)

@pytest.mark.asyncio
async def test_handle_component_info_not_found(server):
    server._initialized = True

    request = Incoming(
        id=UUID(int=1),
        method="component_info",
        params=msgspec.json.encode(ComponentInfoRequest(component = "python://non_existent")),
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    assert response.error is not None
    assert response.error["code"] == -32601
    assert "not found" in response.error["message"]

@pytest.mark.asyncio
async def test_handle_component_execute(server):
    server._initialized = True

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!",
            age_next_year=input_data.age + 1
        )

    request = Incoming(
        id=UUID(int=1),
        method="component_execute",
        params=msgspec.json.encode(ComponentExecuteRequest(
            component = "python://test_component",
            input = {
                "name": "Alice",
                "age": 25
            }
        ))
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    output = msgspec.json.decode(response.result.output, type=ValidOutput)
    assert output.greeting == "Hello Alice!"
    assert output.age_next_year == 26

@pytest.mark.asyncio
async def test_handle_component_execute_invalid_input(server):
    server._initialized = True
    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    request = Incoming(
        id=UUID(int=1),
        method="component_execute",
        params=msgspec.json.encode(ComponentExecuteRequest(
            component = "python://test_component",
            input = {
                "invalid": "input"
            }
        ))
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    assert response.error is not None
    assert response.error["code"] == -32000

@pytest.mark.asyncio
async def test_handle_component_list(server):
    server._initialized = True
    
    @server.component(name="component1")
    def test_component1(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)
    
    @server.component(name="component2")
    def test_component2(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    request = Incoming(
        id=UUID(int=1),
        method="component_list",
        params={}
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    assert "components" in response.result
    assert len(response.result["components"]) == 2
    assert "component1" in response.result["components"]
    assert "component2" in response.result["components"]

@pytest.mark.asyncio
async def test_handle_unknown_method(server):
    server._initialized = True
    
    request = Incoming(
        id=UUID(int=1),
        method="unknown_method",
        params={}
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    assert response.error is not None
    assert response.error["code"] == -32601
    assert "Unknown method" in response.error["message"]

@pytest.mark.asyncio
async def test_uninitialized_server(server):
    request = Incoming(
        id=UUID(int=1),
        method="component_list",
        params={}
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    assert response.error is not None
    assert response.error["code"] == -32002
    assert "not initialized" in response.error["message"]


def test_custom_component_simple_expression():
    """Test custom component with simple expression code."""
    from stepflow_sdk.main import CustomComponentInput, CustomComponentOutput, custom_component
    
    input_data = CustomComponentInput(
        input_schema={
            "type": "object",
            "properties": {
                "x": {"type": "number"},
                "y": {"type": "number"}
            },
            "required": ["x", "y"]
        },
        code="data['x'] + data['y']",
        input={"x": 5, "y": 3}
    )
    
    result = custom_component(input_data)
    assert isinstance(result, CustomComponentOutput)
    assert result.result == 8


def test_custom_component_lambda_style():
    """Test custom component with lambda-style code."""
    from stepflow_sdk.main import CustomComponentInput, CustomComponentOutput, custom_component
    
    input_data = CustomComponentInput(
        input_schema={
            "type": "object",
            "properties": {
                "numbers": {"type": "array", "items": {"type": "number"}}
            },
            "required": ["numbers"]
        },
        code="sum(data['numbers']) / len(data['numbers'])",
        input={"numbers": [1, 2, 3, 4, 5]}
    )
    
    result = custom_component(input_data)
    assert isinstance(result, CustomComponentOutput)
    assert result.result == 3.0


def test_custom_component_function_body():
    """Test custom component with multi-line function body."""
    from stepflow_sdk.main import CustomComponentInput, CustomComponentOutput, custom_component
    
    input_data = CustomComponentInput(
        input_schema={
            "type": "object",
            "properties": {
                "text": {"type": "string"}
            },
            "required": ["text"]
        },
        code="""words = data['text'].split()
result = {}
for word in words:
    result[word] = len(word)
return result""",
        input={"text": "hello world python"}
    )
    
    result = custom_component(input_data)
    assert isinstance(result, CustomComponentOutput)
    expected = {"hello": 5, "world": 5, "python": 6}
    assert result.result == expected


def test_custom_component_named_function():
    """Test custom component with named function definition."""
    from stepflow_sdk.main import CustomComponentInput, CustomComponentOutput, custom_component
    
    input_data = CustomComponentInput(
        input_schema={
            "type": "object",
            "properties": {
                "items": {"type": "array", "items": {"type": "number"}}
            },
            "required": ["items"]
        },
        code="""
def process_data(data):
    items = data['items']
    return {
        'sum': sum(items),
        'count': len(items),
        'average': sum(items) / len(items) if items else 0
    }
""",
        input={"items": [10, 20, 30]},
        function_name="process_data"
    )
    
    result = custom_component(input_data)
    assert isinstance(result, CustomComponentOutput)
    expected = {"sum": 60, "count": 3, "average": 20.0}
    assert result.result == expected


def test_custom_component_validation_error():
    """Test custom component with invalid input according to schema."""
    from stepflow_sdk.main import CustomComponentInput, custom_component
    
    input_data = CustomComponentInput(
        input_schema={
            "type": "object",
            "properties": {
                "x": {"type": "number"},
                "y": {"type": "number"}
            },
            "required": ["x", "y"]
        },
        code="data['x'] + data['y']",
        input={"x": "not_a_number", "y": 3}  # Invalid: x should be number
    )
    
    with pytest.raises(ValueError, match="Input validation failed"):
        custom_component(input_data)


def test_custom_component_function_not_found():
    """Test custom component when specified function name doesn't exist."""
    from stepflow_sdk.main import CustomComponentInput, custom_component
    
    input_data = CustomComponentInput(
        input_schema={
            "type": "object",
            "properties": {"value": {"type": "number"}},
            "required": ["value"]
        },
        code="""
def some_function(data):
    return data['value'] * 2
""",
        input={"value": 5},
        function_name="missing_function"
    )
    
    with pytest.raises(ValueError, match="Function 'missing_function' not found"):
        custom_component(input_data)


def test_custom_component_code_execution_error():
    """Test custom component with code that raises an error."""
    from stepflow_sdk.main import CustomComponentInput, custom_component
    
    input_data = CustomComponentInput(
        input_schema={
            "type": "object",
            "properties": {"value": {"type": "number"}},
            "required": ["value"]
        },
        code="1 / 0",  # Division by zero
        input={"value": 5}
    )
    
    with pytest.raises(ValueError, match="Code execution failed"):
        custom_component(input_data)


def test_custom_component_non_serializable_result():
    """Test custom component that returns non-JSON serializable result."""
    from stepflow_sdk.main import CustomComponentInput, custom_component
    
    input_data = CustomComponentInput(
        input_schema={
            "type": "object",
            "properties": {"value": {"type": "number"}},
            "required": ["value"]
        },
        code="lambda x: x",  # Returns a function, not JSON serializable
        input={"value": 5}
    )
    
    with pytest.raises(ValueError, match="Result is not JSON serializable"):
        custom_component(input_data) 