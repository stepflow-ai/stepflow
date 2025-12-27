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
import yaml

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

    def store_flow(self, flow: dict[str, Any], timeout: float = 30.0) -> str | None:
        """Store a flow and return its ID.

        Args:
            flow: Flow definition as a dictionary
            timeout: Request timeout in seconds

        Returns:
            The flow ID if successful, None otherwise
        """
        try:
            response = requests.post(
                f"{self.url}/flows",
                json={"flow": flow},
                timeout=timeout,
            )
            if response.ok:
                return response.json().get("flowId")
            return None
        except Exception:
            return None

    def submit_run(
        self,
        flow_id: str,
        inputs: list[dict[str, Any]],
        max_concurrent: int | None = None,
        timeout: float = 300.0,
    ) -> dict[str, Any]:
        """Submit a run with one or more inputs.

        Args:
            flow_id: ID of the stored flow
            inputs: List of input dictionaries (can be single or batch)
            max_concurrent: Maximum concurrent executions for batch
            timeout: Request timeout in seconds

        Returns:
            Run response with runId, itemCount, status, and optionally result
        """
        run_request: dict[str, Any] = {
            "flowId": flow_id,
            "input": inputs,
        }
        if max_concurrent is not None:
            run_request["maxConcurrency"] = max_concurrent

        response = requests.post(
            f"{self.url}/runs",
            json=run_request,
            timeout=timeout,
        )
        response.raise_for_status()
        return response.json()

    def get_run_items(self, run_id: str, timeout: float = 30.0) -> list[dict[str, Any]]:
        """Get items for a batch run.

        Args:
            run_id: ID of the run
            timeout: Request timeout in seconds

        Returns:
            List of item results
        """
        response = requests.get(
            f"{self.url}/runs/{run_id}/items",
            timeout=timeout,
        )
        response.raise_for_status()
        return response.json().get("items", [])

    def submit_batch(
        self,
        workflow_yaml: str,
        inputs: list[dict[str, Any]],
        max_concurrent: int | None = None,
        timeout: float = 300.0,
    ) -> tuple[bool, dict[str, Any]]:
        """Submit a batch workflow using the API.

        Args:
            workflow_yaml: YAML content of the workflow
            inputs: List of input dictionaries
            max_concurrent: Maximum concurrent executions
            timeout: Request timeout in seconds

        Returns:
            Tuple of (success, batch_stats)
            batch_stats: {"total": N, "completed": N, "failed": N, "results": [...]}
        """
        try:
            # Parse workflow YAML
            flow = yaml.safe_load(workflow_yaml)

            # Store the flow
            flow_id = self.store_flow(flow, timeout=timeout)
            if not flow_id:
                return False, {
                    "total": 0,
                    "completed": 0,
                    "failed": 0,
                    "results": [],
                    "error": "Failed to store flow",
                }

            # Handle empty inputs
            if not inputs:
                return True, {
                    "total": 0,
                    "completed": 0,
                    "failed": 0,
                    "results": [],
                }

            # Submit the run
            run_result = self.submit_run(flow_id, inputs, max_concurrent, timeout)
            run_id = run_result.get("runId")
            item_count = run_result.get("itemCount", 1)

            # Get results
            results: list[dict[str, Any]] = []
            if item_count == 1:
                # Single result in response
                if run_result.get("result"):
                    results = [run_result["result"]]
            else:
                # Fetch from items endpoint
                items = self.get_run_items(run_id, timeout)
                results = [item["result"] for item in items if item.get("result")]

            # Calculate stats
            batch_stats = {
                "total": len(inputs),
                "completed": sum(1 for r in results if r.get("outcome") == "success"),
                "failed": sum(1 for r in results if r.get("outcome") != "success"),
                "results": results,
                "batch_id": run_id,  # Include run ID for backward compatibility
            }

            # Check success
            success = run_result.get("status") in ("success", "completed")
            if not success and batch_stats["completed"] == batch_stats["total"]:
                success = True

            return success, batch_stats

        except requests.RequestException as e:
            return False, {
                "total": 0,
                "completed": 0,
                "failed": 0,
                "results": [],
                "error": str(e),
            }


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

        self.binary_path = Path(binary_path).resolve()
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

        # Log the binary paths for debugging
        print(f"[StepflowBinaryRunner] CLI binary: {self.binary_path}")
        print(f"[StepflowBinaryRunner] Server binary: {self.server_binary_path}")

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
            # Always try to read it, even if returncode != 0
            # (the CLI now exits with code 1 when workflows fail)
            if Path(output_path).exists():
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

        Uses the HTTP API directly for batch execution via server.submit_batch().

        Args:
            server: Running StepflowServer instance
            workflow_yaml: YAML content of the workflow
            inputs_jsonl_path: Path to JSONL file with inputs (one JSON per line)
            max_concurrent: Maximum number of concurrent executions
            output_path: Optional path to write batch results (JSONL format)
            timeout: Command timeout in seconds

        Returns:
            Tuple of (success, batch_stats, stdout, stderr)
            batch_stats: {"total": N, "completed": N, "failed": N, "results": [...]}
        """
        try:
            # Read inputs from JSONL file
            inputs: list[dict[str, Any]] = []
            with open(inputs_jsonl_path) as f:
                for line in f:
                    line = line.strip()
                    if line:
                        inputs.append(json.loads(line))

            # Use server's submit_batch method
            success, batch_stats = server.submit_batch(
                workflow_yaml=workflow_yaml,
                inputs=inputs,
                max_concurrent=max_concurrent,
                timeout=timeout,
            )

            # Write results to output file if specified
            if output_path and batch_stats.get("results"):
                with open(output_path, "w") as f:
                    for result in batch_stats["results"]:
                        f.write(json.dumps(result) + "\n")

            stderr_content = ""
            if not success:
                error = batch_stats.get("error", "")
                server_stderr = server.get_stderr()
                if server_stderr:
                    stderr_content = f"{error}\n--- Server Logs ---\n{server_stderr}"
                else:
                    stderr_content = error

            return success, batch_stats, json.dumps(batch_stats), stderr_content

        except requests.Timeout as e:
            raise ExecutionError(
                f"Stepflow batch submission timed out after {timeout}s"
            ) from e
        except Exception as e:
            return (
                False,
                {"total": 0, "completed": 0, "failed": 0, "results": []},
                "",
                f"Error during batch submission: {e}",
            )

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
