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

import pytest
from dataclasses import dataclass
from typing import Optional
import msgspec
import asyncio
from uuid import UUID

from stepflow_sdk.exceptions import (
    ComponentNotFoundError,
    InputValidationError,
    ServerNotInitializedError,
    StepflowProtocolError,
)
from stepflow_sdk.server import StepflowStdioServer, ComponentEntry
from stepflow_sdk.generated_protocol import (
    Message,
    MethodRequest,
    MethodSuccess,
    Notification,
    ComponentInfoParams,
    ComponentListParams,
    ComponentExecuteParams,
    Component,
    Method,
    InitializeParams,
    InitializeResult,
    Initialized,
)


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
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
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
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    assert "custom_name" in server._components
    component = server.get_component("python://custom_name")
    assert isinstance(component, ComponentEntry)
    assert component.name == "custom_name"
    assert component.input_type == ValidInput
    assert component.output_type == ValidOutput


def test_component_execution(server):
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
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
    request = MethodRequest(
        id=UUID(int=1),
        method=Method.initialize,
        params=InitializeParams(runtime_protocol_version=1, protocol_prefix="python")
    )
    # Runtime <- Server. Initialize method response.
    response = await server._handle_message(request)
    assert response.id == request.id
    assert isinstance(response, MethodSuccess)
    assert isinstance(response.result, InitializeResult)
    assert response.result.server_protocol_version == 1

    assert server._initialized == False

    # Runtime -> Server: Initialized notification.
    notification = Notification(
        method=Method.initialized,
        params=Initialized()
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

    request = MethodRequest(
        id=UUID(int=1),
        method=Method.components_info,
        params=ComponentInfoParams(component="python://test_component")
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    assert isinstance(response, MethodSuccess)
    assert response.result is not None
    assert response.result.info.input_schema == msgspec.json.schema(ValidInput)
    assert response.result.info.output_schema == msgspec.json.schema(ValidOutput)


@pytest.mark.asyncio
async def test_handle_component_info_not_found(server):
    server._initialized = True

    request = MethodRequest(
        id=UUID(int=1),
        method=Method.components_info,
        params=ComponentInfoParams(component="python://non_existent")
    )
    with pytest.raises(ComponentNotFoundError):
        await server._handle_message(request)


@pytest.mark.asyncio
async def test_handle_component_execute(server):
    server._initialized = True

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    request = MethodRequest(
        id=UUID(int=1),
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="python://test_component", 
            input={"name": "Alice", "age": 25}
        )
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    output = response.result.output
    assert output.greeting == "Hello Alice!"
    assert output.age_next_year == 26


@pytest.mark.asyncio
async def test_handle_component_execute_invalid_input(server):
    server._initialized = True

    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    request = MethodRequest(
        id=UUID(int=1),
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="python://test_component", 
            input={"invalid": "input"}
        )
    )

    with pytest.raises(InputValidationError) as e:
        await server._handle_message(request)
    assert e.value.data == {"input": b'{"invalid":"input"}'}


@pytest.mark.asyncio
async def test_handle_list_components(server):
    server._initialized = True

    @server.component(name="component1")
    def test_component1(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    @server.component(name="component2")
    def test_component2(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    request = MethodRequest(
        id=UUID(int=1), 
        method=Method.components_list, 
        params=ComponentListParams()
    )
    response = await server._handle_message(request)
    assert response.id == request.id
    assert len(response.result.components) == 2
    component_urls = [comp.component for comp in response.result.components]
    assert "python://component1" in component_urls
    assert "python://component2" in component_urls


@pytest.mark.asyncio
async def test_handle_unknown_method(server):
    server._initialized = True

    # NOTE: This test creates an invalid method that shouldn't exist
    # We'll simulate this by creating a raw message directly
    request = MethodRequest(
        id=UUID(int=1), 
        method="unknown_method",  # type: ignore - intentionally invalid
        params=ComponentListParams()
    )

    with pytest.raises(StepflowProtocolError, match="Unknown method 'unknown_method'"):
        await server._handle_message(request)


@pytest.mark.asyncio
async def test_uninitialized_server(server):
    request = MethodRequest(
        id=UUID(int=1), 
        method=Method.components_list, 
        params=ComponentListParams()
    )

    with pytest.raises(ServerNotInitializedError):
        await server._handle_message(request)


@pytest.mark.asyncio
async def test_server_responses_include_jsonrpc(server):
    """Test that server responses include jsonrpc field in JSON encoding."""
    server._initialized = True
    
    @server.component(name="test_component")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1)

    request = MethodRequest(
        id="jsonrpc-test",
        method=Method.components_execute,
        params=ComponentExecuteParams(
            component="python://test_component", 
            input={"name": "Test", "age": 30}
        )
    )
    
    response = await server._handle_message(request)
    
    # Encode the response to JSON and verify jsonrpc field is present
    import json
    response_json = json.loads(msgspec.json.encode(response))
    
    assert "jsonrpc" in response_json
    assert response_json["jsonrpc"] == "2.0"
    assert response_json["id"] == "jsonrpc-test"
    assert "result" in response_json
