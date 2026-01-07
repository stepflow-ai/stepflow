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

"""Configuration builder for Stepflow test configurations.

This module provides a flexible builder pattern for creating Stepflow
configurations for different test scenarios.
"""

import contextlib
import sqlite3
import tempfile
from collections.abc import Generator
from pathlib import Path
from typing import Any

import pytest
import yaml


class StepflowConfigBuilder:
    """Builder for creating Stepflow test configurations with context manager support.

    Provides a flexible way to build configuration dictionaries that can be
    serialized to YAML for testing different scenarios. Supports context manager
    usage for automatic resource cleanup.

    Examples:
        # Context manager for automatic cleanup
        with StepflowConfigBuilder() as config:
            config.with_langflow_database("/tmp/test.db")
            with config.to_temp_yaml() as config_path:
                # Use config_path in tests
                pass

        # Auto-managed temp databases
        with StepflowConfigBuilder() as config:
            config.add_temp_langflow_database()
            with config.to_temp_yaml() as config_path:
                # Database and config automatically cleaned up
                pass
    """

    def __init__(self):
        """Initialize with base configuration structure."""
        # Get path to langflow integration root directory
        current_dir = Path(__file__).parent.parent.parent.parent

        # Load environment variables (try .env if available)
        self._env_vars = self._load_env_vars(current_dir)

        # Track temporary resources for cleanup
        self._temp_files: list[Path] = []

        # Base configuration structure
        self._config = {
            "plugins": {
                "builtin": {"type": "builtin"},
                "langflow": {
                    "type": "stepflow",
                    "command": "uv",
                    "args": [
                        "--project",
                        str(current_dir),
                        "run",
                        "stepflow-langflow-server",
                    ],
                    "env": {},
                },
            },
            "routes": {
                "/langflow/{*component}": [{"plugin": "langflow"}],
                "/builtin/{*component}": [{"plugin": "builtin"}],
            },
            "stateStore": {"type": "inMemory"},
        }

    def _load_env_vars(self, current_dir: Path) -> dict[str, str]:
        """Load environment variables with optional .env file support."""
        import os

        env_vars = dict(os.environ)

        # Try loading from .env if not already available
        if not env_vars.get("OPENAI_API_KEY"):
            try:
                from dotenv import load_dotenv

                env_path = current_dir / ".env"
                if env_path.exists():
                    load_dotenv(env_path)
                    # Re-capture after loading .env
                    env_vars = dict(os.environ)
            except ImportError:
                pass

        return env_vars

    def __enter__(self) -> "StepflowConfigBuilder":
        """Enter context manager."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        """Exit context manager and clean up temporary files."""
        self._cleanup_temp_files()

    def _cleanup_temp_files(self) -> None:
        """Clean up all tracked temporary files."""
        for temp_file in self._temp_files:
            if temp_file.exists():
                temp_file.unlink(missing_ok=True)
        self._temp_files.clear()

    def _add_temp_file(self, file_path: Path) -> None:
        """Track a temporary file for cleanup."""
        self._temp_files.append(file_path)

    @contextlib.contextmanager
    def to_temp_yaml(self) -> Generator[str, None, None]:
        """Create temporary YAML config file with automatic cleanup.

        Yields:
            Path to temporary config file
        """
        with tempfile.NamedTemporaryFile(mode="w", suffix=".yml", delete=False) as f:
            f.write(self.to_yaml())
            temp_path = Path(f.name)

        try:
            yield str(temp_path)
        finally:
            temp_path.unlink(missing_ok=True)

    def add_shared_langflow_database(self) -> "StepflowConfigBuilder":
        """Use a single shared Langflow database for all tests.

        This avoids database reinitialization issues by using one database
        across all tests. Tests should use unique session IDs to isolate data.

        Returns:
            Self for method chaining
        """
        # Use a consistent shared database file in temp directory
        import tempfile

        shared_db_path = (
            Path(tempfile.gettempdir()) / "stepflow_langflow_shared_test.db"
        )

        # Only initialize once - if it doesn't exist yet
        if not shared_db_path.exists():
            self._initialize_langflow_database_service(str(shared_db_path))

        # Configure builder to use the shared database
        return self.with_langflow_database(str(shared_db_path))

    def add_temp_langflow_database(self) -> "StepflowConfigBuilder":
        """Create temporary Langflow database with proper schema initialization.

        Creates a temporary SQLite database and initializes it with Langflow's
        schema using their database service. This ensures schema compatibility.

        Returns:
            Self for method chaining
        """
        # Create temporary database file
        with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as db_file:
            db_path = Path(db_file.name)

        # Track for cleanup
        self._add_temp_file(db_path)

        # Initialize the database using Langflow's database service
        self._initialize_langflow_database_service(str(db_path))

        # Configure builder to use the database
        return self.with_langflow_database(str(db_path))

    def _initialize_langflow_database_service(self, database_path: str) -> None:
        """Initialize Langflow database using their service and migration system."""
        import asyncio
        import os

        from langflow.services.deps import get_db_service, get_settings_service

        # Set up environment to point to our database
        original_db_url = os.environ.get("LANGFLOW_DATABASE_URL")
        # SQLite URL format: sqlite:/// + absolute/path = sqlite:////absolute/path
        os.environ["LANGFLOW_DATABASE_URL"] = f"sqlite:///{database_path}"

        try:
            # Clear any existing service cache to force reinitialization
            from lfx.services.manager import get_service_manager

            # Teardown existing services to force reinitialization with new database
            service_manager = get_service_manager()
            service_manager.teardown()

            # Initialize settings service first (reads LANGFLOW_DATABASE_URL from env)
            get_settings_service()

            # Get database service and reload engine with new database URL
            db_service = get_db_service()

            # Force the database service to reload its engine with the new database URL
            db_service.reload_engine()

            # Run the async method to create tables
            asyncio.run(db_service.create_db_and_tables())

        finally:
            # Restore original database URL if it existed
            if original_db_url:
                os.environ["LANGFLOW_DATABASE_URL"] = original_db_url
            else:
                os.environ.pop("LANGFLOW_DATABASE_URL", None)

    def add_temp_sqlite_state_store(
        self, auto_migrate: bool = True
    ) -> "StepflowConfigBuilder":
        """Create temporary SQLite state store database.

        Args:
            auto_migrate: Whether to automatically run migrations

        Returns:
            Self for method chaining
        """
        # Create temporary database file
        with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as db_file:
            db_path = Path(db_file.name)

        # Track for cleanup
        self._add_temp_file(db_path)

        # Configure builder to use the database
        return self.with_sqlite_state_store(str(db_path), auto_migrate)

    def _initialize_langflow_database(self, database_path: str) -> None:
        """Initialize a Langflow SQLite database with required schema."""
        conn = sqlite3.connect(database_path)
        try:
            cursor = conn.cursor()

            # Create the message table with schema matching Langflow's expectations
            cursor.execute(
                """
            CREATE TABLE IF NOT EXISTS message (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                sender TEXT,
                sender_name TEXT,
                session_id TEXT,
                text TEXT,
                error INTEGER DEFAULT 0,
                edit INTEGER DEFAULT 0,
                files TEXT,
                properties TEXT,
                category TEXT,
                content_blocks TEXT,
                flow_id TEXT
            )
            """
            )

            # Create indices for performance
            cursor.execute(
                """
            CREATE INDEX IF NOT EXISTS idx_message_timestamp ON message (timestamp)
            """
            )
            cursor.execute(
                """
            CREATE INDEX IF NOT EXISTS idx_message_session_id ON message (session_id)
            """
            )
            cursor.execute(
                """
            CREATE INDEX IF NOT EXISTS idx_message_error ON message (error)
            """
            )

            conn.commit()
        finally:
            conn.close()

    def with_plugin_env(
        self, plugin_name: str, env_vars: dict[str, str]
    ) -> "StepflowConfigBuilder":
        """Add environment variables to a plugin configuration.

        Args:
            plugin_name: Name of the plugin to configure
            env_vars: Dictionary of environment variable names and values

        Returns:
            Self for method chaining
        """
        plugins: dict[str, Any] = self._config["plugins"]  # type: ignore[assignment]
        if plugin_name not in plugins:
            raise ValueError(f"Plugin '{plugin_name}' not found in configuration")

        plugin_config: dict[str, Any] = plugins[plugin_name]
        if "env" not in plugin_config:
            plugin_config["env"] = {}

        plugin_config["env"].update(env_vars)
        return self

    def with_sqlite_state_store(
        self, database_path: str, auto_migrate: bool = True
    ) -> "StepflowConfigBuilder":
        """Configure SQLite state store.

        Args:
            database_path: Path to SQLite database file
            auto_migrate: Whether to automatically run migrations

        Returns:
            Self for method chaining
        """
        self._config["stateStore"] = {
            "type": "sqlite",
            "databaseUrl": f"sqlite:{database_path}",
            "autoMigrate": auto_migrate,
            "maxConnections": 10,
        }
        return self

    def with_langflow_database(self, database_path: str) -> "StepflowConfigBuilder":
        """Configure Langflow database environment variables.

        Args:
            database_path: Path to Langflow SQLite database file

        Returns:
            Self for method chaining
        """
        return self.with_plugin_env(
            "langflow",
            {
                # SQLite URL format: sqlite:/// + absolute/path = sqlite:////absolute/path
                "LANGFLOW_DATABASE_URL": f"sqlite:///{database_path}",
                "LANGFLOW_AUTO_LOGIN": "false",
            },
        )

    def with_custom_plugin_env(self, **env_vars: str) -> "StepflowConfigBuilder":
        """Add custom environment variables to langflow plugin.

        Args:
            **env_vars: Environment variables as keyword arguments

        Returns:
            Self for method chaining
        """
        return self.with_plugin_env("langflow", env_vars)

    def with_openai_env(self) -> "StepflowConfigBuilder":
        """Add OpenAI environment variables if available.

        Raises:
            RuntimeError: If OPENAI_API_KEY is not available

        Returns:
            Self for method chaining
        """
        api_key = self._env_vars.get("OPENAI_API_KEY")
        if not api_key:
            pytest.skip(
                "OPENAI_API_KEY environment variable is required but not available"
            )

        return self.with_plugin_env("langflow", {"OPENAI_API_KEY": api_key})

    def with_anthropic_env(self) -> "StepflowConfigBuilder":
        """Add Anthropic environment variables if available.

        Raises:
            RuntimeError: If ANTHROPIC_API_KEY is not available

        Returns:
            Self for method chaining
        """
        api_key = self._env_vars.get("ANTHROPIC_API_KEY")
        if not api_key:
            pytest.skip(
                "ANTHROPIC_API_KEY environment variable is required but not available"
            )

        return self.with_plugin_env("langflow", {"ANTHROPIC_API_KEY": api_key})

    def with_google_env(self) -> "StepflowConfigBuilder":
        """Add Google environment variables if available.

        Raises:
            RuntimeError: If GOOGLE_API_KEY is not available

        Returns:
            Self for method chaining
        """
        api_key = self._env_vars.get("GOOGLE_API_KEY")
        if not api_key:
            pytest.skip(
                "GOOGLE_API_KEY environment variable is required but not available"
            )

        return self.with_plugin_env("langflow", {"GOOGLE_API_KEY": api_key})

    def with_astra_db_env(self) -> "StepflowConfigBuilder":
        """Add AstraDB environment variables if available.

        Raises:
            RuntimeError: If required AstraDB environment variables are not available

        Returns:
            Self for method chaining
        """
        api_endpoint = self._env_vars.get("ASTRA_DB_API_ENDPOINT")
        application_token = self._env_vars.get("ASTRA_DB_APPLICATION_TOKEN")

        if not api_endpoint:
            pytest.skip("ASTRA_DB_API_ENDPOINT environment variable is required")

        if not application_token:
            pytest.skip("ASTRA_DB_APPLICATION_TOKEN environment variable is required")

        return self.with_plugin_env(
            "langflow",
            {
                "ASTRA_DB_API_ENDPOINT": api_endpoint,
                "ASTRA_DB_APPLICATION_TOKEN": application_token,
            },
        )

    def build(self) -> dict[str, Any]:
        """Build and return the configuration dictionary.

        Returns:
            Configuration dictionary ready for YAML serialization
        """
        import copy

        return copy.deepcopy(self._config)

    def to_yaml(self) -> str:
        """Build configuration and serialize to YAML string.

        Returns:
            YAML string representation of the configuration
        """
        return yaml.dump(self.build(), default_flow_style=False, sort_keys=False)


def create_config_builder() -> StepflowConfigBuilder:
    """Create a new configuration builder for custom test configurations.

    The builder provides a flexible way to create test configurations with
    automatic resource management via context managers:

    Examples:
        # Modern context manager approach (recommended)
        with StepflowConfigBuilder() as config:
            config.add_temp_langflow_database()
            with config.to_temp_yaml() as config_path:
                # Use config_path in tests - automatic cleanup
                pass

        # Multiple temp resources
        with StepflowConfigBuilder() as config:
            config.add_temp_langflow_database()
            config.add_temp_sqlite_state_store()
            config.with_custom_plugin_env(DEBUG="true")
            with config.to_temp_yaml() as config_path:
                # All resources cleaned up automatically
                pass

        # Legacy approach (still supported)
        config = create_config_builder().to_yaml()

        # Legacy with explicit paths
        config = (create_config_builder()
                 .with_langflow_database("/path/to/test.db")
                 .to_yaml())

    Returns:
        New StepflowConfigBuilder instance
    """
    return StepflowConfigBuilder()
