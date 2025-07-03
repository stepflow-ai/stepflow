#!/usr/bin/env python3
"""
Protocol generation script for StepFlow Python SDK.

This script generates the protocol types from the JSON schema.
"""

import subprocess
import sys
from pathlib import Path


def main():
    """Generate protocol types from JSON schema."""

    # Get paths
    script_dir = Path(__file__).parent
    project_root = script_dir.parent.parent
    schema_path = project_root.parent.parent / "schemas" / "protocol.json"
    output_path = script_dir / "generated_protocol.py"

    # Check that schema exists
    if not schema_path.exists():
        print(f"Error: Schema file not found at {schema_path}")
        return 1

    print(f"Generating protocol from {schema_path}")

    # Run datamodel-code-generator
    cmd = [
        "python",
        "-m",
        "datamodel_code_generator",
        "--input",
        str(schema_path),
        "--output",
        str(output_path),
        "--output-model-type",
        "msgspec.Struct",
        "--field-constraints",
        "--use-union-operator",
    ]

    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True)

    if result.returncode != 0:
        print(f"Error generating protocol: {result.stderr}")
        return result.returncode

    print(f"✓ Generated {output_path}")

    # Add generation instructions to the top
    print("Adding generation documentation...")

    with open(output_path, "r") as f:
        content = f.read()

    header_lines = content.split("\n")
    header_end = next(
        i for i, line in enumerate(header_lines) if line.startswith("from __future__")
    )

    generation_docs = [
        "# To regenerate this file, run:",
        "#   uv run generate-protocol",
        "",
    ]

    new_content = "\n".join(
        header_lines[:header_end] + generation_docs + header_lines[header_end:]
    )

    with open(output_path, "w") as f:
        f.write(new_content)

    print("✓ Added generation documentation")
    print(f"✓ Protocol generation complete!")

    return 0


if __name__ == "__main__":
    sys.exit(main())
