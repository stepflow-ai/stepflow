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
@click.argument("output_file", type=click.Path(path_type=Path))
@click.option("--pretty", is_flag=True, help="Pretty-print the output YAML")
@click.option("--validate", is_flag=True, help="Validate schemas during conversion")
def convert(
    input_file: Path, 
    output_file: Path, 
    pretty: bool,
    validate: bool
):
    """Convert a Langflow JSON workflow to Stepflow YAML."""
    try:
        converter = LangflowConverter(validate_schemas=validate)
        stepflow_yaml = converter.convert_file(input_file)
        
        # Write output
        with open(output_file, "w", encoding="utf-8") as f:
            f.write(stepflow_yaml)
        
        click.echo(f"✅ Successfully converted {input_file} to {output_file}")
        
    except (ConversionError, ValidationError) as e:
        click.echo(f"❌ Conversion failed: {e}", err=True)
        sys.exit(1)
    except Exception as e:
        click.echo(f"❌ Unexpected error: {e}", err=True)
        sys.exit(1)


@main.command()
@click.argument("input_file", type=click.Path(exists=True, path_type=Path))
def analyze(input_file: Path):
    """Analyze a Langflow workflow structure."""
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


if __name__ == "__main__":
    main()