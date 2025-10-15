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

"""Integration with Stepflow binary for validation and execution testing."""

import json
import os
import subprocess
import tempfile
import threading
import time
from io import StringIO
from pathlib import Path
from typing import Any

import requests

from stepflow_langflow_integration.exceptions import ExecutionError, ValidationError


class StepflowServer:
    """Running Stepflow server instance with lifecycle management.

    This class manages a Stepflow server process and provides methods to
    interact with it and retrieve stdout/stderr for debugging.
    """

    def __init__(
        self,
        process: subprocess.Popen,
        url: str,
        port: int,
        config_path: str,
    ):
        """Initialize StepflowServer instance.

        Args:
            process: Running server subprocess
            url: Base URL for API calls (e.g., "http://localhost:7837/api/v1")
            port: Server port number
            config_path: Path to config file used by server
        """
        self.process = process
        self.url = url
        self.port = port
        self.config_path = config_path
        self._stdout_buffer = StringIO()
        self._stderr_buffer = StringIO()
        self._stdout_thread: threading.Thread | None = None
        self._stderr_thread: threading.Thread | None = None
        self._stop_capture = threading.Event()

        # Start capturing stdout/stderr in background threads
        self._start_output_capture()

    def _start_output_capture(self):
        """Start background threads to capture stdout and stderr."""

        def capture_stream(stream, buffer: StringIO, stop_event: threading.Event):
            """Capture stream output to buffer until stop event is set."""
            try:
                for line in iter(stream.readline, ""):
                    if stop_event.is_set():
                        break
                    buffer.write(line)
            except Exception:
                pass  # Ignore errors during capture

        if self.process.stdout:
            self._stdout_thread = threading.Thread(
                target=capture_stream,
                args=(self.process.stdout, self._stdout_buffer, self._stop_capture),
                daemon=True,
            )
            self._stdout_thread.start()

        if self.process.stderr:
            self._stderr_thread = threading.Thread(
                target=capture_stream,
                args=(self.process.stderr, self._stderr_buffer, self._stop_capture),
                daemon=True,
            )
            self._stderr_thread.start()

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, *args):
        """Context manager exit - ensures server is stopped."""
        self.stop()

    def stop(self):
        """Stop the server and wait for cleanup."""
        if self.process and self.process.poll() is None:
            # Stop output capture first
            self._stop_capture.set()

            # Give threads a moment to finish
            if self._stdout_thread:
                self._stdout_thread.join(timeout=1.0)
            if self._stderr_thread:
                self._stderr_thread.join(timeout=1.0)

            # Terminate server gracefully
            try:
                self.process.terminate()
                self.process.wait(timeout=5.0)
            except subprocess.TimeoutExpired:
                # Force kill if graceful shutdown fails
                self.process.kill()
                self.process.wait(timeout=2.0)

    def get_stdout(self) -> str:
        """Get buffered stdout for debugging.

        Returns:
            All stdout output captured since server start
        """
        return self._stdout_buffer.getvalue()

    def get_stderr(self) -> str:
        """Get buffered stderr for debugging.

        Returns:
            All stderr output captured since server start
        """
        return self._stderr_buffer.getvalue()

    def is_healthy(self) -> bool:
        """Check if server is healthy via health endpoint.

        Returns:
            True if server responds to health check
        """
        try:
            # Health endpoint is at /api/v1/health
            health_url = f"{self.url}/health"
            response = requests.get(health_url, timeout=2.0)
            return response.status_code == 200
        except Exception:
            return False


class StepflowBinaryRunner:
    """Helper class for running Stepflow binary commands in tests."""

    def __init__(self, binary_path: str | None = None):
        """Initialize with Stepflow binary path.

        Args:
            binary_path: Path to stepflow binary. If None, uses STEPFLOW_BINARY_PATH
                          environment variable or defaults to
                          stepflow-rs/target/debug/stepflow
        """
        if binary_path is None:
            # Try environment variable first
            binary_path = os.environ.get("STEPFLOW_BINARY_PATH")

            if binary_path is None:
                # Default to relative path from integrations/langflow to stepflow-rs
                # Go up 6 levels from stepflow_binary.py to reach project root
                current_dir = Path(__file__).parent.parent.parent.parent.parent.parent
                default_path = (
                    current_dir / "stepflow-rs" / "target" / "debug" / "stepflow"
                )
                binary_path = str(default_path)

        self.binary_path = Path(binary_path)
        if not self.binary_path.exists():
            raise FileNotFoundError(
                f"Stepflow binary not found at {self.binary_path}. "
                f"Set STEPFLOW_BINARY_PATH environment variable or "
                "build stepflow-rs first."
            )

        # Also locate the stepflow-server binary (in the same directory)
        self.server_binary_path = self.binary_path.parent / "stepflow-server"
        if not self.server_binary_path.exists():
            raise FileNotFoundError(
                f"Stepflow server binary not found at {self.server_binary_path}. "
                "Please build stepflow-server first."
            )

    def start_server(
        self,
        config_path: str,
        port: int | None = None,
        timeout: float = 30.0,
    ) -> StepflowServer:
        """Start a Stepflow server and return server instance.

        Args:
            config_path: Path to stepflow config file
            port: Server port (if None, finds available port automatically)
            timeout: Timeout for server startup health check

        Returns:
            StepflowServer instance for interacting with the running server

        Raises:
            ExecutionError: If server fails to start or doesn't become healthy
        """
        import socket

        # Find available port if not specified
        if port is None:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.bind(("", 0))
                s.listen(1)
                port = s.getsockname()[1]

        url = f"http://localhost:{port}/api/v1"

        # Start server process using stepflow-server binary
        cmd = [
            str(self.server_binary_path),
            "--port",
            str(port),
            "--config",
            config_path,
        ]

        try:
            process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                cwd=self.binary_path.parent,
            )
        except Exception as e:
            raise ExecutionError(f"Failed to start Stepflow server: {e}") from e

        # Create server instance
        server = StepflowServer(process, url, port, config_path)

        # Wait for server to become healthy
        start_time = time.time()
        while time.time() - start_time < timeout:
            if server.is_healthy():
                return server

            # Check if process died
            if process.poll() is not None:
                stdout = server.get_stdout()
                stderr = server.get_stderr()
                raise ExecutionError(
                    f"Stepflow server process exited unexpectedly.\n"
                    f"Exit code: {process.returncode}\n"
                    f"STDOUT:\n{stdout}\n"
                    f"STDERR:\n{stderr}"
                )

            time.sleep(0.5)

        # Timeout - cleanup and raise error
        stdout = server.get_stdout()
        stderr = server.get_stderr()
        server.stop()
        raise ExecutionError(
            f"Stepflow server failed to become healthy within {timeout}s.\n"
            f"STDOUT:\n{stdout}\n"
            f"STDERR:\n{stderr}"
        )

    def submit_workflow(
        self,
        server: StepflowServer,
        workflow_yaml: str,
        input_data: dict[str, Any],
        timeout: float = 60.0,
    ) -> tuple[bool, dict[str, Any] | None, str, str]:
        """Submit workflow to running Stepflow server.

        Args:
            server: Running StepflowServer instance
            workflow_yaml: YAML content of the workflow
            input_data: Input data for the workflow
            timeout: Command timeout in seconds

        Returns:
            Tuple of (success, result_data, stdout, stderr)

        Raises:
            ExecutionError: If submission fails
        """
        # Write workflow to temporary file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".yaml", delete=False
        ) as workflow_file:
            workflow_file.write(workflow_yaml)
            workflow_path = workflow_file.name

        # Create temporary output file for clean result parsing
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False
        ) as output_file:
            output_path = output_file.name

        try:
            # Use stepflow submit command
            cmd = [
                str(self.binary_path),
                "submit",
                "--url",
                server.url,
                "--flow",
                workflow_path,
                "--stdin-format=json",
                "--output",
                output_path,
            ]

            # Pass input via stdin
            input_str = json.dumps(input_data)

            result = subprocess.run(
                cmd,
                input=input_str,
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=self.binary_path.parent,
            )

            success = result.returncode == 0
            result_data = None
            stdout_content = result.stdout
            stderr_content = result.stderr

            # Read the clean JSON output from the output file
            if success and Path(output_path).exists():
                try:
                    with open(output_path) as f:
                        output_content = f.read().strip()
                    if output_content:
                        result_data = json.loads(output_content)
                except (json.JSONDecodeError, FileNotFoundError):
                    # If output file parsing fails, there's nothing more to do
                    pass

            # Check if workflow execution actually succeeded based on outcome
            if isinstance(result_data, dict) and result_data.get("outcome") == "failed":
                success = False

            # If submission failed, include server logs in stderr for debugging
            if not success:
                server_stderr = server.get_stderr()
                if server_stderr:
                    stderr_content = "\n".join(
                        (stderr_content, "--- Server Logs ---", server_stderr)
                    ).strip()

            return success, result_data, stdout_content, stderr_content

        except subprocess.TimeoutExpired as e:
            raise ExecutionError(
                f"Stepflow submit command timed out after {timeout}s"
            ) from e
        finally:
            # Clean up temp files
            for temp_path in [workflow_path, output_path]:
                Path(temp_path).unlink(missing_ok=True)

    def submit_batch(
        self,
        server: StepflowServer,
        workflow_yaml: str,
        inputs_jsonl_path: str,
        max_concurrent: int | None = None,
        output_path: str | None = None,
        timeout: float = 300.0,
    ) -> tuple[bool, dict[str, Any], str, str]:
        """Submit batch workflow to Stepflow server.

        Args:
            server: Running StepflowServer instance
            workflow_yaml: YAML content of the workflow
            inputs_jsonl_path: Path to JSONL file with inputs (one JSON per line)
            max_concurrent: Maximum number of concurrent executions
            output_path: Optional path to write batch results (JSONL format)
            timeout: Command timeout in seconds

        Returns:
            Tuple of (success, batch_stats, stdout, stderr)
            batch_stats: {"total": N, "completed": N, "failed": N, "batch_id": uuid}
        """
        # Write workflow to temporary file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".yaml", delete=False
        ) as workflow_file:
            workflow_file.write(workflow_yaml)
            workflow_path = workflow_file.name

        try:
            # Build command
            cmd = [
                str(self.binary_path),
                "submit-batch",
                "--url",
                server.url,
                "--flow",
                workflow_path,
                "--inputs",
                inputs_jsonl_path,
            ]

            if max_concurrent is not None:
                cmd.extend(["--max-concurrent", str(max_concurrent)])

            if output_path is not None:
                cmd.extend(["--output", output_path])

            # Run batch submission
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=self.binary_path.parent,
            )

            success = result.returncode == 0
            stdout_content = result.stdout
            stderr_content = result.stderr

            # Parse batch statistics from output
            batch_stats: dict[str, Any] = {
                "total": 0,
                "completed": 0,
                "failed": 0,
                "batch_id": None,
            }

            # Try to extract statistics from stdout
            if stdout_content:
                lines = stdout_content.split("\n")
                for line in lines:
                    if "Batch ID:" in line:
                        # Extract batch ID
                        parts = line.split("Batch ID:")
                        if len(parts) > 1:
                            batch_stats["batch_id"] = parts[1].strip()
                    elif "Total:" in line:
                        # Extract total count
                        parts = line.split("Total:")
                        if len(parts) > 1:
                            try:
                                batch_stats["total"] = int(parts[1].strip())
                            except ValueError:
                                pass
                    elif "Completed:" in line or "✅ Completed:" in line:
                        # Extract completed count
                        parts = line.split("Completed:")
                        if len(parts) > 1:
                            try:
                                batch_stats["completed"] = int(parts[1].strip())
                            except ValueError:
                                pass
                    elif "Failed:" in line or "❌ Failed:" in line:
                        # Extract failed count
                        parts = line.split("Failed:")
                        if len(parts) > 1:
                            try:
                                batch_stats["failed"] = int(parts[1].strip())
                            except ValueError:
                                pass

            # If submission failed, include server logs in stderr
            if not success:
                server_stderr = server.get_stderr()
                if server_stderr:
                    stderr_content = "\n".join(
                        (stderr_content, "--- Server Logs ---", server_stderr)
                    ).strip()

            return success, batch_stats, stdout_content, stderr_content

        except subprocess.TimeoutExpired as e:
            raise ExecutionError(
                f"Stepflow submit-batch command timed out after {timeout}s"
            ) from e
        finally:
            # Clean up temp workflow file
            Path(workflow_path).unlink(missing_ok=True)

    def validate_workflow(
        self,
        workflow_yaml: str,
        config_path: str,
        timeout: float = 30.0,
    ) -> tuple[bool, str, str]:
        """Validate a workflow using stepflow validate command.

        Args:
            workflow_yaml: YAML content of the workflow
            config_path: Path to stepflow config file
            timeout: Command timeout in seconds

        Returns:
            Tuple of (success, stdout, stderr)
        """

        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
            f.write(workflow_yaml)
            workflow_path = f.name

        try:
            cmd = [
                str(self.binary_path),
                "validate",
                "--flow",
                workflow_path,
                "--config",
                config_path,
            ]

            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=self.binary_path.parent,
            )

            return result.returncode == 0, result.stdout, result.stderr

        except subprocess.TimeoutExpired as e:
            raise ValidationError(
                f"Stepflow validate command timed out after {timeout}s"
            ) from e
        finally:
            # Clean up temp file
            Path(workflow_path).unlink(missing_ok=True)

    def check_binary_availability(self) -> tuple[bool, str]:
        """Check if stepflow binary is available and working.

        Returns:
            Tuple of (available, version_info)
        """
        try:
            result = subprocess.run(
                [str(self.binary_path), "--version"],
                capture_output=True,
                text=True,
                timeout=10.0,
            )

            if result.returncode == 0:
                return True, result.stdout.strip()
            else:
                return False, result.stderr.strip()

        except (subprocess.TimeoutExpired, FileNotFoundError) as e:
            return False, str(e)

    def list_components(self, config_path: str) -> tuple[bool, list[str], str]:
        """List available components using stepflow list-components.

        Args:
            config_path: Path to stepflow config file

        Returns:
            Tuple of (success, component_list, stderr)
        """
        try:
            cmd = [str(self.binary_path), "list-components", "--config", config_path]

            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30.0,
                cwd=self.binary_path.parent,
            )

            success = result.returncode == 0
            components = []

            if success and result.stdout.strip():
                # Parse component list from output
                lines = result.stdout.strip().split("\n")
                components = [line.strip() for line in lines if line.strip()]

            return success, components, result.stderr

        except subprocess.TimeoutExpired as e:
            raise ExecutionError("Stepflow list-components timed out") from e
