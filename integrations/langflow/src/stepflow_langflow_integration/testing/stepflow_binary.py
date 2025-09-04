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
from pathlib import Path
from typing import Any

import yaml

from ..utils.errors import ExecutionError, ValidationError


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

    def _extract_json_result(self, stdout: str) -> dict[str, Any] | None:
        """Legacy JSON extraction fallback for when --output file parsing fails.

        This is a simplified fallback that tries basic JSON extraction from
        mixed stdout output. The primary approach now uses --output files.

        Args:
            stdout: Raw stdout from stepflow command

        Returns:
            Parsed JSON data as dict, or {"output": raw_text} fallback
        """
        if not stdout.strip():
            return {"output": ""}

        lines = stdout.strip().split("\n")

        # Look for complete JSON objects on individual lines (most reliable)
        for line in reversed(lines):
            line = line.strip()
            if line.startswith("{") and line.endswith("}"):
                try:
                    parsed = json.loads(line)
                    if isinstance(parsed, dict):
                        return parsed
                except json.JSONDecodeError:
                    continue

        # Fallback - return raw output
        return {"output": stdout.strip()}

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

    def run_workflow(
        self,
        workflow_yaml: str,
        input_data: dict[str, Any],
        config_path: str,
        timeout: float = 60.0,
        input_format: str = "json",
    ) -> tuple[bool, dict[str, Any] | None, str, str]:
        """Run a workflow using stepflow run command with clean output separation.

        Uses stepflow's built-in --output and --log-file options to separate
        result JSON from log output, eliminating the need for complex parsing.

        Args:
            workflow_yaml: YAML content of the workflow
            input_data: Input data for the workflow
            config_path: Path to stepflow config file
            timeout: Command timeout in seconds
            input_format: Input format ("json" or "yaml")

        Returns:
            Tuple of (success, result_data, stdout, stderr)
            - stdout contains clean JSON result
            - stderr contains both original stderr and logs for debugging
        """

        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
            f.write(workflow_yaml)
            workflow_path = f.name

        # Create temporary files for clean output separation
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False
        ) as output_file:
            output_path = output_file.name
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".log", delete=False
        ) as log_file:
            log_path = log_file.name

        try:
            cmd = [
                str(self.binary_path),
                "run",
                "--flow",
                workflow_path,
                "--config",
                config_path,
                f"--stdin-format={input_format}",
                "--output",
                output_path,
                "--log-file",
                log_path,
            ]

            # Pass input via stdin
            input_str = (
                json.dumps(input_data)
                if input_format == "json"
                else yaml.dump(input_data)
            )

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
            stdout_content = ""
            stderr_content = result.stderr

            # Read the clean JSON output from the output file
            if success and Path(output_path).exists():
                try:
                    with open(output_path) as f:
                        output_content = f.read().strip()
                    if output_content:
                        result_data = json.loads(output_content)
                        stdout_content = output_content  # For compatibility
                except (json.JSONDecodeError, FileNotFoundError):
                    # If output file parsing fails, fall back to stdout parsing
                    if result.stdout.strip():
                        result_data = self._extract_json_result(result.stdout)
                        stdout_content = result.stdout

            # Read logs from log file and append to stderr for debugging
            if Path(log_path).exists():
                try:
                    with open(log_path) as f:
                        log_content = f.read().strip()
                    if log_content:
                        # Add logs to stderr for debugging (keeping original stderr too)
                        stderr_content = (
                            f"{stderr_content}\n--- Logs ---\n{log_content}".strip()
                        )
                except FileNotFoundError:
                    pass

            # Check if workflow execution actually succeeded based on outcome
            if (
                isinstance(result_data, dict)
                and result_data.get("outcome") == "failed"
            ):
                success = False

            return success, result_data, stdout_content, stderr_content

        except subprocess.TimeoutExpired as e:
            raise ExecutionError(
                f"Stepflow run command timed out after {timeout}s"
            ) from e
        finally:
            # Clean up temp files
            for temp_path in [workflow_path, output_path, log_path]:
                Path(temp_path).unlink(missing_ok=True)

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
