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
- Component lookup (exact and pattern-based)
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

    assert "/test_component" in server.get_components()
    result = server.get_component("/test_component")
    assert result is not None
    component, path_params = result
    assert isinstance(component, ComponentEntry)
    assert path_params == {}
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
    result = server.get_component("/custom_name")
    assert result is not None
    component, path_params = result
    assert isinstance(component, ComponentEntry)
    assert path_params == {}
    assert component.name == "/custom_name"
    assert component.input_type == ValidInput
    assert component.output_type == ValidOutput


def test_component_execution(server):
    @server.component
    def test_component(input_data: ValidInput) -> ValidOutput:
        return ValidOutput(
            greeting=f"Hello {input_data.name}!", age_next_year=input_data.age + 1
        )

    lookup_result = server.get_component("/test_component")
    assert lookup_result is not None
    component, path_params = lookup_result
    assert path_params == {}
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
    expected_components = ["/component1", "/component2", "/udf"]

    # LangChain components may be registered if langchain is available
    try:
        import langchain_core  # noqa: F401

        expected_components.extend(["/langchain/invoke"])
    except ImportError:
        pass

    assert len(components) == len(expected_components)
    for expected in expected_components:
        assert expected in components

    component1 = components["/component1"]
    assert isinstance(component1, ComponentEntry)
    assert component1.name == "/component1"
    assert component1.input_type == ValidInput
    assert component1.output_type == ValidOutput

    component2 = components["/component2"]
    assert isinstance(component2, ComponentEntry)
    assert component2.name == "/component2"
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

    lookup_result = server.get_component("/sync_component")
    assert lookup_result is not None
    component, path_params = lookup_result
    assert path_params == {}
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

    lookup_result = server.get_component("/async_component")
    assert lookup_result is not None
    component, path_params = lookup_result
    assert path_params == {}
    assert inspect.iscoroutinefunction(component.function)
    result = await component.function(ValidInput(name="Bob", age=30))
    assert isinstance(result, ValidOutput)
    assert result.greeting == "Sync Hello Bob!"
    assert result.age_next_year == 31

    server2 = StepflowServer()
    server2.component(async_component, name="async_component2")

    lookup_result2 = server2.get_component("/async_component2")
    assert lookup_result2 is not None
    component2, path_params2 = lookup_result2
    assert path_params2 == {}
    assert inspect.iscoroutinefunction(component2.function)
    result2 = await component2.function(ValidInput(name="Bob", age=30))
    assert isinstance(result2, ValidOutput)
    assert result2.greeting == "Sync Hello Bob!"
    assert result2.age_next_year == 31


# =============================================================================
# Path Parameter Tests
# =============================================================================


def test_wildcard_path_parameter():
    """Test wildcard path parameter {*name} captures remaining path."""
    server = StepflowServer(include_builtins=False)

    @server.component(name="core/{*component}")
    def core_handler(input: ValidInput, component: str) -> ValidOutput:
        return ValidOutput(greeting=f"Component: {component}", age_next_year=input.age)

    # Test wildcard matching with nested path
    result = server.get_component("/core/my/nested/component")
    assert result is not None
    component, path_params = result
    assert path_params == {"component": "my/nested/component"}
    assert component.name == "/core/{*component}"

    # Test single segment also matches
    result2 = server.get_component("/core/simple")
    assert result2 is not None
    _, path_params2 = result2
    assert path_params2 == {"component": "simple"}

    # Test no match for just the prefix
    assert server.get_component("/core") is None
    assert server.get_component("/core/") is None


def test_single_segment_path_parameter():
    """Test single segment path parameter {name} captures one segment."""
    server = StepflowServer(include_builtins=False)

    @server.component(name="users/{user_id}/profile")
    def user_profile(input: ValidInput, user_id: str) -> ValidOutput:
        return ValidOutput(greeting=f"User: {user_id}", age_next_year=input.age)

    # Test matching
    result = server.get_component("/users/alice/profile")
    assert result is not None
    component, path_params = result
    assert path_params == {"user_id": "alice"}
    assert component.name == "/users/{user_id}/profile"

    # Test non-matching (too many segments)
    assert server.get_component("/users/alice/bob/profile") is None

    # Test non-matching (wrong suffix)
    assert server.get_component("/users/alice/settings") is None


def test_multiple_path_parameters():
    """Test multiple path parameters in one path."""
    server = StepflowServer(include_builtins=False)

    @server.component(name="orgs/{org}/repos/{repo}")
    def repo_handler(input: ValidInput, org: str, repo: str) -> ValidOutput:
        return ValidOutput(greeting=f"{org}/{repo}", age_next_year=input.age)

    result = server.get_component("/orgs/acme/repos/my-project")
    assert result is not None
    component, path_params = result
    assert path_params == {"org": "acme", "repo": "my-project"}


def test_path_params_passed_to_function():
    """Test that path parameters are passed as kwargs to function."""
    server = StepflowServer(include_builtins=False)

    captured_params = {}

    @server.component(name="api/{version}/{*path}")
    def api_handler(input: ValidInput, version: str, path: str) -> ValidOutput:
        captured_params["version"] = version
        captured_params["path"] = path
        return ValidOutput(greeting=f"v{version}: {path}", age_next_year=input.age)

    result = server.get_component("/api/v2/users/list")
    assert result is not None
    component, path_params = result
    assert path_params == {"version": "v2", "path": "users/list"}

    # Call function with path params
    output = component.function(ValidInput(name="test", age=25), **path_params)
    assert captured_params == {"version": "v2", "path": "users/list"}
    assert output.greeting == "vv2: users/list"


def test_exact_match_takes_precedence():
    """Test that exact component matches take precedence over wildcards."""
    server = StepflowServer(include_builtins=False)

    @server.component(name="api/{*path}")
    def wildcard_handler(input: ValidInput, path: str) -> ValidOutput:
        return ValidOutput(greeting=f"Wildcard: {path}", age_next_year=input.age)

    @server.component(name="api/health")
    def health_handler(input: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="Health OK", age_next_year=input.age)

    # Exact match should win
    result = server.get_component("/api/health")
    assert result is not None
    component, path_params = result
    assert path_params == {}
    assert component.name == "/api/health"

    # Wildcard should match other paths
    result2 = server.get_component("/api/other/path")
    assert result2 is not None
    component2, path_params2 = result2
    assert path_params2 == {"path": "other/path"}
    assert component2.name == "/api/{*path}"


def test_get_components_includes_wildcard():
    """Test that get_components() includes wildcard components."""
    server = StepflowServer(include_builtins=False)

    @server.component(name="exact")
    def exact_handler(input: ValidInput) -> ValidOutput:
        return ValidOutput(greeting="exact", age_next_year=input.age)

    @server.component(name="wild/{*path}")
    def wildcard_handler(input: ValidInput, path: str) -> ValidOutput:
        return ValidOutput(greeting=f"wild: {path}", age_next_year=input.age)

    components = server.get_components()
    assert "/exact" in components
    assert "/wild/{*path}" in components
    # Note: LangChain components may also be present if langchain is installed
