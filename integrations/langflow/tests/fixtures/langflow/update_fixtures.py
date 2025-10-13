#!/usr/bin/env python3
"""Update Langflow fixture files from the installed langflow package.

This script copies example flows from the installed langflow package to ensure
the fixtures match the API version we're testing against.
"""

import hashlib
import json
import shutil
import sys
from datetime import datetime, timezone
from importlib.metadata import version
from pathlib import Path


# Mapping of local fixture names to installed package file names
FIXTURES = {
    "basic_prompting.json": "Basic Prompting.json",
    "document_qa.json": "Document Q&A.json",
    "memory_chatbot.json": "Memory Chatbot.json",
    "simple_agent.json": "Simple Agent.json",
    "vector_store_rag.json": "Vector Store RAG.json",
}


def get_langflow_version() -> str:
    """Get the installed langflow version.

    Returns:
        Version string
    """
    try:
        return version("langflow")
    except Exception as e:
        print(f"Error getting langflow version: {e}", file=sys.stderr)
        return "unknown"


def get_lfx_info() -> dict:
    """Get information about the installed lfx package.

    Returns:
        Dictionary with version and source information
    """
    info = {
        "version": "unknown",
        "source": "unknown",
    }

    try:
        info["version"] = version("lfx")
    except Exception as e:
        print(f"Warning: Could not get lfx version: {e}", file=sys.stderr)

    # Try to determine lfx source and check for custom module
    try:
        import lfx

        lfx_path = Path(lfx.__file__).parent
        # Check if we can find the custom module
        custom_module = lfx_path / "custom"
        if custom_module.exists():
            info["has_custom_module"] = True
            # Determine source by checking if it's a git checkout or site-packages
            if ".git" in str(lfx_path) or "site-packages" not in str(lfx_path):
                info["source"] = "github"
            else:
                info["source"] = "pypi"
        else:
            info["source"] = "unknown"
            info["has_custom_module"] = False
    except Exception as e:
        print(f"Warning: Could not inspect lfx installation: {e}", file=sys.stderr)

    return info


def get_starter_projects_dir() -> Path:
    """Get the path to the starter_projects directory in the installed package.

    Returns:
        Path to starter_projects directory

    Raises:
        FileNotFoundError: If starter_projects directory not found
    """
    try:
        import langflow
        langflow_path = Path(langflow.__file__).parent
        starter_projects = langflow_path / "initial_setup" / "starter_projects"

        if not starter_projects.exists():
            raise FileNotFoundError(
                f"Starter projects directory not found at {starter_projects}"
            )

        return starter_projects
    except Exception as e:
        print(f"Error finding starter_projects directory: {e}", file=sys.stderr)
        raise


def calculate_sha256(file_path: Path) -> str:
    """Calculate SHA-256 hash of file.

    Args:
        file_path: Path to file

    Returns:
        Hexadecimal SHA-256 hash
    """
    sha256_hash = hashlib.sha256()
    with open(file_path, "rb") as f:
        for byte_block in iter(lambda: f.read(4096), b""):
            sha256_hash.update(byte_block)
    return sha256_hash.hexdigest()


def update_fixtures():
    """Update all fixture files from installed langflow package."""
    langflow_version = get_langflow_version()
    lfx_info = get_lfx_info()

    print(f"Using langflow version: {langflow_version}")
    print(f"Using lfx version: {lfx_info['version']} (source: {lfx_info['source']})")
    if lfx_info.get("has_custom_module") is False:
        print(
            "⚠️  WARNING: lfx installation is missing custom module",
            file=sys.stderr,
        )
        print(
            "   This may cause test failures. Ensure lfx is properly installed.",
            file=sys.stderr,
        )

    try:
        starter_projects_dir = get_starter_projects_dir()
        print(f"Source directory: {starter_projects_dir}")
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        return False

    # Track results
    results = {
        "updated_at": datetime.now(timezone.utc).isoformat(),
        "langflow_version": langflow_version,
        "lfx": lfx_info,
        "source": "installed_package",
        "files": {},
    }

    # Copy each fixture
    fixture_dir = Path(__file__).parent
    for local_name, source_name in FIXTURES.items():
        print(f"\nCopying {source_name}...")
        source_path = starter_projects_dir / source_name

        try:
            if not source_path.exists():
                raise FileNotFoundError(f"Source file not found: {source_path}")

            # Copy file
            dest_path = fixture_dir / local_name
            shutil.copy2(source_path, dest_path)

            # Calculate hash
            sha256 = calculate_sha256(dest_path)
            size_bytes = dest_path.stat().st_size

            # Record metadata
            results["files"][local_name] = {
                "source_name": source_name,
                "sha256": sha256,
                "size_bytes": size_bytes,
            }

            print(f"  ✓ Updated {local_name}")
            print(f"    SHA-256: {sha256}")
            print(f"    Size: {size_bytes:,} bytes")

        except Exception as e:
            print(f"  ✗ Failed to update {local_name}: {e}", file=sys.stderr)
            results["files"][local_name] = {
                "source_name": source_name,
                "error": str(e),
            }

    # Write tracking metadata
    metadata_path = fixture_dir / "LAST_UPDATED.json"
    metadata_path.write_text(json.dumps(results, indent=2) + "\n")
    print(f"\n✓ Wrote metadata to {metadata_path.name}")

    # Print summary
    successful = sum(1 for f in results["files"].values() if "error" not in f)
    failed = len(results["files"]) - successful
    print(f"\nSummary: {successful} successful, {failed} failed")

    return failed == 0


if __name__ == "__main__":
    try:
        success = update_fixtures()
        sys.exit(0 if success else 1)
    except KeyboardInterrupt:
        print("\n\nAborted by user", file=sys.stderr)
        sys.exit(130)
    except Exception as e:
        print(f"\nFatal error: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)
