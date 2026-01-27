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

"""StepflowOrchestrator - Launch and manage the Stepflow server subprocess."""

from __future__ import annotations

import asyncio
import json
import os
import subprocess
import sys
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class OrchestratorConfig:
    """Configuration for StepflowOrchestrator.

    Can be constructed with keyword arguments:
        config = OrchestratorConfig(port=8080, log_level="debug")

    Or by mutating a default instance:
        config = OrchestratorConfig()
        config.port = 8080
        config.config_path = Path("stepflow-config.yml")

    Configuration can be provided in two ways (mutually exclusive):
        - config_path: Path to a stepflow-config.yml file
        - config: A dict representing the config (passed to server via stdin as JSON)
    """

    # Server settings
    port: int = 0  # Auto-assign (0 = random available port)

    # Timeouts (in seconds)
    startup_timeout: float = 30.0
    shutdown_timeout: float = 10.0
    health_check_interval: float = 0.5
    health_check_timeout: float = 5.0

    # Configuration - either path or object (mutually exclusive)
    config_path: Path | None = None  # Path to stepflow-config.yml
    config: dict[str, Any] | None = None  # JSON-serializable config dict

    # Logging
    log_level: str = "info"

    # Environment variables to pass to subprocess
    env: dict[str, str] = field(default_factory=dict)

    def __post_init__(self) -> None:
        """Validate configuration values."""
        if self.port < 0 or self.port > 65535:
            raise ValueError(f"port must be 0-65535, got {self.port}")
        if self.startup_timeout <= 0:
            raise ValueError(
                f"startup_timeout must be positive, got {self.startup_timeout}"
            )
        if self.shutdown_timeout <= 0:
            raise ValueError(
                f"shutdown_timeout must be positive, got {self.shutdown_timeout}"
            )
        valid_levels = ("trace", "debug", "info", "warn", "error")
        if self.log_level not in valid_levels:
            raise ValueError(
                f"log_level must be one of: {', '.join(valid_levels)}; "
                f"got {self.log_level}"
            )
        if self.config_path is not None and self.config is not None:
            raise ValueError(
                "Cannot specify both config_path and config. Use one or the other."
            )


class StepflowOrchestrator:
    """Context manager for running the Stepflow orchestrator as a subprocess.

    Usage with default config:
        async with StepflowOrchestrator.start() as orchestrator:
            print(f"Server running at {orchestrator.url}")
            # Use orchestrator.url with a client library

    Usage with custom config:
        config = OrchestratorConfig(port=8080, log_level="debug")
        async with StepflowOrchestrator.start(config) as orchestrator:
            # orchestrator.url, orchestrator.port, orchestrator.is_running available

    The context manager yields the orchestrator instance for accessing server info.
    The subprocess is automatically started and stopped with the context.
    """

    def __init__(self, config: OrchestratorConfig | None = None):
        self._config = config or OrchestratorConfig()
        self._process: subprocess.Popen[bytes] | None = None
        self._actual_port: int | None = None
        self._url: str | None = None

    @classmethod
    @asynccontextmanager
    async def start(
        cls,
        config: OrchestratorConfig | None = None,
    ) -> AsyncIterator[StepflowOrchestrator]:
        """Start the orchestrator and return it for lifecycle management.

        Args:
            config: Configuration for the orchestrator. Uses defaults if not provided.

        Yields:
            StepflowOrchestrator instance with url, port, and is_running properties.
        """
        orchestrator = cls(config)
        async with orchestrator:
            yield orchestrator

    @property
    def port(self) -> int:
        """Return the actual bound port."""
        if self._actual_port is None:
            raise RuntimeError("Orchestrator not started")
        return self._actual_port

    @property
    def url(self) -> str:
        """Return the base URL of the orchestrator."""
        if self._url is None:
            raise RuntimeError("Orchestrator not started")
        return self._url

    @property
    def is_running(self) -> bool:
        """Check if the orchestrator process is running."""
        return self._process is not None and self._process.poll() is None

    async def __aenter__(self) -> StepflowOrchestrator:
        """Start the orchestrator subprocess."""
        await self._start()
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: Any,
    ) -> None:
        """Stop the orchestrator subprocess."""
        await self._stop()

    async def _start(self) -> None:
        """Start the stepflow-server subprocess."""
        binary_path = self._get_binary_path()

        cmd = [
            str(binary_path),
            "--port",
            str(self._config.port),
            "--log-level",
            self._config.log_level,
        ]

        # Determine stdin input for config
        stdin_input: bytes | None = None

        if self._config.config_path:
            cmd.extend(["--config", str(self._config.config_path)])
        elif self._config.config is not None:
            cmd.append("--config-stdin")
            # Serialize config dict to JSON
            # Note: We need to remove None values because Rust's serde untagged
            # enum deserialization can fail with explicit null values for
            # optional fields (e.g., StepflowTransport).
            cleaned = self._remove_none_values(self._config.config)
            stdin_input = json.dumps(cleaned).encode("utf-8")

        env = os.environ.copy()
        env.update(self._config.env)

        self._process = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE if stdin_input else None,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )

        # Write config to stdin if provided
        if stdin_input and self._process.stdin:
            self._process.stdin.write(stdin_input)
            self._process.stdin.close()

        # Read the port announcement from stdout
        await self._read_port_announcement()

        # Wait for health check
        await self._wait_for_health()

        # Server binds to localhost/0.0.0.0, use localhost for client connections
        self._url = f"http://127.0.0.1:{self._actual_port}"

    async def _stop(self) -> None:
        """Stop the subprocess gracefully."""
        if self._process is None:
            return

        # Send SIGTERM
        self._process.terminate()

        try:
            # Wait for graceful shutdown
            loop = asyncio.get_event_loop()
            await asyncio.wait_for(
                loop.run_in_executor(None, self._process.wait),
                timeout=self._config.shutdown_timeout,
            )
        except TimeoutError:
            # Force kill if graceful shutdown fails
            self._process.kill()
            self._process.wait()

        self._process = None
        self._actual_port = None
        self._url = None

    @staticmethod
    def _remove_none_values(obj: Any) -> Any:
        """Recursively remove None values from a dict or list.

        This is needed because Rust's serde untagged enum deserialization
        can fail when explicit null values are present for optional fields.
        """
        if isinstance(obj, dict):
            return {
                k: StepflowOrchestrator._remove_none_values(v)
                for k, v in obj.items()
                if v is not None
            }
        elif isinstance(obj, list):
            return [StepflowOrchestrator._remove_none_values(item) for item in obj]
        return obj

    def _get_binary_path(self) -> Path:
        """Get the path to the stepflow-server binary.

        Resolution order:
        1. STEPFLOW_DEV_BINARY environment variable (for development)
        2. Bundled binary in package bin/ directory (for production)

        Raises:
            FileNotFoundError: If no binary is found.
        """
        # Check for development mode (explicit path)
        dev_path = os.environ.get("STEPFLOW_DEV_BINARY")
        if dev_path:
            dev_binary = Path(dev_path)
            if not dev_binary.exists():
                raise FileNotFoundError(
                    f"STEPFLOW_DEV_BINARY set but file not found: {dev_binary}"
                )
            return dev_binary

        # Use bundled binary from package
        package_dir = Path(__file__).parent
        binary_name = (
            "stepflow-server.exe" if sys.platform == "win32" else "stepflow-server"
        )
        bundled_binary = package_dir / "bin" / binary_name

        if bundled_binary.exists():
            return bundled_binary

        raise FileNotFoundError(
            f"stepflow-server binary not found.\n"
            f"Expected bundled binary at: {bundled_binary}\n"
            f"For development, set STEPFLOW_DEV_BINARY=/path/to/stepflow-server"
        )

    async def _read_port_announcement(self) -> None:
        """Read the port announcement from stdout.

        The server may emit log lines before the port announcement.
        We read lines until we find {"port": N}.
        """
        assert self._process is not None
        assert self._process.stdout is not None

        loop = asyncio.get_event_loop()
        start_time = loop.time()

        while True:
            # Check timeout
            if loop.time() - start_time > self._config.startup_timeout:
                raise TimeoutError(
                    f"Timed out waiting for port announcement after "
                    f"{self._config.startup_timeout}s"
                )

            line = await loop.run_in_executor(None, self._process.stdout.readline)

            if not line:
                # Check if process died
                if self._process.poll() is not None:
                    stderr_output = ""
                    if self._process.stderr:
                        stderr_output = self._process.stderr.read().decode("utf-8")
                    raise RuntimeError(
                        f"Process exited without announcing port. "
                        f"Exit code: {self._process.returncode}. "
                        f"Stderr: {stderr_output}"
                    )
                raise RuntimeError("Process exited without announcing port")

            # Try to parse as JSON port announcement
            line_str = line.decode("utf-8").strip()
            if line_str.startswith("{") and "port" in line_str:
                try:
                    data = json.loads(line_str)
                    if "port" in data:
                        self._actual_port = data["port"]
                        return
                except json.JSONDecodeError:
                    pass  # Not valid JSON, continue reading

    async def _wait_for_health(self) -> None:
        """Wait for the health endpoint to respond."""
        import urllib.error
        import urllib.request

        health_url = f"http://127.0.0.1:{self._actual_port}/api/v1/health"
        loop = asyncio.get_event_loop()
        start_time = loop.time()

        while True:
            elapsed = loop.time() - start_time
            if elapsed > self._config.startup_timeout:
                raise TimeoutError(
                    f"Orchestrator failed to become healthy within "
                    f"{self._config.startup_timeout}s"
                )

            # Check if process is still running
            if self._process and self._process.poll() is not None:
                stderr_output = ""
                if self._process.stderr:
                    stderr_output = self._process.stderr.read().decode("utf-8")
                raise RuntimeError(
                    f"Process died during startup. "
                    f"Exit code: {self._process.returncode}. "
                    f"Stderr: {stderr_output}"
                )

            try:
                req = urllib.request.Request(health_url, method="GET")
                timeout = self._config.health_check_timeout

                def do_request(
                    r: urllib.request.Request = req, t: float = timeout
                ) -> bool:
                    with urllib.request.urlopen(r, timeout=t):
                        return True

                await loop.run_in_executor(None, do_request)
                return  # Health check passed
            except (urllib.error.URLError, TimeoutError, OSError):
                await asyncio.sleep(self._config.health_check_interval)
