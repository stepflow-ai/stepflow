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

"""Main CLI entry point for Langflow integration."""

import json
import sys
from pathlib import Path

import click
from dotenv import load_dotenv

from ..converter.stepflow_tweaks import (
    apply_stepflow_tweaks_to_dict,
)
from ..converter.translator import LangflowConverter
from ..exceptions import ConversionError, ValidationError
from ..executor.langflow_server import StepflowLangflowServer


@click.group()
@click.version_option()
def main():
    """Stepflow Langflow Integration CLI."""
    # Load environment variables from .env file if it exists
    load_dotenv()
    pass


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
@click.argument("output_file", type=click.Path(path_type=Path), required=False)
def convert(input_file: Path, output_file: Path | None):
    """Convert a Langflow JSON workflow to Stepflow YAML.

    If no output file is specified, prints the YAML to stdout.
    """
    try:
        converter = LangflowConverter()
        stepflow_yaml = converter.convert_file(input_file)

        if output_file:
            # Write to file
            with open(output_file, "w", encoding="utf-8") as f:
                f.write(stepflow_yaml)
            click.echo(
                f"✅ Successfully converted {input_file} to {output_file}", err=True
            )
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
@click.argument("stepflow_file", type=click.Path(exists=True, path_type=Path))
@click.argument("output_file", type=click.Path(path_type=Path), required=False)
@click.option(
    "--tweaks",
    type=str,
    required=True,
    help="JSON string of tweaks to apply (node_id -> {field: value})",
)
def tweak(stepflow_file: Path, output_file: Path | None, tweaks: str):
    """Apply tweaks to a Stepflow YAML workflow file.

    STEPFLOW_FILE: Path to Stepflow YAML workflow file
    OUTPUT_FILE: Path for output file (optional, prints to stdout if not provided)

    Tweaks should be JSON format:
    --tweaks '{"LanguageModelComponent-kBOja": {"api_key": "new_key"}}'
    """
    try:
        # Parse tweaks
        try:
            parsed_tweaks = json.loads(tweaks)
        except json.JSONDecodeError as e:
            click.echo(f"❌ Invalid tweaks JSON: {e}", err=True)
            sys.exit(1)

        click.echo(f"🔧 Applying tweaks to {len(parsed_tweaks)} components...")

        # Load workflow
        try:
            import yaml

            with open(stepflow_file, encoding="utf-8") as f:
                workflow_dict = yaml.safe_load(f)
        except Exception as e:
            click.echo(f"❌ Error loading workflow: {e}", err=True)
            sys.exit(1)

        # Apply tweaks
        tweaked_dict = apply_stepflow_tweaks_to_dict(workflow_dict, parsed_tweaks)

        # Convert back to YAML
        import yaml

        tweaked_yaml = yaml.dump(
            tweaked_dict,
            default_flow_style=False,
            sort_keys=False,
            allow_unicode=True,
            width=120,
        )

        if output_file:
            # Write to file
            with open(output_file, "w", encoding="utf-8") as f:
                f.write(tweaked_yaml)
            click.echo(f"✅ Tweaked workflow written to {output_file}")
        else:
            # Write to stdout
            click.echo(tweaked_yaml)

        click.echo("✅ Tweaks applied successfully")

    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
def analyze(input_file: Path):
    """Analyze a Langflow workflow structure.

    This command provides detailed analysis of a Langflow JSON workflow without
    converting it. It examines the workflow structure, component types, dependencies,
    and identifies potential issues that might affect conversion. Useful for
    understanding workflow complexity and debugging conversion problems before
    attempting the full conversion process.
    """
    try:
        converter = LangflowConverter()

        # Load and analyze
        with open(input_file, encoding="utf-8") as f:
            langflow_data = json.load(f)

        analysis = converter.analyze(langflow_data)

        # Display results
        click.echo(f"📊 Analysis of {input_file}:")
        click.echo(f"  • Nodes: {analysis.node_count}")
        click.echo(f"  • Edges: {analysis.edge_count}")

        click.echo("\n📦 Component Types:")
        for comp_type, count in analysis.component_types.items():
            click.echo(f"  • {comp_type}: {count}")

        if analysis.dependencies:
            click.echo("\n🔗 Dependencies:")
            for target, sources in analysis.dependencies.items():
                click.echo(f"  • {target} ← {', '.join(sources)}")

        if analysis.potential_issues:
            click.echo("\n⚠️  Potential Issues:")
            for issue in analysis.potential_issues:
                click.echo(f"  • {issue}")

    except (ConversionError, ValidationError) as e:
        click.echo(f"❌ Analysis failed: {e}", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.option("--host", default="localhost", help="Server host")
@click.option("--port", default=5264, help="Server port")
@click.option("--protocol-prefix", default="langflow", help="Protocol prefix")
@click.option("--http", is_flag=True, help="Start in HTTP mode (default is STDIO)")
@click.option("--workers", default=3, help="Number of worker processes for HTTP mode")
@click.option("--backlog", default=128, help="Maximum number of pending connections")
@click.option("--timeout-keep-alive", default=5, help="Keep-alive timeout in seconds")
def serve(
    host: str,
    port: int,
    protocol_prefix: str,
    http: bool,
    workers: int,
    backlog: int,
    timeout_keep_alive: int,
):
    """Start the Langflow component server."""
    import asyncio

    try:
        server = StepflowLangflowServer()

        if http:
            click.echo("🚀 Starting Langflow component server in HTTP mode...")
            click.echo(f"   Host: {host}")
            click.echo(f"   Port: {port}")
            click.echo(f"   Protocol prefix: {protocol_prefix}")
            click.echo(f"   Workers: {workers}")
            click.echo(f"   Backlog: {backlog}")
            click.echo(f"   Keep-alive timeout: {timeout_keep_alive}s")

            # Apply nest_asyncio to handle nested event loops in HTTP mode
            import nest_asyncio  # type: ignore

            nest_asyncio.apply()

            # Run the HTTP server
            asyncio.run(
                server.serve(
                    host=host,
                    port=port,
                    workers=workers,
                    backlog=backlog,
                    timeout_keep_alive=timeout_keep_alive,
                )
            )
        else:
            click.echo("🚀 Starting Langflow component server in STDIO mode...")
            click.echo(f"   Protocol prefix: {protocol_prefix}")

            # Increase pipe buffer size to handle large payloads
            # (e.g., Wikipedia articles)
            if sys.platform == "linux":
                import fcntl

                try:
                    # Set stdout buffer to 1MB to handle large responses
                    fcntl.fcntl(sys.stdout.fileno(), fcntl.F_SETPIPE_SZ, 1048576)
                    click.echo("   Pipe buffer size increased to 1MB")
                except (OSError, AttributeError) as e:
                    # F_SETPIPE_SZ might not be available on all platforms
                    click.echo(
                        f"   Warning: Could not increase pipe buffer: {e}", err=True
                    )
            else:
                click.echo(
                    "   Warning: Cannot increase pipe buffer on this platform",
                    err=True,
                )

            # Run the STDIO server
            server.run()

    except KeyboardInterrupt:
        click.echo("\n🛑 Server stopped")
    except Exception as e:
        click.echo(f"❌ Server error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
@click.argument("input_json", type=str, default="{}")
@click.option(
    "--config",
    type=click.Path(exists=True, path_type=Path),
    help="Custom stepflow config file",
)
@click.option(
    "--stepflow-binary",
    type=click.Path(exists=True, path_type=Path),
    help="Path to stepflow binary",
)
@click.option("--timeout", type=int, default=60, help="Execution timeout in seconds")
@click.option("--dry-run", is_flag=True, help="Only convert, don't execute")
@click.option("--keep-files", is_flag=True, help="Keep temporary files after execution")
@click.option(
    "--output-dir",
    type=click.Path(path_type=Path),
    help="Directory to save temporary files",
)
@click.option(
    "--tweaks",
    type=str,
    help="JSON string of tweaks to apply (node_id -> {field: value})",
)
def execute(
    input_file: Path,
    input_json: str,
    config: Path | None,
    stepflow_binary: Path | None,
    timeout: int,
    dry_run: bool,
    keep_files: bool,
    output_dir: Path | None,
    tweaks: str | None,
):
    """Convert and execute a Langflow workflow using Stepflow.

    INPUT_FILE: Path to Langflow JSON workflow file
    INPUT_JSON: JSON input data for the workflow (default: {})

    Tweaks can be provided as JSON string to modify component configurations:
    --tweaks '{"LanguageModelComponent-kBOja": {"api_key": "new_key"}}'
    """
    import shutil
    import subprocess
    import tempfile
    import time
    from pathlib import Path

    try:
        # Parse input JSON
        try:
            json.loads(input_json)  # Validate JSON format
        except json.JSONDecodeError as e:
            click.echo(f"❌ Invalid input JSON: {e}", err=True)
            sys.exit(1)

        click.echo(f"🔄 Converting {input_file}...")

        # Convert Langflow to Stepflow (no tweaks at conversion time)
        converter = LangflowConverter()
        stepflow_yaml = converter.convert_file(input_file)

        click.echo("✅ Conversion completed")

        # Apply tweaks if provided (before dry-run check)
        if tweaks:
            try:
                parsed_tweaks = json.loads(tweaks)
                click.echo(f"🔧 Applying tweaks to {len(parsed_tweaks)} components...")

                # Parse the YAML, apply tweaks
                import yaml

                workflow_dict = yaml.safe_load(stepflow_yaml)
                tweaked_dict = apply_stepflow_tweaks_to_dict(
                    workflow_dict, parsed_tweaks
                )
                stepflow_yaml = yaml.dump(
                    tweaked_dict,
                    default_flow_style=False,
                    sort_keys=False,
                    allow_unicode=True,
                    width=120,
                )

                click.echo("✅ Tweaks applied")
            except json.JSONDecodeError as e:
                click.echo(f"❌ Invalid tweaks JSON: {e}", err=True)
                sys.exit(1)
            except Exception as e:
                click.echo(f"❌ Error applying tweaks: {e}", err=True)
                sys.exit(1)

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

        # Write workflow file (tweaks already applied if provided)
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

            # Always use real Langflow UDF execution
            config_content = f"""plugins:
  builtin:
    type: builtin
  langflow:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "{current_dir}", "run", "stepflow-langflow-server"]

routes:
  "/langflow/{{*component}}":
    - plugin: langflow
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
                click.echo(
                    "❌ Stepflow binary not found. Please specify --stepflow-binary "
                    "or build the project",
                    err=True,
                )
                sys.exit(1)

        click.echo("🎯 Executing workflow with stepflow...")
        click.echo(f"   • Input: {input_json}")
        click.echo(f"   • Timeout: {timeout}s")

        # Create output and log files for clean separation
        output_file = temp_dir / "result.json"
        log_file = temp_dir / "execution.log"

        # Execute with stepflow using clean output separation
        cmd = [
            str(binary_path),
            "run",
            f"--flow={workflow_file}",
            f"--config={config_file}",
            f"--input-json={input_json}",
            f"--output={output_file}",
            f"--log-file={log_file}",
        ]

        try:
            start_time = time.time()
            result = subprocess.run(
                cmd, capture_output=True, text=True, timeout=timeout
            )
            end_time = time.time()
            duration = end_time - start_time

            if result.returncode == 0:
                click.echo(f"✅ Execution completed successfully in {duration:.2f}s!")
                click.echo("\n🎯 Results:")

                # Read clean JSON result from output file
                workflow_success = True
                result_data = None

                try:
                    if output_file.exists():
                        with open(output_file) as f:
                            output_content = f.read().strip()
                        if output_content:
                            result_data = json.loads(output_content)
                            click.echo(json.dumps(result_data, indent=2))

                            # Check if workflow outcome is success
                            outcome = result_data.get("outcome", "unknown")
                            if outcome != "success":
                                workflow_success = False
                                click.echo(
                                    f"\n❌ Workflow failed with outcome: {outcome}",
                                    err=True,
                                )
                    else:
                        # Fallback to stdout parsing if output file doesn't exist
                        if result.stdout.strip():
                            lines = result.stdout.strip().split("\n")
                            for line in lines:
                                if line.strip().startswith(
                                    "{"
                                ) and line.strip().endswith("}"):
                                    result_data = json.loads(line)
                                    click.echo(json.dumps(result_data, indent=2))
                                    break
                            else:
                                click.echo(result.stdout)

                except Exception as e:
                    # Fallback to stdout if file parsing fails
                    if result.stdout:
                        click.echo(result.stdout)
                    click.echo(f"\n⚠️  Could not parse result: {e}", err=True)

                # Exit with error code if workflow failed
                if not workflow_success:
                    sys.exit(
                        2
                    )  # Use exit code 2 for workflow failure vs 1 for system failure
            else:
                click.echo(f"❌ Execution failed in {duration:.2f}s")
                if result.stdout:
                    click.echo("STDOUT:", err=True)
                    click.echo(result.stdout, err=True)
                if result.stderr:
                    click.echo("STDERR:", err=True)
                    click.echo(result.stderr, err=True)

                # Also show logs from log file if available for debugging
                if log_file.exists():
                    try:
                        with open(log_file) as f:
                            log_content = f.read().strip()
                        if log_content:
                            click.echo("LOGS:", err=True)
                            click.echo(log_content, err=True)
                    except Exception:
                        pass

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


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
@click.argument("inputs_jsonl", type=click.Path(exists=True, path_type=Path))
@click.option(
    "--url",
    type=str,
    required=True,
    help="Stepflow server URL (e.g., http://localhost:7837/api/v1)",
)
@click.option(
    "--tweaks",
    type=str,
    help="JSON string of tweaks to apply (node_id -> {field: value})",
)
@click.option(
    "--max-concurrent",
    type=int,
    help="Maximum number of concurrent executions",
)
@click.option(
    "--output",
    type=click.Path(path_type=Path),
    help="Output JSONL file for batch results",
)
def submit_batch(
    input_file: Path,
    inputs_jsonl: Path,
    url: str,
    tweaks: str | None,
    max_concurrent: int | None,
    output: Path | None,
):
    """Convert Langflow workflow and submit batch execution to Stepflow server.

    INPUT_FILE: Path to Langflow JSON workflow file
    INPUTS_JSONL: Path to JSONL file with inputs (one JSON per line)

    Requires a running Stepflow server. The workflow is converted once,
    then all inputs are executed in batch.

    Example:
        stepflow-langflow submit-batch flow.json inputs.jsonl \\
            --url http://localhost:7837/api/v1 \\
            --tweaks '{"Component-123": {"api_key": "sk-..."}}' \\
            --max-concurrent 5 \\
            --output results.jsonl
    """
    import subprocess
    import tempfile

    try:
        click.echo(f"🔄 Converting {input_file}...")

        # Convert Langflow to Stepflow (once)
        converter = LangflowConverter()
        stepflow_yaml = converter.convert_file(input_file)

        click.echo("✅ Conversion completed")

        # Apply tweaks if provided (once)
        if tweaks:
            try:
                parsed_tweaks = json.loads(tweaks)
                click.echo(f"🔧 Applying tweaks to {len(parsed_tweaks)} components...")

                import yaml

                workflow_dict = yaml.safe_load(stepflow_yaml)
                tweaked_dict = apply_stepflow_tweaks_to_dict(
                    workflow_dict, parsed_tweaks
                )
                stepflow_yaml = yaml.dump(
                    tweaked_dict,
                    default_flow_style=False,
                    sort_keys=False,
                    allow_unicode=True,
                    width=120,
                )

                click.echo("✅ Tweaks applied")
            except json.JSONDecodeError as e:
                click.echo(f"❌ Invalid tweaks JSON: {e}", err=True)
                sys.exit(1)
            except Exception as e:
                click.echo(f"❌ Error applying tweaks: {e}", err=True)
                sys.exit(1)

        # Write workflow to temporary file
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".yaml", delete=False
        ) as workflow_file:
            workflow_file.write(stepflow_yaml)
            workflow_path = workflow_file.name

        try:
            # Find stepflow binary
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
                click.echo(
                    "❌ Stepflow binary not found. Please build the project first.",
                    err=True,
                )
                sys.exit(1)

            click.echo(f"🚀 Submitting batch to {url}...")
            click.echo(f"   • Workflow: {input_file}")
            click.echo(f"   • Inputs: {inputs_jsonl}")
            if max_concurrent:
                click.echo(f"   • Max concurrent: {max_concurrent}")
            if output:
                click.echo(f"   • Output: {output}")

            # Build command
            cmd = [
                str(binary_path),
                "submit-batch",
                f"--url={url}",
                f"--flow={workflow_path}",
                f"--inputs={inputs_jsonl}",
            ]

            if max_concurrent is not None:
                cmd.append(f"--max-concurrent={max_concurrent}")

            if output is not None:
                cmd.append(f"--output={output}")

            # Submit batch (inherit stdout/stderr for progress display)
            result = subprocess.run(
                cmd,
                timeout=300,  # 5 minute default timeout for batch
            )

            if result.returncode == 0:
                click.echo("\n✅ Batch execution completed successfully!")
            else:
                click.echo("\n❌ Batch execution failed", err=True)
                sys.exit(1)

        finally:
            # Cleanup temporary workflow file
            import os

            os.unlink(workflow_path)

    except (ConversionError, ValidationError) as e:
        click.echo(f"❌ Conversion failed: {e}", err=True)
        sys.exit(1)
    except subprocess.TimeoutExpired:
        click.echo("❌ Batch execution timed out", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
@click.argument("input_json", type=str, default="{}")
@click.option(
    "--url",
    type=str,
    required=True,
    help="Stepflow server URL (e.g., http://localhost:7837/api/v1)",
)
@click.option(
    "--tweaks",
    type=str,
    help="JSON string of tweaks to apply (node_id -> {field: value})",
)
def convert_and_submit(
    input_file: Path,
    input_json: str,
    url: str,
    tweaks: str | None,
):
    """Convert Langflow workflow to Stepflow and submit it to a running server.

    INPUT_FILE: Path to Langflow JSON workflow file
    INPUT_JSON: JSON input data for the workflow (default: {})

    This command converts a Langflow workflow to Stepflow format and submits it
    to a running Stepflow server for execution.
    """
    import subprocess
    import tempfile

    try:
        # Parse input JSON
        try:
            json.loads(input_json)  # Validate JSON format
        except json.JSONDecodeError as e:
            click.echo(f"❌ Invalid input JSON: {e}", err=True)
            sys.exit(1)

        click.echo(f"🔄 Converting {input_file}...")

        # Convert Langflow to Stepflow (no tweaks at conversion time)
        converter = LangflowConverter()
        stepflow_yaml = converter.convert_file(input_file)

        click.echo("✅ Conversion completed")

        # Apply tweaks if provided
        if tweaks:
            try:
                parsed_tweaks = json.loads(tweaks)
                click.echo(f"🔧 Applying tweaks to {len(parsed_tweaks)} components...")

                # Parse the YAML, apply tweaks
                import yaml

                workflow_dict = yaml.safe_load(stepflow_yaml)
                tweaked_dict = apply_stepflow_tweaks_to_dict(
                    workflow_dict, parsed_tweaks
                )
                stepflow_yaml = yaml.dump(
                    tweaked_dict,
                    default_flow_style=False,
                    sort_keys=False,
                    allow_unicode=True,
                    width=120,
                )

                click.echo("✅ Tweaks applied")
            except json.JSONDecodeError as e:
                click.echo(f"❌ Invalid tweaks JSON: {e}", err=True)
                sys.exit(1)
            except Exception as e:
                click.echo(f"❌ Error applying tweaks: {e}", err=True)
                sys.exit(1)

        # Create temporary files
        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
            f.write(stepflow_yaml)
            workflow_file = f.name

        try:
            # Find stepflow binary
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
                click.echo(
                    "❌ Stepflow binary not found. Please build the project first.",
                    err=True,
                )
                sys.exit(1)

            click.echo(f"🚀 Submitting workflow to {url}...")

            # Submit with stepflow
            cmd = [
                str(binary_path),
                "submit",
                f"--url={url}",
                f"--flow={workflow_file}",
                f"--input-json={input_json}",
            ]

            result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)

            if result.returncode == 0:
                click.echo("✅ Workflow submitted successfully!")
                click.echo("\n🎯 Results:")
                try:
                    # Try to pretty-print JSON output
                    lines = result.stdout.strip().split("\n")
                    for line in lines:
                        if line.strip().startswith("{") and line.strip().endswith("}"):
                            result_data = json.loads(line)
                            click.echo(json.dumps(result_data, indent=2))
                            break
                    else:
                        click.echo(result.stdout)
                except Exception:
                    click.echo(result.stdout)
            else:
                click.echo("❌ Workflow submission failed")
                click.echo("STDOUT:", err=True)
                click.echo(result.stdout, err=True)
                click.echo("STDERR:", err=True)
                click.echo(result.stderr, err=True)
                sys.exit(1)

        finally:
            # Cleanup temporary file
            import os

            os.unlink(workflow_file)

    except (ConversionError, ValidationError) as e:
        click.echo(f"❌ Conversion failed: {e}", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("workflow_file", type=click.Path(exists=True, path_type=Path))
@click.argument("input_json", type=str)
@click.option(
    "--url",
    type=str,
    required=True,
    help="Stepflow server URL (e.g., http://localhost:7837/api/v1)",
)
@click.option(
    "--tweaks",
    type=str,
    help="JSON string of tweaks to apply (node_id -> {field: value})",
)
def run_flow(
    workflow_file: Path,
    input_json: str,
    url: str,
    tweaks: str | None,
):
    """Run a Stepflow workflow with tweaks on a running server.

    WORKFLOW_FILE: Path to Stepflow YAML workflow file
    INPUT_JSON: JSON input data for the workflow

    This command submits a Stepflow workflow to a running server with optional tweaks.
    """
    import subprocess
    import tempfile

    try:
        # Parse input JSON
        try:
            json.loads(input_json)  # Validate JSON format
        except json.JSONDecodeError as e:
            click.echo(f"❌ Invalid input JSON: {e}", err=True)
            sys.exit(1)

        # Load workflow
        try:
            import yaml

            with open(workflow_file, encoding="utf-8") as f:
                workflow_dict = yaml.safe_load(f)
        except Exception as e:
            click.echo(f"❌ Error loading workflow: {e}", err=True)
            sys.exit(1)

        # Apply tweaks if provided
        if tweaks:
            try:
                parsed_tweaks = json.loads(tweaks)
                click.echo(f"🔧 Applying tweaks to {len(parsed_tweaks)} components...")

                tweaked_dict = apply_stepflow_tweaks_to_dict(
                    workflow_dict, parsed_tweaks
                )
                tweaked_yaml = yaml.dump(
                    tweaked_dict,
                    default_flow_style=False,
                    sort_keys=False,
                    allow_unicode=True,
                    width=120,
                )

                # Write tweaked workflow to temp file
                with tempfile.NamedTemporaryFile(
                    mode="w", suffix=".yaml", delete=False
                ) as f:
                    f.write(tweaked_yaml)
                    temp_workflow_file = f.name

                click.echo("✅ Tweaks applied")
            except json.JSONDecodeError as e:
                click.echo(f"❌ Invalid tweaks JSON: {e}", err=True)
                sys.exit(1)
            except Exception as e:
                click.echo(f"❌ Error applying tweaks: {e}", err=True)
                sys.exit(1)
        else:
            temp_workflow_file = str(workflow_file)

        try:
            # Find stepflow binary
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
                click.echo(
                    "❌ Stepflow binary not found. Please build the project first.",
                    err=True,
                )
                sys.exit(1)

            click.echo(f"🚀 Running workflow on {url}...")

            # Submit with stepflow
            cmd = [
                str(binary_path),
                "submit",
                f"--url={url}",
                f"--flow={temp_workflow_file}",
                f"--input-json={input_json}",
            ]

            result = subprocess.run(cmd, capture_output=True, text=True, timeout=60)

            if result.returncode == 0:
                click.echo("✅ Workflow executed successfully!")
                click.echo("\n🎯 Results:")
                try:
                    # Try to pretty-print JSON output
                    lines = result.stdout.strip().split("\n")
                    for line in lines:
                        if line.strip().startswith("{") and line.strip().endswith("}"):
                            result_data = json.loads(line)
                            click.echo(json.dumps(result_data, indent=2))
                            break
                    else:
                        click.echo(result.stdout)
                except Exception:
                    click.echo(result.stdout)
            else:
                click.echo("❌ Workflow execution failed")
                click.echo("STDOUT:", err=True)
                click.echo(result.stdout, err=True)
                click.echo("STDERR:", err=True)
                click.echo(result.stderr, err=True)
                sys.exit(1)

        finally:
            # Cleanup temporary file if we created one
            if tweaks and temp_workflow_file != str(workflow_file):
                import os

                os.unlink(temp_workflow_file)

    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
