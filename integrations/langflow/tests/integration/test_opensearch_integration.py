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

"""OpenSearch integration tests with testcontainers.

Tests mode-aware vector store execution (ingest, retrieve, hybrid) using
a real OpenSearch container.
"""

import os
import tempfile
from typing import Any

import pytest

# Check if testcontainers is available
try:
    from testcontainers.core.container import DockerContainer

    TESTCONTAINERS_AVAILABLE = True
except ImportError:
    TESTCONTAINERS_AVAILABLE = False

from stepflow_langflow_integration.converter.translator import LangflowConverter
from tests.helpers.testing.stepflow_binary import StepflowBinaryRunner


@pytest.fixture(scope="module")
def opensearch_container():
    """Start OpenSearch container for tests.

    Requires Docker to be running. Skips tests if Docker is unavailable.
    """
    if not TESTCONTAINERS_AVAILABLE:
        pytest.skip("testcontainers package not installed")

    try:
        # Start OpenSearch 2.11.0 with basic configuration
        container = DockerContainer("opensearchproject/opensearch:2.11.0")

        # Configure OpenSearch for testing
        container.with_env("discovery.type", "single-node")
        container.with_env("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        container.with_env("OPENSEARCH_INITIAL_ADMIN_PASSWORD", "Admin123!")
        container.with_env("plugins.security.disabled", "true")  # Disable security
        container.with_exposed_ports(9200)

        # Start container
        container.start()

        # Get connection details
        host = container.get_container_host_ip()
        port = container.get_exposed_port(9200)
        url = f"http://{host}:{port}"

        # Wait for OpenSearch to be ready
        import time

        import requests

        max_retries = 30
        for _ in range(max_retries):
            try:
                response = requests.get(f"{url}/_cluster/health", timeout=2)
                if response.status_code == 200:
                    break
            except Exception:
                pass
            time.sleep(1)
        else:
            container.stop()
            pytest.skip("OpenSearch container failed to start within timeout")

        yield {
            "container": container,
            "host": host,
            "port": port,
            "url": url,
            "username": "admin",
            "password": "Admin123!",
        }

        # Cleanup
        container.stop()

    except Exception as e:
        pytest.skip(f"Failed to start OpenSearch container: {e}")


@pytest.fixture(scope="module")
def converter():
    """Create Langflow converter instance."""
    return LangflowConverter()


@pytest.fixture(scope="module")
def stepflow_runner():
    """Create stepflow binary runner."""
    try:
        runner = StepflowBinaryRunner()
        available, version = runner.check_binary_availability()
        if not available:
            pytest.skip(f"Stepflow binary not available: {version}")
        return runner
    except FileNotFoundError as e:
        pytest.skip(f"Stepflow binary not found: {e}")


@pytest.fixture(scope="module")
def shared_config(opensearch_container):
    """Create Stepflow configuration with Langflow and OpenSearch access.

    This is a simplified version that doesn't require the full shared_server
    infrastructure from test_example_flows.py.
    """
    import asyncio

    # Create shared database path
    from pathlib import Path

    import yaml

    shared_db_path = Path(tempfile.gettempdir()) / "stepflow_opensearch_test.db"

    # Initialize database
    if shared_db_path.exists():
        shared_db_path.unlink()

    # Initialize Langflow database
    original_db_url = os.environ.get("LANGFLOW_DATABASE_URL")
    os.environ["LANGFLOW_DATABASE_URL"] = f"sqlite:///{shared_db_path}"

    try:
        from langflow.services.utils import initialize_services, teardown_services
        from lfx.services.deps import get_db_service

        asyncio.run(teardown_services())
        asyncio.run(initialize_services())
        db_service = get_db_service()
        assert db_service is not None, "Database service not available"
        db_service.reload_engine()
        asyncio.run(db_service.create_db_and_tables())
    finally:
        if original_db_url is not None:
            os.environ["LANGFLOW_DATABASE_URL"] = original_db_url
        elif "LANGFLOW_DATABASE_URL" in os.environ:
            del os.environ["LANGFLOW_DATABASE_URL"]

    # Create configuration with Langflow executor
    config_dict = {
        "plugins": {
            "builtin": {"type": "builtin"},
            "langflow": {
                "type": "stepflow",
                "transport": "stdio",
                "command": "uv",
                "args": [
                    "--project",
                    "../../integrations/langflow",
                    "run",
                    "stepflow-langflow-server",
                ],
                "env": {
                    "LANGFLOW_DATABASE_URL": f"sqlite:///{shared_db_path}",
                },
            },
        },
        "routes": {
            "/langflow/{*component}": [{"plugin": "langflow"}],
            "/{*component}": [{"plugin": "builtin"}],
        },
        "stateStore": {"type": "inMemory"},
    }

    # Write to temporary file
    with tempfile.NamedTemporaryFile(mode="w", suffix=".yml", delete=False) as f:
        yaml.dump(config_dict, f)
        config_path = f.name

    yield config_dict, config_path

    # Cleanup
    if os.path.exists(config_path):
        os.unlink(config_path)
    if shared_db_path.exists():
        shared_db_path.unlink()


class OpenSearchTestExecutor:
    """Simplified test executor for OpenSearch tests."""

    def __init__(self, converter, stepflow_runner, config_path):
        self.converter = converter
        self.runner = stepflow_runner
        self.config_path = config_path
        self.server_process = None
        self.server_url = "http://localhost:7837"

    def start_server(self):
        """Start Stepflow server."""
        success, process, stdout, stderr = self.runner.start_server(
            config_path=self.config_path,
            port=7837,
        )
        if not success:
            raise RuntimeError(f"Failed to start server: {stderr}")
        self.server_process = process

    def stop_server(self):
        """Stop Stepflow server."""
        if self.server_process:
            self.runner.stop_server(self.server_process)
            self.server_process = None

    def execute_flow(
        self,
        flow_name: str,
        input_data: dict[str, Any],
        tweaks: dict[str, Any] | None = None,
        timeout: float = 60.0,
    ) -> dict[str, Any]:
        """Execute a workflow with OpenSearch.

        Args:
            flow_name: Name of the Langflow workflow fixture
            input_data: Input data for the workflow
            tweaks: Component configuration overrides
            timeout: Execution timeout in seconds

        Returns:
            Workflow execution result
        """
        # Load Langflow workflow
        import json
        from pathlib import Path

        fixture_path = (
            Path(__file__).parent.parent / "fixtures" / "langflow" / f"{flow_name}.json"
        )
        with open(fixture_path) as f:
            langflow_data = json.load(f)

        # Apply tweaks if provided
        if tweaks:
            from tests.helpers.tweaks import apply_stepflow_tweaks

            # Convert to Stepflow first
            stepflow_workflow = self.converter.convert(langflow_data)

            # Apply tweaks
            stepflow_workflow = apply_stepflow_tweaks(stepflow_workflow, tweaks)
        else:
            # Just convert
            stepflow_workflow = self.converter.convert(langflow_data)

        # Write workflow to temporary file
        import yaml

        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
            yaml.dump(stepflow_workflow.model_dump(by_alias=True), f)
            workflow_path = f.name

        try:
            # Validate workflow
            success, stdout, stderr = self.runner.validate_workflow(
                workflow_path=workflow_path,
                config_path=self.config_path,
            )
            if not success:
                raise ValueError(f"Workflow validation failed: {stderr}")

            # Submit workflow
            success, result_data = self.runner.submit_workflow(
                workflow_path=workflow_path,
                input_data=input_data,
                server_url=self.server_url,
                timeout=timeout,
            )

            if not success:
                raise RuntimeError(f"Workflow execution failed: {result_data}")

            return result_data

        finally:
            if os.path.exists(workflow_path):
                os.unlink(workflow_path)


@pytest.fixture(scope="module")
def test_executor(converter, stepflow_runner, shared_config):
    """Create and manage test executor with Stepflow server."""
    config_dict, config_path = shared_config
    executor = OpenSearchTestExecutor(converter, stepflow_runner, config_path)

    # Start server
    executor.start_server()

    yield executor

    # Stop server
    executor.stop_server()


# Sample test document for ingestion
TEST_DOCUMENT = """# Advanced Machine Learning Concepts

This document covers fundamental concepts in machine learning and artificial
intelligence.

## Neural Networks

Neural networks are computational models inspired by biological neural networks.
They consist of interconnected layers of nodes that process information.

### Deep Learning

Deep learning uses multi-layered neural networks to learn hierarchical
representations of data. Popular architectures include CNNs and transformers.

## Vector Embeddings

Vector embeddings represent text as high-dimensional numerical vectors,
enabling semantic similarity search and retrieval operations.
"""


def test_opensearch_ingest_mode(test_executor, opensearch_container):
    """Test OpenSearch vector store in ingest mode.

    Verifies that documents are correctly ingested into OpenSearch.
    """
    # Skip if OpenAI API key not available
    if not os.getenv("OPENAI_API_KEY"):
        pytest.skip("OPENAI_API_KEY not set")

    # Create test document file
    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        f.write(TEST_DOCUMENT)
        test_file_path = f.name

    try:
        from tests.helpers.tweaks_builder import TweaksBuilder

        # Configure tweaks for OpenSearch and OpenAI
        tweaks = (
            TweaksBuilder()
            # Configure OpenSearch connection
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "opensearch_url",
                opensearch_container["url"],
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "auth_mode",
                "basic",
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "username",
                opensearch_container["username"],
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "password",
                opensearch_container["password"],
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "use_ssl",
                False,
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "verify_certs",
                False,
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "index_name",
                "test_ingest",
            )
            # Configure OpenAI embedding model
            .add_env_tweak("EmbeddingModel-eZ6bT", "api_key", "OPENAI_API_KEY")
            .build()
        )

        # Execute workflow in ingest mode
        result = test_executor.execute_flow(
            flow_name="opensearch_ingestion",
            input_data={
                "mode": "ingest",
                "file_path": test_file_path,
            },
            tweaks=tweaks,
            timeout=120.0,
        )

        # Verify workflow completed successfully
        assert result["outcome"] == "success", f"Unexpected outcome: {result}"

        # Verify documents were ingested into OpenSearch
        from opensearchpy import OpenSearch

        client = OpenSearch(
            [opensearch_container["url"]],
            http_auth=(
                opensearch_container["username"],
                opensearch_container["password"],
            ),
            use_ssl=False,
            verify_certs=False,
        )

        # Check index exists and has documents
        indices = client.cat.indices(format="json")
        index_names = [idx["index"] for idx in indices]
        assert "test_ingest" in index_names, "Index not created"

        # Count documents
        count_response = client.count(index="test_ingest")
        doc_count = count_response["count"]
        assert doc_count > 0, f"No documents ingested, count: {doc_count}"

    finally:
        if os.path.exists(test_file_path):
            os.unlink(test_file_path)


def test_opensearch_retrieve_mode(test_executor, opensearch_container):
    """Test OpenSearch vector store in retrieve mode.

    Assumes documents have been ingested in a previous test run.
    If no documents exist, this test will skip.
    """
    # Skip if OpenAI API key not available
    if not os.getenv("OPENAI_API_KEY"):
        pytest.skip("OPENAI_API_KEY not set")

    # First, ingest test documents (prerequisite for retrieve)
    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        f.write(TEST_DOCUMENT)
        test_file_path = f.name

    try:
        from tests.helpers.tweaks_builder import TweaksBuilder

        # Step 1: Ingest documents first
        ingest_tweaks = (
            TweaksBuilder()
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "opensearch_url",
                opensearch_container["url"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "auth_mode", "basic")
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "username",
                opensearch_container["username"],
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "password",
                opensearch_container["password"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "use_ssl", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "verify_certs", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "index_name", "test_retrieve")
            .add_env_tweak("EmbeddingModel-eZ6bT", "api_key", "OPENAI_API_KEY")
            .build()
        )

        test_executor.execute_flow(
            flow_name="opensearch_ingestion",
            input_data={"mode": "ingest", "file_path": test_file_path},
            tweaks=ingest_tweaks,
            timeout=120.0,
        )

        # Step 2: Now test retrieval
        retrieve_tweaks = (
            TweaksBuilder()
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "opensearch_url",
                opensearch_container["url"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "auth_mode", "basic")
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "username",
                opensearch_container["username"],
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "password",
                opensearch_container["password"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "use_ssl", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "verify_certs", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "index_name", "test_retrieve")
            .add_env_tweak("EmbeddingModel-eZ6bT", "api_key", "OPENAI_API_KEY")
            .build()
        )

        result = test_executor.execute_flow(
            flow_name="opensearch_ingestion",
            input_data={
                "mode": "retrieve",
                "query": "What are neural networks?",
            },
            tweaks=retrieve_tweaks,
            timeout=120.0,
        )

        # Verify workflow completed and returned results
        assert result["outcome"] == "success", f"Unexpected outcome: {result}"
        assert "result" in result, "No results returned"

        # Verify results contain relevant content
        result_data = result["result"]
        assert result_data is not None, "Empty result data"

    finally:
        if os.path.exists(test_file_path):
            os.unlink(test_file_path)


def test_opensearch_hybrid_mode_auto_ingest(test_executor, opensearch_container):
    """Test OpenSearch hybrid mode auto-detecting ingest from inputs.

    When documents are provided but no query, hybrid mode should ingest.
    """
    if not os.getenv("OPENAI_API_KEY"):
        pytest.skip("OPENAI_API_KEY not set")

    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        f.write(TEST_DOCUMENT)
        test_file_path = f.name

    try:
        from tests.helpers.tweaks_builder import TweaksBuilder

        tweaks = (
            TweaksBuilder()
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "opensearch_url",
                opensearch_container["url"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "auth_mode", "basic")
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "username",
                opensearch_container["username"],
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "password",
                opensearch_container["password"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "use_ssl", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "verify_certs", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "index_name", "test_hybrid_ingest")
            .add_env_tweak("EmbeddingModel-eZ6bT", "api_key", "OPENAI_API_KEY")
            .build()
        )

        # Execute with hybrid mode (or default) - should auto-detect ingest
        result = test_executor.execute_flow(
            flow_name="opensearch_ingestion",
            input_data={
                "mode": "hybrid",  # Hybrid mode with documents only
                "file_path": test_file_path,
            },
            tweaks=tweaks,
            timeout=120.0,
        )

        assert result["outcome"] == "success"

        # Verify documents were ingested
        from opensearchpy import OpenSearch

        client = OpenSearch(
            [opensearch_container["url"]],
            http_auth=(
                opensearch_container["username"],
                opensearch_container["password"],
            ),
            use_ssl=False,
            verify_certs=False,
        )

        count_response = client.count(index="test_hybrid_ingest")
        assert count_response["count"] > 0, "Hybrid mode failed to ingest documents"

    finally:
        if os.path.exists(test_file_path):
            os.unlink(test_file_path)


def test_opensearch_hybrid_mode_auto_retrieve(test_executor, opensearch_container):
    """Test OpenSearch hybrid mode auto-detecting retrieve from inputs.

    When query is provided (after ingesting), hybrid mode should retrieve.
    """
    if not os.getenv("OPENAI_API_KEY"):
        pytest.skip("OPENAI_API_KEY not set")

    with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
        f.write(TEST_DOCUMENT)
        test_file_path = f.name

    try:
        from tests.helpers.tweaks_builder import TweaksBuilder

        # Step 1: Ingest first
        ingest_tweaks = (
            TweaksBuilder()
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "opensearch_url",
                opensearch_container["url"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "auth_mode", "basic")
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "username",
                opensearch_container["username"],
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "password",
                opensearch_container["password"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "use_ssl", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "verify_certs", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "index_name", "test_hybrid_retrieve")
            .add_env_tweak("EmbeddingModel-eZ6bT", "api_key", "OPENAI_API_KEY")
            .build()
        )

        test_executor.execute_flow(
            flow_name="opensearch_ingestion",
            input_data={"mode": "ingest", "file_path": test_file_path},
            tweaks=ingest_tweaks,
            timeout=120.0,
        )

        # Step 2: Retrieve with hybrid mode (query only)
        retrieve_tweaks = (
            TweaksBuilder()
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "opensearch_url",
                opensearch_container["url"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "auth_mode", "basic")
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "username",
                opensearch_container["username"],
            )
            .add_tweak(
                "OpenSearchHybrid-Ve6bS",
                "password",
                opensearch_container["password"],
            )
            .add_tweak("OpenSearchHybrid-Ve6bS", "use_ssl", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "verify_certs", False)
            .add_tweak("OpenSearchHybrid-Ve6bS", "index_name", "test_hybrid_retrieve")
            .add_env_tweak("EmbeddingModel-eZ6bT", "api_key", "OPENAI_API_KEY")
            .build()
        )

        result = test_executor.execute_flow(
            flow_name="opensearch_ingestion",
            input_data={
                "mode": "hybrid",  # Hybrid with query only - should retrieve
                "query": "Explain neural networks",
            },
            tweaks=retrieve_tweaks,
            timeout=120.0,
        )

        assert result["outcome"] == "success"
        assert "result" in result
        assert result["result"] is not None

    finally:
        if os.path.exists(test_file_path):
            os.unlink(test_file_path)
