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

"""Tests for the core StepflowServer component management.

This test file focuses on testing the core server functionality:
- Component registration and management
- Component lookup by component ID
- Context detection
"""

import inspect

import msgspec
import pytest

from stepflow_py.worker.server import ComponentEntry, StepflowServer


# Test message classes
class ValidInput(msgspec.Struct):
    name: str
    age: int


class ValidOutput(msgspec.Struct):
    greeting: str
    age_next_year: int


@pytest.fixture
def server():
    return StepflowServer()


def test_component_registration(server):
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    assert "test_component" in server.get_components()
    result = server.get_component("test_component")
    assert result is not None
    assert isinstance(result, ComponentEntry)
    assert result.name == "test_component"
    assert result.path == "/test_component"
    assert result.input_type == ValidInput
    assert result.output_type == ValidOutput


def test_component_with_custom_subpath(server):
    @server.component(subpath="custom_name")
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    # Component ID defaults to function name, subpath is custom
    assert "test_component" in server.get_components()
    result = server.get_component("test_component")
    assert result is not None
    assert isinstance(result, ComponentEntry)
    assert result.name == "test_component"
    assert result.path == "/custom_name"
    assert result.input_type == ValidInput
    assert result.output_type == ValidOutput


def test_component_execution(server):
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    component = server.get_component("test_component")
    assert component is not None
    result = component.function(ValidInput(name="Alice", age=25))
    assert isinstance(result, ValidOutput)
    assert result.greeting == "Hello Alice!"
    assert result.age_next_year == 26


def test_list_components(server):
    @server.component(subpath="component1")
    def test_component1(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    @server.component(subpath="component2")
    def test_component2(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="", age_next_year=0)

    components = server.get_components()
    expected_components = ["test_component1", "test_component2", "udf"]

    # LangChain components may be registered if langchain is available
    try:
        import langchain_core  # noqa: F401

        expected_components.extend(["langchain_invoke"])
    except ImportError:
        pass

    assert len(components) == len(expected_components)
    for expected in expected_components:
        assert expected in components

    component1 = components["test_component1"]
    assert isinstance(component1, ComponentEntry)
    assert component1.name == "test_component1"
    assert component1.input_type == ValidInput
    assert component1.output_type == ValidOutput

    component2 = components["test_component2"]
    assert isinstance(component2, ComponentEntry)
    assert component2.name == "test_component2"
    assert component2.input_type == ValidInput
    assert component2.output_type == ValidOutput


@pytest.mark.asyncio
async def test_decorator_sync():
    server = StepflowServer()

    @server.component
    def sync_component(input: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Sync Hello {input.name}!", age_next_year=input.age + 1
        )

    component = server.get_component("sync_component")
    assert component is not None
    result = component.function(ValidInput(name="Bob", age=30))
    assert isinstance(result, ValidOutput)
    assert result.greeting == "Sync Hello Bob!"
    assert result.age_next_year == 31


@pytest.mark.asyncio
async def test_decorator_async():
    server = StepflowServer()

    @server.component
    async def async_component(input: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Sync Hello {input.name}!", age_next_year=input.age + 1
        )

    component = server.get_component("async_component")
    assert component is not None
    assert inspect.iscoroutinefunction(component.function)
    result = await component.function(ValidInput(name="Bob", age=30))
    assert isinstance(result, ValidOutput)
    assert result.greeting == "Sync Hello Bob!"
    assert result.age_next_year == 31

    server2 = StepflowServer()
    server2.component(async_component, subpath="async_component2")

    component2 = server2.get_component("async_component")
    assert component2 is not None
    assert inspect.iscoroutinefunction(component2.function)
    result2 = await component2.function(ValidInput(name="Bob", age=30))
    assert isinstance(result2, ValidOutput)
    assert result2.greeting == "Sync Hello Bob!"
    assert result2.age_next_year == 31


# =============================================================================
# Path Parameter Registration Tests
# =============================================================================


def test_wildcard_path_parameter_registration():
    """Test wildcard path parameter components are registered by function name."""
    server = StepflowServer(include_builtins=False)

    @server.component(subpath="core/{*component}")
    def core_handler(input: ValidInput, component: str) -> ValidOutput:
        return ValidOutput(greeting=f"Component: {component}", age_next_year=input.age)

    # Component is registered by function name (component_id)
    result = server.get_component("core_handler")
    assert result is not None
    assert result.name == "core_handler"
    assert result.path == "/core/{*component}"


def test_single_segment_path_parameter_registration():
    """Test single segment path parameter components are registered by function name."""
    server = StepflowServer(include_builtins=False)

    @server.component(subpath="users/{user_id}/profile")
    def user_profile(input: ValidInput, user_id: str) -> ValidOutput:
        return ValidOutput(greeting=f"User: {user_id}", age_next_year=input.age)

    result = server.get_component("user_profile")
    assert result is not None
    assert result.name == "user_profile"
    assert result.path == "/users/{user_id}/profile"


def test_multiple_path_parameters_registration():
    """Test multiple path parameters — registered by function name."""
    server = StepflowServer(include_builtins=False)

    @server.component(subpath="orgs/{org}/repos/{repo}")
    def repo_handler(input: ValidInput, org: str, repo: str) -> ValidOutput:
        return ValidOutput(greeting=f"{org}/{repo}", age_next_year=input.age)

    result = server.get_component("repo_handler")
    assert result is not None
    assert result.path == "/orgs/{org}/repos/{repo}"


def test_path_params_passed_to_function():
    """Test that path parameters are passed as kwargs to function."""
    server = StepflowServer(include_builtins=False)

    captured_params = {}

    @server.component(subpath="api/{version}/{*path}")
    def api_handler(input: ValidInput, version: str, path: str) -> ValidOutput:
        captured_params["version"] = version
        captured_params["path"] = path
        return ValidOutput(greeting=f"v{version}: {path}", age_next_year=input.age)

    result = server.get_component("api_handler")
    assert result is not None
    assert result.path == "/api/{version}/{*path}"

    # Call function with path params (these would come from the proto request)
    path_params = {"version": "v2", "path": "users/list"}
    output = result.function(ValidInput(name="test", age=25), **path_params)
    assert captured_params == {"version": "v2", "path": "users/list"}
    assert output.greeting == "vv2: users/list"


def test_get_components_includes_wildcard():
    """Test that get_components() includes wildcard-pattern components."""
    server = StepflowServer(include_builtins=False)

    @server.component(subpath="exact")
    def exact_handler(input: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="exact", age_next_year=input.age)

    @server.component(subpath="wild/{*path}")
    def wildcard_handler(input: ValidInput, path: str) -> ValidOutput:
        return ValidOutput(greeting=f"wild: {path}", age_next_year=input.age)

    components = server.get_components()
    assert "exact_handler" in components
    # Wildcard components are keyed by function name (component ID)
    assert "wildcard_handler" in components
    assert components["wildcard_handler"].path == "/wild/{*path}"
