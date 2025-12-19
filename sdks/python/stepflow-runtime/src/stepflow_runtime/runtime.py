# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
# in compliance with the License. You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software distributed under the License
# is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
# or implied. See the License for the specific language governing permissions and limitations under
# the License.

"""Embedded Stepflow runtime with subprocess lifecycle management."""

from __future__ import annotations

import asyncio
import atexit
import logging
import os
import signal
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import TYPE_CHECKING, Any, Callable

import httpx

from stepflow import (
    ComponentInfo,
    FlowResult,
    LogEntry,
    RestartPolicy,
    ValidationResult,
)
from stepflow_client import StepflowClient

from .logging import LogConfig, LogForwarder
from .utils import find_free_port, get_binary_path

if TYPE_CHECKING:
    from types import TracebackType

logger = logging.getLogger(__name__)

# Default config that enables builtin components with a catch-all route
DEFAULT_CONFIG = """\
plugins:
  builtin:
    type: builtin

routes:
  "/{*component}":
    - plugin: builtin
"""


class StepflowRuntimeError(Exception):
    """Error raised by StepflowRuntime operations."""

    pass


class StepflowRuntime:
    """Embedded Stepflow server with subprocess lifecycle management.

    This class manages a stepflow-server subprocess, providing:
    - Automatic subprocess spawning and lifecycle management
    - Signal handling for graceful shutdown
    - Process monitoring with configurable restart policies
    - Log capture and forwarding to Python logging
    - Health check-based startup waiting

    The runtime implements the StepflowExecutor protocol, delegating all
    workflow operations to an internal StepflowClient.

    Example:
        ```python
        # Basic usage with context manager
        async with StepflowRuntime.start() as runtime:
            result = await runtime.run("workflow.yaml", {"x": 1})

        # With configuration
        runtime = StepflowRuntime.start(
            "stepflow-config.yml",
            log_config=LogConfig(level="debug", capture=True),
            restart_policy=RestartPolicy.ON_FAILURE,
        )
        try:
            result = await runtime.run("workflow.yaml", {"x": 1})
        finally:
            runtime.stop()
        ```
    """

    def __init__(
        self,
        *,
        config_path: str | Path | None,
        port: int,
        env: dict[str, str] | None,
        inherit_env: bool,
        log_config: LogConfig,
        restart_policy: RestartPolicy,
        max_restarts: int,
        startup_timeout: float,
        on_crash: Callable[[int], None] | None,
    ) -> None:
        """Initialize the runtime. Use StepflowRuntime.start() instead."""
        self._config_path = Path(config_path) if config_path else None
        self._port = port
        self._env = env or {}
        self._inherit_env = inherit_env
        self._log_config = log_config
        self._restart_policy = restart_policy
        self._max_restarts = max_restarts
        self._startup_timeout = startup_timeout
        self._on_crash = on_crash

        self._process: subprocess.Popen[str] | None = None
        self._client: StepflowClient | None = None
        self._log_forwarder: LogForwarder | None = None
        self._monitor_thread: threading.Thread | None = None
        self._temp_config_file: tempfile.NamedTemporaryFile | None = None

        self._stopping = False
        self._crashed = False
        self._restart_count = 0
        self._lock = threading.Lock()
        self._original_handlers: dict[signal.Signals, Any] = {}

    @classmethod
    def start(
        cls,
        config_path: str | Path | None = None,
        *,
        port: int | None = None,
        env: dict[str, str] | None = None,
        inherit_env: bool = True,
        log_config: LogConfig | None = None,
        restart_policy: RestartPolicy = RestartPolicy.NEVER,
        max_restarts: int = 3,
        startup_timeout: float = 30.0,
        on_crash: Callable[[int], None] | None = None,
    ) -> StepflowRuntime:
        """Start the embedded stepflow server.

        Args:
            config_path: Path to stepflow-config.yml. If None, starts with
                        builtin plugin only (openai, eval, create_messages, etc.)
            port: Port for the server. If None, finds a free port automatically.
            env: Additional environment variables for the subprocess.
            inherit_env: Whether to inherit the parent process environment.
            log_config: Configuration for logging. Defaults to info level with capture.
            restart_policy: When to restart the subprocess on exit.
            max_restarts: Maximum number of restart attempts.
            startup_timeout: Seconds to wait for server to become ready.
            on_crash: Callback invoked when the subprocess crashes.

        Returns:
            A running StepflowRuntime instance.

        Raises:
            StepflowRuntimeError: If the server fails to start.
            FileNotFoundError: If the stepflow-server binary is not found.
            TimeoutError: If the server doesn't become ready within timeout.
        """
        if port is None:
            port = find_free_port()

        if log_config is None:
            log_config = LogConfig()

        runtime = cls(
            config_path=config_path,
            port=port,
            env=env,
            inherit_env=inherit_env,
            log_config=log_config,
            restart_policy=restart_policy,
            max_restarts=max_restarts,
            startup_timeout=startup_timeout,
            on_crash=on_crash,
        )

        runtime._setup_signal_handlers()
        runtime._start_process()
        runtime._wait_for_ready_sync()

        return runtime

    @classmethod
    async def start_async(
        cls,
        config_path: str | Path | None = None,
        *,
        port: int | None = None,
        env: dict[str, str] | None = None,
        inherit_env: bool = True,
        log_config: LogConfig | None = None,
        restart_policy: RestartPolicy = RestartPolicy.NEVER,
        max_restarts: int = 3,
        startup_timeout: float = 30.0,
        on_crash: Callable[[int], None] | None = None,
    ) -> StepflowRuntime:
        """Start the embedded stepflow server asynchronously.

        Same as start() but with async health check waiting.
        """
        if port is None:
            port = find_free_port()

        if log_config is None:
            log_config = LogConfig()

        runtime = cls(
            config_path=config_path,
            port=port,
            env=env,
            inherit_env=inherit_env,
            log_config=log_config,
            restart_policy=restart_policy,
            max_restarts=max_restarts,
            startup_timeout=startup_timeout,
            on_crash=on_crash,
        )

        runtime._setup_signal_handlers()
        runtime._start_process()
        await runtime._wait_for_ready_async()

        return runtime

    @property
    def client(self) -> StepflowClient:
        """Get the HTTP client for this runtime."""
        if self._client is None:
            self._client = StepflowClient(self.url)
        return self._client

    @property
    def url(self) -> str:
        """Base URL of the running server."""
        return f"http://localhost:{self._port}"

    @property
    def port(self) -> int:
        """Port the server is listening on."""
        return self._port

    @property
    def is_alive(self) -> bool:
        """Check if the subprocess is still running."""
        if self._process is None:
            return False
        return self._process.poll() is None

    @property
    def crashed(self) -> bool:
        """Check if the subprocess has crashed and won't be restarted."""
        return self._crashed

    def stop(self, timeout: float = 5.0) -> None:
        """Gracefully stop the server.

        Args:
            timeout: Seconds to wait for graceful shutdown before force killing.
        """
        with self._lock:
            if self._stopping:
                return
            self._stopping = True

        self._restore_signal_handlers()

        # Close the client
        if self._client is not None:
            # StepflowClient might need async close, but we handle sync here
            self._client = None

        # Stop the log forwarder
        if self._log_forwarder is not None:
            self._log_forwarder.stop()

        # Stop the subprocess
        if self._process is not None and self._process.poll() is None:
            logger.info("Stopping stepflow-server...")
            try:
                self._process.terminate()
                try:
                    self._process.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    logger.warning("Server did not stop gracefully, force killing")
                    self._process.kill()
                    self._process.wait(timeout=1.0)
            except Exception as e:
                logger.error(f"Error stopping server: {e}")

        self._process = None
        logger.info("stepflow-server stopped")

    async def stop_async(self, timeout: float = 5.0) -> None:
        """Asynchronously stop the server.

        Args:
            timeout: Seconds to wait for graceful shutdown before force killing.
        """
        # Close the async client properly
        if self._client is not None:
            await self._client.close()
            self._client = None

        # Run the rest synchronously (subprocess operations)
        await asyncio.get_event_loop().run_in_executor(
            None, lambda: self._stop_process(timeout)
        )

    def _stop_process(self, timeout: float) -> None:
        """Stop the subprocess (internal helper)."""
        with self._lock:
            if self._stopping:
                return
            self._stopping = True

        self._restore_signal_handlers()

        if self._log_forwarder is not None:
            self._log_forwarder.stop()

        if self._process is not None and self._process.poll() is None:
            logger.info("Stopping stepflow-server...")
            try:
                self._process.terminate()
                try:
                    self._process.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    logger.warning("Server did not stop gracefully, force killing")
                    self._process.kill()
                    self._process.wait(timeout=1.0)
            except Exception as e:
                logger.error(f"Error stopping server: {e}")

        self._process = None
        logger.info("stepflow-server stopped")

    def get_recent_logs(self, limit: int = 100) -> list[LogEntry]:
        """Get recent log entries from the server.

        Only available when log_config.capture is True.

        Args:
            limit: Maximum number of entries to return.

        Returns:
            List of LogEntry objects, most recent last.
        """
        if self._log_forwarder is None:
            return []
        return self._log_forwarder.get_recent(limit)

    # StepflowExecutor protocol methods - delegate to client

    async def run(
        self,
        flow: str | Path,
        input: dict[str, Any],
        overrides: dict[str, Any] | None = None,
    ) -> FlowResult:
        """Run a workflow and wait for the result."""
        return await self.client.run(flow, input, overrides)

    async def submit(
        self,
        flow: str | Path,
        input: dict[str, Any],
        overrides: dict[str, Any] | None = None,
    ) -> str:
        """Submit a workflow for execution (returns immediately)."""
        return await self.client.submit(flow, input, overrides)

    async def get_result(self, run_id: str) -> FlowResult:
        """Get the result of a workflow run."""
        return await self.client.get_result(run_id)

    async def validate(self, flow: str | Path) -> ValidationResult:
        """Validate a workflow definition."""
        return await self.client.validate(flow)

    async def list_components(self) -> list[ComponentInfo]:
        """List available components."""
        return await self.client.list_components()

    async def health(self) -> dict[str, Any]:
        """Check the health of the server."""
        return await self.client.health()

    async def submit_batch(
        self,
        flow: str | Path,
        inputs: list[dict[str, Any]],
        max_concurrent: int | None = None,
        overrides: dict[str, Any] | None = None,
    ) -> str:
        """Submit a batch of workflow runs."""
        return await self.client.submit_batch(flow, inputs, max_concurrent, overrides)

    async def get_batch(
        self, batch_id: str, include_results: bool = True
    ) -> tuple[dict[str, Any], list[FlowResult] | None]:
        """Get batch execution status and results."""
        return await self.client.get_batch(batch_id, include_results)

    # Context manager support

    def __enter__(self) -> StepflowRuntime:
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        self.stop()

    async def __aenter__(self) -> StepflowRuntime:
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
    ) -> None:
        await self.stop_async()

    # Internal methods

    def _start_process(self) -> None:
        """Start the stepflow-server subprocess."""
        binary_path = get_binary_path()

        # Build command line arguments
        cmd = [
            str(binary_path),
            "--port",
            str(self._port),
        ]

        # Determine config path
        config_path_to_use = self._config_path
        if config_path_to_use is None:
            # Create a temporary config file with default builtin routing
            self._temp_config_file = tempfile.NamedTemporaryFile(
                mode="w",
                suffix=".yml",
                delete=False,  # Don't delete immediately on close
            )
            self._temp_config_file.write(DEFAULT_CONFIG)
            self._temp_config_file.flush()
            config_path_to_use = Path(self._temp_config_file.name)

        cmd.extend(["--config", str(config_path_to_use)])

        # Add log configuration
        cmd.extend(self._log_config.to_cli_args())

        # Build environment
        if self._inherit_env:
            process_env = os.environ.copy()
        else:
            process_env = {}

        process_env.update(self._env)

        logger.info(f"Starting stepflow-server on port {self._port}")
        logger.debug(f"Command: {' '.join(cmd)}")

        try:
            self._process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                env=process_env,
                text=True,
                bufsize=1,  # Line buffered
            )
        except Exception as e:
            raise StepflowRuntimeError(f"Failed to start stepflow-server: {e}") from e

        # Start log forwarder if capturing
        if self._log_config.capture and self._process.stdout:
            self._log_forwarder = LogForwarder(
                pipe=self._process.stdout,
                logger_name=self._log_config.python_logger,
            )
            self._log_forwarder.start()

        # Start monitor thread
        self._start_monitor()

    def _wait_for_ready_sync(self) -> None:
        """Wait for the server to be ready (synchronous)."""
        url = f"{self.url}/api/v1/health"
        start = time.monotonic()

        while time.monotonic() - start < self._startup_timeout:
            # Check if process is still alive
            if self._process and self._process.poll() is not None:
                exit_code = self._process.returncode
                raise StepflowRuntimeError(
                    f"stepflow-server exited during startup with code {exit_code}"
                )

            try:
                with httpx.Client(timeout=1.0) as client:
                    response = client.get(url)
                    if response.status_code == 200:
                        logger.info("stepflow-server is ready")
                        return
            except httpx.ConnectError:
                pass
            except Exception as e:
                logger.debug(f"Health check error: {e}")

            time.sleep(0.1)

        raise TimeoutError(
            f"stepflow-server did not become ready within {self._startup_timeout}s"
        )

    async def _wait_for_ready_async(self) -> None:
        """Wait for the server to be ready (asynchronous)."""
        url = f"{self.url}/api/v1/health"
        start = time.monotonic()

        async with httpx.AsyncClient(timeout=1.0) as client:
            while time.monotonic() - start < self._startup_timeout:
                # Check if process is still alive
                if self._process and self._process.poll() is not None:
                    exit_code = self._process.returncode
                    raise StepflowRuntimeError(
                        f"stepflow-server exited during startup with code {exit_code}"
                    )

                try:
                    response = await client.get(url)
                    if response.status_code == 200:
                        logger.info("stepflow-server is ready")
                        return
                except httpx.ConnectError:
                    pass
                except Exception as e:
                    logger.debug(f"Health check error: {e}")

                await asyncio.sleep(0.1)

        raise TimeoutError(
            f"stepflow-server did not become ready within {self._startup_timeout}s"
        )

    def _setup_signal_handlers(self) -> None:
        """Set up signal handlers for graceful shutdown."""
        # Only set up signal handlers in the main thread
        if threading.current_thread() is not threading.main_thread():
            return

        # Register atexit handler
        atexit.register(self._cleanup)

        # Set up signal handlers (Unix-like systems)
        if sys.platform != "win32":
            for sig in (signal.SIGTERM, signal.SIGINT):
                try:
                    self._original_handlers[sig] = signal.signal(
                        sig, self._signal_handler
                    )
                except (ValueError, OSError):
                    # Can't set signal handler (not main thread, etc.)
                    pass

    def _signal_handler(self, signum: int, frame: Any) -> None:
        """Handle termination signals."""
        logger.info(f"Received signal {signum}, shutting down...")
        self.stop()

        # Restore original handler and re-raise
        sig = signal.Signals(signum)
        original = self._original_handlers.get(sig, signal.SIG_DFL)
        signal.signal(sig, original)
        os.kill(os.getpid(), signum)

    def _restore_signal_handlers(self) -> None:
        """Restore original signal handlers."""
        if threading.current_thread() is not threading.main_thread():
            return

        for sig, handler in self._original_handlers.items():
            try:
                signal.signal(sig, handler)
            except (ValueError, OSError):
                pass
        self._original_handlers.clear()

        # Unregister atexit handler
        try:
            atexit.unregister(self._cleanup)
        except Exception:
            pass

    def _cleanup(self) -> None:
        """Cleanup handler for atexit."""
        if self._process and self._process.poll() is None:
            self.stop()

        # Clean up temporary config file
        if self._temp_config_file:
            try:
                os.unlink(self._temp_config_file.name)
            except OSError:
                pass
            self._temp_config_file = None

    def _start_monitor(self) -> None:
        """Start the process monitor thread."""
        self._monitor_thread = threading.Thread(
            target=self._monitor_loop,
            daemon=True,
            name="stepflow-monitor",
        )
        self._monitor_thread.start()

    def _monitor_loop(self) -> None:
        """Monitor the subprocess and handle crashes."""
        while not self._stopping:
            if self._process is None:
                break

            try:
                exit_code = self._process.wait(timeout=0.5)
            except subprocess.TimeoutExpired:
                continue

            if self._stopping:
                break

            self._handle_crash(exit_code)
            break

    def _handle_crash(self, exit_code: int) -> None:
        """Handle subprocess crash."""
        logger.error(f"stepflow-server exited with code {exit_code}")

        # Call crash callback
        if self._on_crash:
            try:
                self._on_crash(exit_code)
            except Exception:
                logger.exception("on_crash callback failed")

        # Determine if we should restart
        should_restart = (
            self._restart_policy == RestartPolicy.ALWAYS
            or (self._restart_policy == RestartPolicy.ON_FAILURE and exit_code != 0)
        )

        if should_restart and self._restart_count < self._max_restarts:
            self._restart_count += 1
            logger.info(
                f"Restarting stepflow-server (attempt {self._restart_count}/{self._max_restarts})"
            )
            time.sleep(1.0)  # Brief delay before restart

            try:
                self._start_process()
                self._wait_for_ready_sync()
                logger.info("stepflow-server restarted successfully")
            except Exception as e:
                logger.error(f"Failed to restart stepflow-server: {e}")
                self._crashed = True
        else:
            if should_restart:
                logger.error(
                    f"Max restarts ({self._max_restarts}) exceeded, not restarting"
                )
            self._crashed = True
