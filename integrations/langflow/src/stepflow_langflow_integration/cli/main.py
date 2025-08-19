# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

"""Main CLI entry point for Langflow integration."""

import json
import sys
from pathlib import Path
from typing import Optional

import click

from ..converter.translator import LangflowConverter
from ..executor.langflow_server import StepflowLangflowServer
from ..utils.errors import ConversionError, ValidationError


@click.group()
@click.version_option()
def main():
    """Stepflow Langflow Integration CLI."""
    pass


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
@click.argument("output_file", type=click.Path(path_type=Path), required=False)
@click.option("--pretty", is_flag=True, help="Pretty-print the output YAML")
@click.option("--validate", is_flag=True, help="Validate schemas during conversion")
def convert(
    input_file: Path, 
    output_file: Optional[Path],
    pretty: bool,
    validate: bool
):
    """Convert a Langflow JSON workflow to Stepflow YAML.
    
    If no output file is specified, prints the YAML to stdout.
    """
    try:
        converter = LangflowConverter(validate_schemas=validate)
        stepflow_yaml = converter.convert_file(input_file)
        
        if output_file:
            # Write to file
            with open(output_file, "w", encoding="utf-8") as f:
                f.write(stepflow_yaml)
            click.echo(f"✅ Successfully converted {input_file} to {output_file}", err=True)
        else:
            # Write to stdout
            click.echo(stepflow_yaml)
        
    except (ConversionError, ValidationError) as e:
        click.echo(f"❌ Conversion failed: {e}", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
def analyze(input_file: Path):
    """Analyze a Langflow workflow structure.
    
    This command provides detailed analysis of a Langflow JSON workflow without converting it.
    It examines the workflow structure, component types, dependencies, and identifies potential
    issues that might affect conversion. Useful for understanding workflow complexity and 
    debugging conversion problems before attempting the full conversion process.
    """
    try:
        converter = LangflowConverter()
        
        # Load and analyze
        with open(input_file, "r", encoding="utf-8") as f:
            langflow_data = json.load(f)
        
        analysis = converter.analyze(langflow_data)
        
        # Display results
        click.echo(f"📊 Analysis of {input_file}:")
        click.echo(f"  • Nodes: {analysis['node_count']}")
        click.echo(f"  • Edges: {analysis['edge_count']}")
        
        click.echo("\n📦 Component Types:")
        for comp_type, count in analysis["component_types"].items():
            click.echo(f"  • {comp_type}: {count}")
        
        if analysis["dependencies"]:
            click.echo("\n🔗 Dependencies:")
            for target, sources in analysis["dependencies"].items():
                click.echo(f"  • {target} ← {', '.join(sources)}")
        
        if analysis["potential_issues"]:
            click.echo("\n⚠️  Potential Issues:")
            for issue in analysis["potential_issues"]:
                click.echo(f"  • {issue}")
        
    except (ConversionError, ValidationError) as e:
        click.echo(f"❌ Analysis failed: {e}", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.option("--host", default="localhost", help="Server host")
@click.option("--port", default=8000, help="Server port")
@click.option("--protocol-prefix", default="langflow", help="Protocol prefix")
def serve(host: str, port: int, protocol_prefix: str):
    """Start the Langflow component server."""
    try:
        click.echo(f"🚀 Starting Langflow component server...")
        click.echo(f"   Protocol prefix: {protocol_prefix}")
        
        server = StepflowLangflowServer(protocol_prefix=protocol_prefix)
        
        # For now, only stdio mode is supported
        if host != "localhost" or port != 8000:
            click.echo("⚠️  HTTP mode not yet implemented, falling back to stdio mode")
        
        server.run()
        
    except KeyboardInterrupt:
        click.echo("\n🛑 Server stopped")
    except Exception as e:
        click.echo(f"❌ Server error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
def validate(input_file: Path):
    """Validate a Langflow workflow file."""
    try:
        converter = LangflowConverter(validate_schemas=True)
        
        # Load JSON
        with open(input_file, "r", encoding="utf-8") as f:
            langflow_data = json.load(f)
        
        # Convert with validation
        workflow = converter.convert(langflow_data)
        
        click.echo(f"✅ {input_file} is valid")
        click.echo(f"   • Workflow: {workflow.name}")
        click.echo(f"   • Steps: {len(workflow.steps)}")
        
    except (ConversionError, ValidationError) as e:
        click.echo(f"❌ Validation failed: {e}", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
@click.argument("input_json", type=str, default="{}")
@click.option("--mock", is_flag=True, help="Use mock components instead of real UDF execution")
@click.option("--config", type=click.Path(exists=True, path_type=Path), help="Custom stepflow config file")
@click.option("--stepflow-binary", type=click.Path(exists=True, path_type=Path), help="Path to stepflow binary")
@click.option("--timeout", type=int, default=60, help="Execution timeout in seconds")
@click.option("--dry-run", is_flag=True, help="Only convert, don't execute")
@click.option("--keep-files", is_flag=True, help="Keep temporary files after execution")
@click.option("--output-dir", type=click.Path(path_type=Path), help="Directory to save temporary files")
def execute(
    input_file: Path,
    input_json: str,
    mock: bool,
    config: Optional[Path],
    stepflow_binary: Optional[Path],
    timeout: int,
    dry_run: bool,
    keep_files: bool,
    output_dir: Optional[Path]
):
    """Convert and execute a Langflow workflow using Stepflow.
    
    INPUT_FILE: Path to Langflow JSON workflow file
    INPUT_JSON: JSON input data for the workflow (default: {})
    """
    import tempfile
    import subprocess
    import shutil
    from pathlib import Path
    
    try:
        # Parse input JSON
        try:
            input_data = json.loads(input_json)
        except json.JSONDecodeError as e:
            click.echo(f"❌ Invalid input JSON: {e}", err=True)
            sys.exit(1)
        
        click.echo(f"🔄 Converting {input_file}...")
        
        # Convert Langflow to Stepflow
        converter = LangflowConverter()
        stepflow_yaml = converter.convert_file(input_file)
        
        click.echo(f"✅ Conversion completed")
        
        if dry_run:
            click.echo("\n📄 Converted workflow:")
            click.echo(stepflow_yaml)
            return
        
        # Set up temporary directory
        if output_dir:
            temp_dir = output_dir
            temp_dir.mkdir(parents=True, exist_ok=True)
        else:
            temp_dir = Path(tempfile.mkdtemp())
        
        workflow_file = temp_dir / "workflow.yaml"
        config_file = temp_dir / "stepflow-config.yml"
        
        # Write workflow file
        with open(workflow_file, "w", encoding="utf-8") as f:
            f.write(stepflow_yaml)
        
        # Create or copy config file
        if config:
            shutil.copy2(config, config_file)
            click.echo(f"📝 Using custom config: {config}")
        else:
            # Generate appropriate config
            # Get the langflow integration root directory (where pyproject.toml is)
            current_file = Path(__file__).resolve()
            current_dir = current_file.parent.parent.parent.parent
            
            if mock:
                mock_server_path = current_dir / "tests" / "mock_langflow_server.py"
                config_content = f"""plugins:
  builtin:
    type: builtin
  mock_langflow:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "{current_dir}", "run", "python", "{mock_server_path}"]

routes:
  "/langflow/{{*component}}":
    - plugin: mock_langflow
  "/builtin/{{*component}}":
    - plugin: builtin

stateStore:
  type: inMemory
"""
                click.echo("🎭 Using mock components")
            else:
                config_content = f"""plugins:
  builtin:
    type: builtin
  real_langflow:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "{current_dir}", "run", "stepflow-langflow-server"]

routes:
  "/langflow/{{*component}}":
    - plugin: real_langflow
  "/builtin/{{*component}}":
    - plugin: builtin

stateStore:
  type: inMemory
"""
                click.echo("🚀 Using real Langflow UDF execution")
            
            with open(config_file, "w", encoding="utf-8") as f:
                f.write(config_content)
        
        # Find stepflow binary
        if stepflow_binary:
            binary_path = stepflow_binary
        else:
            # Look for stepflow binary in common locations
            possible_paths = [
                Path("../../stepflow-rs/target/debug/stepflow"),
                Path("../../../stepflow-rs/target/debug/stepflow"),
                Path("target/debug/stepflow"),
            ]
            
            binary_path = None
            for path in possible_paths:
                if path.exists():
                    binary_path = path.resolve()
                    break
            
            if not binary_path:
                click.echo("❌ Stepflow binary not found. Please specify --stepflow-binary or build the project", err=True)
                sys.exit(1)
        
        click.echo(f"🎯 Executing workflow with stepflow...")
        click.echo(f"   • Input: {input_json}")
        click.echo(f"   • Timeout: {timeout}s")
        
        # Execute with stepflow
        cmd = [
            str(binary_path),
            "run",
            f"--flow={workflow_file}",
            f"--config={config_file}",
            f"--input-json={input_json}"
        ]
        
        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout
            )
            
            if result.returncode == 0:
                click.echo("✅ Execution completed successfully!")
                click.echo("\n🎯 Results:")
                # Try to pretty-print JSON output
                try:
                    # Look for JSON in the output
                    lines = result.stdout.strip().split('\n')
                    for line in lines:
                        if line.strip().startswith('{') and line.strip().endswith('}'):
                            result_data = json.loads(line)
                            click.echo(json.dumps(result_data, indent=2))
                            break
                    else:
                        click.echo(result.stdout)
                except:
                    click.echo(result.stdout)
            else:
                click.echo("❌ Execution failed")
                click.echo("STDOUT:", err=True)
                click.echo(result.stdout, err=True)
                click.echo("STDERR:", err=True)
                click.echo(result.stderr, err=True)
                sys.exit(1)
                
        except subprocess.TimeoutExpired:
            click.echo(f"❌ Execution timed out after {timeout} seconds", err=True)
            sys.exit(1)
        
        # Cleanup temporary files
        if not keep_files and not output_dir:
            shutil.rmtree(temp_dir)
            click.echo("🧹 Temporary files cleaned up")
        else:
            click.echo(f"📁 Files kept in: {temp_dir}")
            click.echo(f"   • Workflow: {workflow_file}")
            click.echo(f"   • Config: {config_file}")
            
        click.echo("🎉 Langflow-to-Stepflow execution complete!")
        
    except (ConversionError, ValidationError) as e:
        click.echo(f"❌ Conversion failed: {e}", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


if __name__ == "__main__":
    main()