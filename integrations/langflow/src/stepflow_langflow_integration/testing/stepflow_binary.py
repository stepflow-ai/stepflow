"""Integration with Stepflow binary for validation and execution testing."""

import json
import os
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Dict, Any, Optional, List, Tuple
import yaml

from ..utils.errors import ValidationError, ExecutionError


class StepflowBinaryRunner:
    """Helper class for running Stepflow binary commands in tests."""
    
    def __init__(self, binary_path: Optional[str] = None):
        """Initialize with Stepflow binary path.
        
        Args:
            binary_path: Path to stepflow binary. If None, uses STEPFLOW_BINARY_PATH 
                        environment variable or defaults to stepflow-rs/target/debug/stepflow
        """
        if binary_path is None:
            # Try environment variable first
            binary_path = os.environ.get('STEPFLOW_BINARY_PATH')
            
            if binary_path is None:
                # Default to relative path from integrations/langflow to stepflow-rs
                # We're at: stepflow-rs/integrations/langflow/src/stepflow_langflow_integration/testing/stepflow_binary.py
                # We need: stepflow-rs/stepflow-rs/target/debug/stepflow
                current_dir = Path(__file__).parent.parent.parent.parent.parent
                binary_path = current_dir / "stepflow-rs" / "target" / "debug" / "stepflow"
        
        self.binary_path = Path(binary_path)
        if not self.binary_path.exists():
            raise FileNotFoundError(
                f"Stepflow binary not found at {self.binary_path}. "
                f"Set STEPFLOW_BINARY_PATH environment variable or build stepflow-rs first."
            )
    
    def validate_workflow(
        self, 
        workflow_yaml: str, 
        config_path: Optional[str] = None,
        timeout: float = 30.0
    ) -> Tuple[bool, str, str]:
        """Validate a workflow using stepflow validate command.
        
        Args:
            workflow_yaml: YAML content of the workflow
            config_path: Path to stepflow config file (optional)
            timeout: Command timeout in seconds
            
        Returns:
            Tuple of (success, stdout, stderr)
        """
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
            f.write(workflow_yaml)
            workflow_path = f.name
        
        try:
            cmd = [str(self.binary_path), "validate", "--flow", workflow_path]
            if config_path:
                cmd.extend(["--config", config_path])
            
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=self.binary_path.parent
            )
            
            return result.returncode == 0, result.stdout, result.stderr
            
        except subprocess.TimeoutExpired as e:
            raise ValidationError(f"Stepflow validate command timed out after {timeout}s") from e
        finally:
            # Clean up temp file
            Path(workflow_path).unlink(missing_ok=True)
    
    def run_workflow(
        self,
        workflow_yaml: str,
        input_data: Dict[str, Any],
        config_path: Optional[str] = None,
        timeout: float = 60.0,
        input_format: str = "json"
    ) -> Tuple[bool, Optional[Dict[str, Any]], str, str]:
        """Run a workflow using stepflow run command.
        
        Args:
            workflow_yaml: YAML content of the workflow
            input_data: Input data for the workflow
            config_path: Path to stepflow config file (optional)
            timeout: Command timeout in seconds
            input_format: Input format ("json" or "yaml")
            
        Returns:
            Tuple of (success, result_data, stdout, stderr)
        """
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
            f.write(workflow_yaml)
            workflow_path = f.name
        
        try:
            cmd = [
                str(self.binary_path), "run", 
                "--flow", workflow_path,
                f"--stdin-format={input_format}"
            ]
            
            if config_path:
                cmd.extend(["--config", config_path])
            
            # Pass input via stdin
            input_str = json.dumps(input_data) if input_format == "json" else yaml.dump(input_data)
            
            result = subprocess.run(
                cmd,
                input=input_str,
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=self.binary_path.parent
            )
            
            success = result.returncode == 0
            result_data = None
            
            if result.stdout.strip():
                try:
                    # Try to parse JSON output - look for JSON on the last line
                    lines = result.stdout.strip().split('\n')
                    json_line = None
                    for line in reversed(lines):
                        line = line.strip()
                        if line.startswith('{') and line.endswith('}'):
                            json_line = line
                            break
                    
                    if json_line:
                        result_data = json.loads(json_line)
                        # Check if workflow execution actually succeeded
                        if isinstance(result_data, dict) and result_data.get("outcome") == "failed":
                            success = False
                    else:
                        # No JSON found, treat as raw output
                        if success:
                            result_data = {"output": result.stdout.strip()}
                        
                except json.JSONDecodeError:
                    # If not JSON, treat as raw output
                    if success:
                        result_data = {"output": result.stdout.strip()}
            
            return success, result_data, result.stdout, result.stderr
            
        except subprocess.TimeoutExpired as e:
            raise ExecutionError(f"Stepflow run command timed out after {timeout}s") from e
        finally:
            # Clean up temp file  
            Path(workflow_path).unlink(missing_ok=True)
    
    def run_workflow_with_file_input(
        self,
        workflow_yaml: str,
        input_file_path: str,
        config_path: Optional[str] = None,
        timeout: float = 60.0
    ) -> Tuple[bool, Optional[Dict[str, Any]], str, str]:
        """Run a workflow with input from file.
        
        Args:
            workflow_yaml: YAML content of the workflow
            input_file_path: Path to input JSON file
            config_path: Path to stepflow config file (optional)
            timeout: Command timeout in seconds
            
        Returns:
            Tuple of (success, result_data, stdout, stderr)
        """
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
            f.write(workflow_yaml)
            workflow_path = f.name
        
        try:
            cmd = [
                str(self.binary_path), "run",
                "--flow", workflow_path,
                "--input", input_file_path
            ]
            
            if config_path:
                cmd.extend(["--config", config_path])
            
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=self.binary_path.parent
            )
            
            success = result.returncode == 0
            result_data = None
            
            if success and result.stdout.strip():
                try:
                    result_data = json.loads(result.stdout)
                except json.JSONDecodeError:
                    result_data = {"output": result.stdout.strip()}
            
            return success, result_data, result.stdout, result.stderr
            
        except subprocess.TimeoutExpired as e:
            raise ExecutionError(f"Stepflow run command timed out after {timeout}s") from e
        finally:
            Path(workflow_path).unlink(missing_ok=True)
    
    def check_binary_availability(self) -> Tuple[bool, str]:
        """Check if stepflow binary is available and working.
        
        Returns:
            Tuple of (available, version_info)
        """
        try:
            result = subprocess.run(
                [str(self.binary_path), "--version"],
                capture_output=True,
                text=True,
                timeout=10.0
            )
            
            if result.returncode == 0:
                return True, result.stdout.strip()
            else:
                return False, result.stderr.strip()
                
        except (subprocess.TimeoutExpired, FileNotFoundError) as e:
            return False, str(e)
    
    def list_components(self, config_path: Optional[str] = None) -> Tuple[bool, List[str], str]:
        """List available components using stepflow list-components.
        
        Args:
            config_path: Path to stepflow config file (optional)
            
        Returns:
            Tuple of (success, component_list, stderr)
        """
        try:
            cmd = [str(self.binary_path), "list-components"]
            if config_path:
                cmd.extend(["--config", config_path])
            
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30.0,
                cwd=self.binary_path.parent
            )
            
            success = result.returncode == 0
            components = []
            
            if success and result.stdout.strip():
                # Parse component list from output
                lines = result.stdout.strip().split('\n')
                components = [line.strip() for line in lines if line.strip()]
            
            return success, components, result.stderr
            
        except subprocess.TimeoutExpired as e:
            raise ExecutionError(f"Stepflow list-components timed out") from e


def get_default_stepflow_config() -> str:
    """Get a default stepflow config for testing Langflow integration.
    
    Returns:
        YAML content for stepflow config
    """
    # Get path to mock langflow server relative to this file
    current_dir = Path(__file__).parent.parent.parent.parent
    mock_server_path = current_dir / "tests" / "mock_langflow_server.py"
    
    return f"""
plugins:
  builtin:
    type: builtin
  mock_langflow:
    type: stepflow
    transport: stdio
    command: python3
    args: ["{mock_server_path}"]

routes:
  "/langflow/{{*component}}":
    - plugin: mock_langflow
  "/builtin/{{*component}}":
    - plugin: builtin

stateStore:
  type: inMemory
"""


def create_test_config_file(config_content: str) -> str:
    """Create a temporary config file for testing.
    
    Args:
        config_content: YAML content for the config
        
    Returns:
        Path to created config file (caller should clean up)
    """
    with tempfile.NamedTemporaryFile(mode='w', suffix='.yml', delete=False) as f:
        f.write(config_content)
        return f.name