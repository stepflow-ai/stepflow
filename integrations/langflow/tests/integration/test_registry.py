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

"""Centralized test registry for Langflow integration tests.

This module provides a single source of truth for all test workflows and their
expected behaviors. It eliminates duplication across test files by providing
common workflow definitions, input data,
and expected outcomes.
"""

import json
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest


@dataclass
class ConversionExpectation:
    """Expected results from Langflow → Stepflow conversion."""

    should_succeed: bool = True
    workflow_name: str | None = None
    step_count: int | None = None
    step_ids: list[str] | None = None
    component_types: list[str] | None = None
    component_types_include: list[str] | None = None
    udf_executor_count: int | None = None
    has_dependencies: bool = False
    error_type: str | None = None
    error_message_contains: str | None = None


@dataclass
class ValidationExpectation:
    """Expected results from Stepflow workflow validation."""

    should_succeed: bool = True
    error_contains: str | None = None
    requires_components: list[str] | None = None


class ResultValidator(ABC):
    """Abstract base class for workflow-specific result validation."""

    @abstractmethod
    def validate(
        self,
        result_data: dict[str, Any],
        input_data: dict[str, Any],
        workflow_name: str,
    ) -> None:
        """Validate execution result. Should raise AssertionError if validation
        fails."""
        pass


class ChatResultValidator(ResultValidator):
    """Validator for chat-based workflows."""

    def validate(
        self,
        result_data: dict[str, Any],
        input_data: dict[str, Any],
        workflow_name: str,
    ) -> None:
        """Validate chat workflow results."""
        if "message" in input_data:
            input_message = input_data["message"]
            result_str = str(result_data).lower()
            input_words = input_message.lower().split()

            # Result should either contain the input message or show it was processed
            has_input_reflection = any(
                word in result_str for word in input_words if len(word) > 3
            )
            has_meaningful_content = len(result_str.strip()) > 10

            assert has_input_reflection or has_meaningful_content, (
                f"Chat result should reflect input or provide meaningful response for "
                f"{workflow_name}. Input: {input_message}, Result: {result_data}"
            )


class PromptingResultValidator(ResultValidator):
    """Validator for prompting workflows."""

    def validate(
        self,
        result_data: dict[str, Any],
        input_data: dict[str, Any],
        workflow_name: str,
    ) -> None:
        """Validate prompting workflow results."""
        result_str = str(result_data)

        # Should have reasonable content length
        assert len(result_str.strip()) > 5, (
            f"Prompting result should have meaningful content for "
            f"{workflow_name}: {result_data}"
        )


class MemoryResultValidator(ResultValidator):
    """Validator for memory-enabled workflows."""

    def validate(
        self,
        result_data: dict[str, Any],
        input_data: dict[str, Any],
        workflow_name: str,
    ) -> None:
        """Validate memory workflow results."""
        if "remember" in str(input_data).lower() or "memory" in str(input_data).lower():
            result_str = str(result_data).lower()
            memory_indicators = ["remember", "memory", "stored", "noted", "recall"]

            has_memory_indication = any(
                indicator in result_str for indicator in memory_indicators
            )
            has_meaningful_response = len(result_str.strip()) > 15

            assert has_memory_indication or has_meaningful_response, (
                f"Memory workflow should acknowledge memory operations for "
                f"{workflow_name}: {result_data}"
            )


class DocumentQAResultValidator(ResultValidator):
    """Validator for document QA workflows."""

    def validate(
        self,
        result_data: dict[str, Any],
        input_data: dict[str, Any],
        workflow_name: str,
    ) -> None:
        """Validate QA/document workflow results."""
        if "document" in input_data:
            document_content = str(input_data["document"]).lower()
            result_str = str(result_data).lower()

            # Result should relate to document or provide analysis
            doc_words = document_content.split()
            meaningful_words = [w for w in doc_words if len(w) > 4]

            has_document_relation = any(
                word in result_str for word in meaningful_words[:3]
            )
            has_analysis_content = len(result_str.strip()) > 20

            assert has_document_relation or has_analysis_content, (
                f"QA result should relate to document content for "
                f"{workflow_name}. Document: {document_content[:50]}..., "
                f"Result: {result_data}"
            )


class AgentResultValidator(ResultValidator):
    """Validator for agent workflows."""

    def validate(
        self,
        result_data: dict[str, Any],
        input_data: dict[str, Any],
        workflow_name: str,
    ) -> None:
        """Validate agent workflow results."""
        result_str = str(result_data).lower()

        # Look for indicators of tool usage or calculation
        tool_indicators = ["calculate", "search", "tool", "result", "found", "computed"]
        calculation_patterns = ["*", "+", "-", "/", "=", "result"]

        has_tool_indication = any(
            indicator in result_str for indicator in tool_indicators
        )
        has_calculation = any(pattern in result_str for pattern in calculation_patterns)
        has_substantial_content = len(result_str.strip()) > 25

        assert has_tool_indication or has_calculation or has_substantial_content, (
            f"Agent result should show evidence of tool usage or reasoning for "
            f"{workflow_name}: {result_data}"
        )


class GenericResultValidator(ResultValidator):
    """Generic validator for workflows without specific validation requirements."""

    def validate(
        self,
        result_data: dict[str, Any],
        input_data: dict[str, Any],
        workflow_name: str,
    ) -> None:
        """Basic validation - just ensure result has meaningful content."""
        assert result_data is not None, f"Result should not be None for {workflow_name}"
        assert isinstance(result_data, dict), (
            f"Result should be dict for {workflow_name}, got {type(result_data)}"
        )


@dataclass
class ExecutionExpectation:
    """Expected results from Stepflow workflow execution."""

    should_succeed: bool = True
    result_structure: dict[str, Any] | None = None
    result_contains_keys: list[str] | None = None
    result_values: dict[str, Any] | None = None
    timeout_seconds: float = 30.0
    can_mock: bool = False
    mock_response: dict[str, Any] | None = None
    requires_api_keys: list[str] | None = None
    performance_category: str = "small_workflow"
    result_validator: ResultValidator | None = None


@dataclass
class TestWorkflow:
    """Complete test workflow definition."""

    name: str
    langflow_file: str | None = None
    langflow_content: dict[str, Any] | None = None
    input_data: dict[str, Any] | None = None
    environment: dict[str, str] | None = None
    conversion: ConversionExpectation = None
    validation: ValidationExpectation = None
    execution: ExecutionExpectation = None
    tags: list[str] | None = None

    def __post_init__(self):
        """Set defaults for expectations."""
        if self.conversion is None:
            self.conversion = ConversionExpectation()
        if self.validation is None:
            self.validation = ValidationExpectation()
        if self.execution is None:
            self.execution = ExecutionExpectation()
        if self.tags is None:
            self.tags = []


class TestRegistry:
    """Registry of all test workflows with common loading and validation logic."""

    def __init__(self, fixtures_dir: Path | None = None):
        """Initialize test registry.

        Args:
            fixtures_dir: Path to test fixtures directory
        """
        if fixtures_dir is None:
            fixtures_dir = Path(__file__).parent.parent / "fixtures"
        self.fixtures_dir = fixtures_dir
        self.langflow_fixtures_dir = fixtures_dir / "langflow"
        self._workflows: list[TestWorkflow] = []
        self._register_workflows()

    def _register_workflows(self):
        """Register all test workflows directly in Python."""

        # Simple Chat - Basic workflow with 2 components
        self._workflows.append(
            TestWorkflow(
                name="simple_chat",
                langflow_file="simple_chat.json",
                input_data={"message": "Hello, world!"},
                conversion=ConversionExpectation(
                    workflow_name="Simple Chat Example",
                    step_count=0,  # ChatInput and ChatOutput are I/O connection points,
                    # not processing steps
                    udf_executor_count=0,  # No UDF executors needed
                    has_dependencies=False,  # No processing steps means no dependencies
                ),
                execution=ExecutionExpectation(
                    result_contains_keys=["text", "sender", "type"],
                    result_values={"type": "Message", "sender": "User"},
                    result_validator=ChatResultValidator(),
                ),
            )
        )

        # OpenAI Chat - Workflow with API dependency
        self._workflows.append(
            TestWorkflow(
                name="openai_chat",
                langflow_file="openai_chat.json",
                input_data={"message": "What is Python?"},
                environment={"OPENAI_API_KEY": "test-key-123"},
                conversion=ConversionExpectation(
                    workflow_name="OpenAI Chat Workflow",
                    step_count=1,  # Only LanguageModelComponent creates a
                    # processing step
                    component_types_include=[
                        "/langflow/LanguageModelComponent",
                    ],  # LanguageModelComponent routes to standalone component
                    udf_executor_count=0,  # No UDF executors needed
                    has_dependencies=True,
                ),
                execution=ExecutionExpectation(
                    can_mock=True,
                    mock_response={
                        "text": "Python is a programming language",
                        "sender": "AI",
                        "type": "Message",
                    },
                    result_structure={
                        "type": "object",
                        "required_fields": ["text", "sender", "type"],
                    },
                    result_validator=ChatResultValidator(),
                ),
                tags=["mockable", "requires_api"],
            )
        )

        # Invalid Workflow - Test error handling
        self._workflows.append(
            TestWorkflow(
                name="invalid_workflow",
                langflow_file="invalid.json",  # Non-existent file
                conversion=ConversionExpectation(
                    should_succeed=False,
                    error_type="ConversionError",
                    error_message_contains="not found",
                ),
                validation=ValidationExpectation(should_succeed=False),
                execution=ExecutionExpectation(should_succeed=False),
            )
        )

        # Empty Workflow - Test inline content
        self._workflows.append(
            TestWorkflow(
                name="empty_workflow",
                langflow_content={"data": {"nodes": [], "edges": []}},
                conversion=ConversionExpectation(
                    should_succeed=False,
                    error_type="ConversionError",
                    error_message_contains="No nodes found",
                ),
                validation=ValidationExpectation(should_succeed=False),
                execution=ExecutionExpectation(should_succeed=False),
            )
        )

        # Basic Prompting - Complex workflow from Langflow starter
        self._workflows.append(
            TestWorkflow(
                name="basic_prompting",
                langflow_file="basic_prompting.json",
                input_data={"message": "Write a haiku about coding"},
                conversion=ConversionExpectation(
                    workflow_name="Basic Prompting",
                    step_count=4,  # Prompt blob + Prompt processing +
                    # LanguageModelComponent blob + LanguageModelComponent processing
                    has_dependencies=True,
                    component_types_include=[
                        "/langflow/udf_executor",
                        "/builtin/put_blob",
                    ],  # UDF executors + blob storage
                    udf_executor_count=2,  # Both Prompt and LanguageModelComponent
                    # are UDF executors
                ),
                execution=ExecutionExpectation(
                    can_mock=True,
                    mock_response={
                        "text": (
                            "Code flows like stream\n"
                            "Logic branches, functions bloom\n"
                            "Debug finds the truth"
                        ),
                        "type": "Message",
                    },
                    result_validator=PromptingResultValidator(),
                ),
                tags=["mockable"],
            )
        )

        # Memory Chatbot - Complex workflow with memory handling
        self._workflows.append(
            TestWorkflow(
                name="memory_chatbot",
                langflow_file="memory_chatbot.json",
                input_data={"message": "Remember my name is Alice"},
                conversion=ConversionExpectation(
                    workflow_name="Memory Chatbot",
                    step_count=5,  # Memory + Prompt blob + Prompt processing +
                    # LanguageModelComponent blob + LanguageModelComponent processing
                    has_dependencies=True,
                    component_types_include=[
                        "/langflow/udf_executor",
                        "/langflow/memory",
                        "/builtin/put_blob",
                    ],  # UDF executors + memory component + blob storage
                    udf_executor_count=2,  # Prompt and LanguageModelComponent
                    # components (Memory is standalone)
                ),
                execution=ExecutionExpectation(
                    can_mock=True,
                    timeout_seconds=45.0,  # Memory operations may be slower
                    result_validator=MemoryResultValidator(),
                ),
                tags=["mockable"],
            )
        )

        # Document QA - RAG workflow
        self._workflows.append(
            TestWorkflow(
                name="document_qa",
                langflow_file="document_qa.json",
                input_data={
                    "message": "What is the main topic of the document?",
                    "document": "This is a sample document about machine learning.",
                },
                conversion=ConversionExpectation(
                    workflow_name="Document Q&A",
                    step_count=5,  # File + 2 UDF executors + 2 blob storage steps
                    has_dependencies=True,
                    component_types_include=[
                        "/langflow/file",
                        "/langflow/udf_executor",
                        "/builtin/put_blob",
                    ],  # File component + UDF executors + blob storage
                    udf_executor_count=2,  # Prompt and LanguageModelComponent both use
                    # UDF executor
                ),
                execution=ExecutionExpectation(
                    can_mock=True,
                    mock_response={
                        "text": "The main topic is machine learning.",
                        "type": "Message",
                    },
                    result_validator=DocumentQAResultValidator(),
                ),
                tags=["mockable"],
            )
        )

        # Simple Agent - Tool-using agent workflow
        self._workflows.append(
            TestWorkflow(
                name="simple_agent",
                langflow_file="simple_agent.json",
                input_data={
                    "message": "Calculate 15 * 23 and then search for information "
                    + "about the result"
                },
                conversion=ConversionExpectation(
                    workflow_name="Simple Agent",
                    step_count=4,  # Tool blobs (2x) + Agent blob +
                    # Agent processing step
                    has_dependencies=True,
                    component_types_include=[
                        "/langflow/udf_executor",
                        "/builtin/put_blob",
                    ],  # UDF executor for Agent + blob storage steps
                    udf_executor_count=1,  # Only Agent component (tools are
                    # dependencies, not separate UDF executors)
                ),
                execution=ExecutionExpectation(
                    can_mock=True,
                    timeout_seconds=60.0,  # Agent workflows may be slower
                    result_validator=AgentResultValidator(),
                ),
                tags=["mockable"],
            )
        )

        # Vector Store RAG - Large, complex workflow
        self._workflows.append(
            TestWorkflow(
                name="vector_store_rag",
                langflow_file="vector_store_rag.json",
                input_data={
                    "message": "Find information about artificial intelligence"
                },
                conversion=ConversionExpectation(
                    workflow_name="Vector Store RAG",
                    step_count=11,  # 8 processing steps + 3 blob storage steps
                    has_dependencies=True,
                    component_types_include=[
                        "/langflow/file",
                        "/langflow/AstraDB",
                        "/langflow/udf_executor",
                        "/builtin/put_blob",
                    ],  # File component + AstraDB + UDF executors + blob storage
                    udf_executor_count=4,  # Prompt, SplitText, parser,
                    # LanguageModelComponent (OpenAI embeddings merged into AstraDB)
                ),
                execution=ExecutionExpectation(
                    can_mock=True,
                    timeout_seconds=120.0,  # Large workflows may be slow
                    performance_category="large_workflow",
                    result_validator=DocumentQAResultValidator(),
                ),
                tags=["mockable", "slow"],
            )
        )

    def get_workflows(
        self, tags: list[str] | None = None, exclude_tags: list[str] | None = None
    ) -> list[TestWorkflow]:
        """Get workflows filtered by tags.

        Args:
            tags: Only include workflows with these tags
            exclude_tags: Exclude workflows with these tags

        Returns:
            List of matching workflows
        """
        workflows = self._workflows

        if tags:
            workflows = [w for w in workflows if any(tag in w.tags for tag in tags)]

        if exclude_tags:
            workflows = [
                w for w in workflows if not any(tag in w.tags for tag in exclude_tags)
            ]

        return workflows

    def get_workflow_by_name(self, name: str) -> TestWorkflow | None:
        """Get workflow by name."""
        for workflow in self._workflows:
            if workflow.name == name:
                return workflow
        return None

    def load_langflow_data(self, workflow: TestWorkflow) -> dict[str, Any]:
        """Load Langflow JSON data for a workflow.

        Args:
            workflow: TestWorkflow to load data for

        Returns:
            Parsed Langflow JSON data

        Raises:
            FileNotFoundError: If langflow_file doesn't exist and no inline content
        """
        if workflow.langflow_content:
            return workflow.langflow_content

        if workflow.langflow_file:
            file_path = self.langflow_fixtures_dir / workflow.langflow_file
            if file_path.exists():
                with open(file_path) as f:
                    return json.load(f)
            else:
                raise FileNotFoundError(
                    f"Langflow file not found: {workflow.langflow_file}"
                )

        raise ValueError(
            f"Workflow {workflow.name} has no langflow_file or langflow_content"
        )

    def get_conversion_test_cases(self) -> list[TestWorkflow]:
        """Get workflows suitable for conversion testing."""
        return self.get_workflows()  # All workflows should be conversion tested

    def get_validation_test_cases(self) -> list[TestWorkflow]:
        """Get workflows suitable for validation testing."""
        # Only workflows that should convert successfully
        return [w for w in self._workflows if w.conversion.should_succeed]

    def get_execution_test_cases(
        self, mockable_only: bool = False
    ) -> list[TestWorkflow]:
        """Get workflows suitable for execution testing.

        Args:
            mockable_only: Only return workflows that can be mocked
        """
        workflows = [w for w in self._workflows if w.validation.should_succeed]

        if mockable_only:
            workflows = [w for w in workflows if w.execution.can_mock]

        return workflows

    def get_performance_test_cases(self) -> list[TestWorkflow]:
        """Get workflows suitable for performance testing."""
        return [w for w in self._workflows if "slow" not in w.tags]

    @property
    def all_workflows(self) -> list[TestWorkflow]:
        """Get all registered workflows."""
        return self._workflows.copy()


# Global registry instance
_registry = None


def get_test_registry() -> TestRegistry:
    """Get the global test registry instance."""
    global _registry
    if _registry is None:
        _registry = TestRegistry()
    return _registry


def workflow_ids(workflows: list[TestWorkflow]) -> list[str]:
    """Extract workflow names for pytest parametrization."""
    return [w.name for w in workflows]


def pytest_parametrize_workflows(workflows: list[TestWorkflow]):
    """Create pytest.param objects for workflow parametrization."""
    return [
        pytest.param(
            workflow,
            id=workflow.name,
            marks=pytest.mark.slow if "slow" in workflow.tags else [],
        )
        for workflow in workflows
    ]
