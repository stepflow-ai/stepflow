#!/usr/bin/env python3
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

"""Generate stepflow-api models from the OpenAPI spec.

Only the models/ directory is generated. Other files (client.py, types.py, errors.py, api/)
are static source files maintained in the repository.

Usage:
    python scripts/generate_api_client.py                       # Regenerate from stored spec
    python scripts/generate_api_client.py generate              # Same as above
    python scripts/generate_api_client.py generate --from FILE  # Regenerate from specific file
    python scripts/generate_api_client.py check                 # Check if models are up-to-date
    python scripts/generate_api_client.py update-spec           # Update stored spec from server
    python scripts/generate_api_client.py update-spec --generate  # Update spec AND regenerate
"""

from __future__ import annotations

import argparse
import atexit
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
PYTHON_SDK_DIR = SCRIPT_DIR.parent
PROJECT_ROOT = PYTHON_SDK_DIR.parent.parent
API_CLIENT_DIR = PYTHON_SDK_DIR / "stepflow-api"
MODELS_DIR = API_CLIENT_DIR / "src" / "stepflow_api" / "models"
STORED_SPEC = PROJECT_ROOT / "schemas" / "openapi.json"
PORT = 17837

# Global for cleanup
_server_process: subprocess.Popen | None = None
_temp_dir: Path | None = None


def cleanup():
    """Clean up server process and temp directory."""
    global _server_process, _temp_dir
    if _server_process is not None:
        _server_process.terminate()
        try:
            _server_process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            _server_process.kill()
        _server_process = None
    if _temp_dir is not None and _temp_dir.exists():
        shutil.rmtree(_temp_dir)
        _temp_dir = None


atexit.register(cleanup)
signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
signal.signal(signal.SIGINT, lambda *_: sys.exit(0))


def fetch_spec_from_server(output_path: Path) -> None:
    """Build and start server, fetch OpenAPI spec."""
    global _server_process

    print(">>> Building stepflow-server...")
    subprocess.run(
        ["cargo", "build", "--release", "-p", "stepflow-server"],
        cwd=PROJECT_ROOT / "stepflow-rs",
        check=True,
        capture_output=True,
    )

    print(f">>> Starting stepflow-server on port {PORT}...")
    server_bin = PROJECT_ROOT / "stepflow-rs" / "target" / "release" / "stepflow-server"
    _server_process = subprocess.Popen(
        [str(server_bin), "--port", str(PORT)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    print(">>> Waiting for server to be ready...")
    for i in range(30):
        try:
            urllib.request.urlopen(f"http://localhost:{PORT}/health", timeout=1)
            break
        except Exception:
            if i == 29:
                raise RuntimeError("Server failed to start")
            time.sleep(1)

    print(">>> Fetching OpenAPI spec...")
    with urllib.request.urlopen(f"http://localhost:{PORT}/api/v1/openapi.json") as resp:
        output_path.write_bytes(resp.read())

    _server_process.terminate()
    _server_process.wait()
    _server_process = None


def camel_to_snake(name: str) -> str:
    """Convert CamelCase to snake_case."""
    s1 = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s1).lower()


def generate_models(spec_file: Path, output_dir: Path) -> None:
    """Generate models/generated.py using datamodel-code-generator."""
    print(">>> Generating models with datamodel-code-generator...")

    output_dir.mkdir(parents=True, exist_ok=True)

    subprocess.run(
        [
            "uvx", "--from", "datamodel-code-generator", "datamodel-codegen",
            "--input", str(spec_file),
            "--input-file-type", "openapi",
            "--output-model-type", "pydantic_v2.BaseModel",
            "--output", str(output_dir / "generated.py"),
            "--use-annotated",
            "--field-constraints",
            "--use-standard-collections",
            "--use-union-operator",
            "--target-python-version", "3.11",
            "--enum-field-as-literal", "one",
        ],
        check=True,
    )


def create_model_stubs(models_dir: Path) -> None:
    """Create stub files for all models found in generated.py."""
    generated_file = models_dir / "generated.py"
    content = generated_file.read_text()
    classes = re.findall(r"^class (\w+)\(", content, re.MULTILINE)

    print(f">>> Creating {len(classes)} model stub files...")

    for classname in classes:
        filename = camel_to_snake(classname)
        (models_dir / f"{filename}.py").write_text(f'''"""Re-export {classname} from generated models.

DO NOT EDIT - Generated by scripts/generate_api_client.py
To customize, replace this file. Custom files are preserved during regeneration.
"""

from .generated import {classname}

__all__ = ["{classname}"]
''')


def generate_models_init(models_dir: Path, custom_overrides: list[str]) -> None:
    """Generate models/__init__.py with compatibility methods."""
    # Build imports for custom overrides
    custom_imports = ""
    if custom_overrides:
        custom_imports = "\n# Import custom model overrides (replace generated versions)\n"
        for name in custom_overrides:
            class_name = "".join(word.capitalize() for word in name.replace(".py", "").split("_"))
            custom_imports += f"from .{name.replace('.py', '')} import {class_name}  # noqa: E402, F401\n"

    (models_dir / "__init__.py").write_text(f'''\
"""Generated models for the Stepflow API.

DO NOT EDIT - Generated by scripts/generate_api_client.py
"""

from __future__ import annotations

import sys
from typing import Any

from .generated import *  # noqa: F401, F403
from . import generated
from ..types import UNSET, Unset  # noqa: F401

__all__: list[str] = [name for name in dir(generated) if isinstance(getattr(generated, name), type)]
__all__.extend(["UNSET", "Unset"])
{custom_imports}

# Add to_dict/from_dict compatibility methods to all pydantic models
def _add_compatibility_methods():
    from pydantic import BaseModel
    module = sys.modules[__name__]
    for name in __all__:
        cls = getattr(module, name, None)
        if cls is None or not isinstance(cls, type) or not issubclass(cls, BaseModel):
            continue
        if not hasattr(cls, "to_dict"):
            cls.to_dict = lambda self: self.model_dump(mode="json", by_alias=True, exclude_unset=True)
        if not hasattr(cls, "from_dict"):
            cls.from_dict = classmethod(lambda cls, data: cls.model_validate(data))

_add_compatibility_methods()
''')


def format_files(directory: Path) -> None:
    """Format files with ruff."""
    subprocess.run(
        ["uvx", "ruff@0.9.4", "format", "--config", str(PYTHON_SDK_DIR / "pyproject.toml"), str(directory)],
        capture_output=True,
    )


def compare_models(generated_file: Path, existing_file: Path) -> bool:
    """Compare generated models, ignoring timestamp line."""
    if not existing_file.exists():
        return False
    gen_lines = generated_file.read_text().splitlines()
    exist_lines = existing_file.read_text().splitlines()
    # Remove timestamp line (line 3) which changes every run
    if len(gen_lines) > 2:
        gen_lines.pop(2)
    if len(exist_lines) > 2:
        exist_lines.pop(2)
    return gen_lines == exist_lines


def find_custom_overrides(models_dir: Path) -> list[str]:
    """Find custom model files (not auto-generated stubs)."""
    custom = []
    if models_dir.exists():
        for f in models_dir.glob("*.py"):
            if f.name in ("__init__.py", "generated.py"):
                continue
            if not f.read_text().startswith('"""Re-export '):
                custom.append(f.name)
    return custom


def do_generate(spec_file: Path) -> None:
    """Generate models from a spec file."""
    global _temp_dir
    _temp_dir = Path(tempfile.mkdtemp())

    print(f">>> Using spec: {spec_file}")

    # Find custom overrides before regenerating
    custom_overrides = find_custom_overrides(MODELS_DIR)

    # Generate to temp directory
    temp_models = _temp_dir / "models"
    generate_models(spec_file, temp_models)
    create_model_stubs(temp_models)
    generate_models_init(temp_models, custom_overrides)

    # Copy custom overrides to temp
    if custom_overrides:
        print(f">>> Preserving {len(custom_overrides)} custom model overrides...")
        for name in custom_overrides:
            shutil.copy(MODELS_DIR / name, temp_models / name)

    # Replace models directory
    if MODELS_DIR.exists():
        shutil.rmtree(MODELS_DIR)
    shutil.copytree(temp_models, MODELS_DIR)

    # Format
    print(">>> Formatting...")
    format_files(MODELS_DIR)

    print(f">>> Updated: {MODELS_DIR}")


def cmd_generate(args: argparse.Namespace) -> int:
    """Handle 'generate' subcommand."""
    if args.from_spec:
        spec_file = args.from_spec
        if not spec_file.exists():
            print(f"ERROR: Spec file not found: {spec_file}")
            return 1
    elif STORED_SPEC.exists():
        spec_file = STORED_SPEC
    else:
        print(f"ERROR: No stored spec at {STORED_SPEC}")
        print("Run 'update-spec' first to fetch from server.")
        return 1

    do_generate(spec_file)
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    """Handle 'check' subcommand."""
    global _temp_dir
    _temp_dir = Path(tempfile.mkdtemp())

    print(">>> Checking if models match stored spec...")

    if not STORED_SPEC.exists():
        print(f"ERROR: No stored spec at {STORED_SPEC}")
        return 1

    temp_models = _temp_dir / "models"
    generate_models(STORED_SPEC, temp_models)
    format_files(temp_models)

    if not compare_models(temp_models / "generated.py", MODELS_DIR / "generated.py"):
        print("ERROR: Models are out of date!")
        print("\nTo fix: uv run poe api-gen")
        return 1

    print(">>> Models are up-to-date")
    return 0


def cmd_update_spec(args: argparse.Namespace) -> int:
    """Handle 'update-spec' subcommand."""
    print(">>> Updating stored OpenAPI spec from server...")
    fetch_spec_from_server(STORED_SPEC)
    print(f">>> Stored spec updated: {STORED_SPEC}")

    if args.generate:
        print()
        do_generate(STORED_SPEC)

    return 0


def main():
    parser = argparse.ArgumentParser(
        description="Generate stepflow-api models from OpenAPI spec",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
subcommands:
  generate      Regenerate models from stored spec (default)
  check         Check if models are up-to-date with stored spec
  update-spec   Fetch new spec from running stepflow-server

examples:
  %(prog)s                          # Regenerate from stored spec
  %(prog)s generate --from FILE     # Regenerate from specific file
  %(prog)s check                    # Verify models are current
  %(prog)s update-spec              # Fetch new spec from server
  %(prog)s update-spec --generate   # Fetch spec AND regenerate
""",
    )

    subparsers = parser.add_subparsers(dest="command", metavar="command")

    # generate subcommand
    gen_parser = subparsers.add_parser(
        "generate",
        help="Regenerate models from OpenAPI spec",
        description="Regenerate models from the stored OpenAPI spec or a specific file.",
    )
    gen_parser.add_argument(
        "--from", dest="from_spec", type=Path, metavar="FILE",
        help="Generate from specific spec file instead of stored spec",
    )

    # check subcommand
    subparsers.add_parser(
        "check",
        help="Check if models are up-to-date",
        description="Verify generated models match the stored OpenAPI spec. Exits non-zero if out of date.",
    )

    # update-spec subcommand
    update_parser = subparsers.add_parser(
        "update-spec",
        help="Update stored spec from server",
        description="Build stepflow-server, run it, and fetch the OpenAPI spec.",
    )
    update_parser.add_argument(
        "--generate", action="store_true",
        help="Also regenerate models after updating spec",
    )

    args = parser.parse_args()

    print("=== stepflow-api model generation ===")

    # Default to 'generate' if no subcommand given
    if args.command is None:
        args.command = "generate"
        args.from_spec = None

    if args.command == "generate":
        result = cmd_generate(args)
    elif args.command == "check":
        result = cmd_check(args)
    elif args.command == "update-spec":
        result = cmd_update_spec(args)
    else:
        parser.print_help()
        result = 1

    print("=== Done ===")
    return result


if __name__ == "__main__":
    sys.exit(main())
