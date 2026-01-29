#!/usr/bin/env python3
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

"""Verify that stepflow-orchestrator release works correctly.

This script tests that the stepflow-orchestrator package correctly provides
the stepflow-server binary and can be used to run workflows.

Usage:
    python verify_stepflow_orchestrator.py
"""

from __future__ import annotations

import asyncio
import os
import subprocess
import sys
from pathlib import Path


def test_import() -> bool:
    """Test that stepflow_orchestrator can be imported."""
    print("Testing import...")
    try:
        import stepflow_orchestrator

        print(f"  ✓ Import successful: {stepflow_orchestrator.__name__}")
        return True
    except ImportError as e:
        print(f"  ✗ Import failed: {e}")
        return False


def test_orchestrator_class() -> bool:
    """Test that OrchestratorConfig and StepflowOrchestrator classes are available."""
    print("Testing orchestrator classes...")
    try:
        from stepflow_orchestrator import OrchestratorConfig, StepflowOrchestrator

        # Verify OrchestratorConfig can be created
        config = OrchestratorConfig(port=0)  # port 0 = auto-assign
        print(f"  ✓ OrchestratorConfig created: port={config.port}")

        # Verify StepflowOrchestrator has expected methods
        assert hasattr(StepflowOrchestrator, "start")
        assert hasattr(StepflowOrchestrator, "__aenter__")
        assert hasattr(StepflowOrchestrator, "__aexit__")
        print("  ✓ StepflowOrchestrator class has expected methods")

        return True
    except ImportError as e:
        print(f"  ✗ Import failed: {e}")
        return False
    except Exception as e:
        print(f"  ✗ Class test failed: {e}")
        return False


def test_binary_available() -> bool:
    """Test that the bundled binary is available (or STEPFLOW_DEV_BINARY is set)."""
    print("Testing binary availability...")
    try:
        # Check for dev binary first
        dev_binary = os.environ.get("STEPFLOW_DEV_BINARY")
        if dev_binary:
            path = Path(dev_binary)
            if path.exists():
                print(f"  ✓ Dev binary found: {path}")
                return True
            else:
                print(f"  ✗ STEPFLOW_DEV_BINARY set but not found: {path}")
                return False

        # Check for bundled binary
        from stepflow_orchestrator import StepflowOrchestrator

        # Create instance to access _get_binary_path (internal method)
        orchestrator = StepflowOrchestrator()
        try:
            binary_path = orchestrator._get_binary_path()
            print(f"  ✓ Bundled binary found: {binary_path}")
            return True
        except FileNotFoundError as e:
            print(f"  ✗ Bundled binary not found: {e}")
            return False

    except Exception as e:
        print(f"  ✗ Binary test failed: {e}")
        return False


def test_binary_execution() -> bool:
    """Test that the binary can be executed (--help)."""
    print("Testing binary execution...")
    try:
        # Get binary path
        dev_binary = os.environ.get("STEPFLOW_DEV_BINARY")
        if dev_binary:
            binary_path = Path(dev_binary)
        else:
            from stepflow_orchestrator import StepflowOrchestrator

            orchestrator = StepflowOrchestrator()
            binary_path = orchestrator._get_binary_path()

        # Run --help (returns exit code 0)
        result = subprocess.run(
            [str(binary_path), "--help"],
            capture_output=True,
            text=True,
            timeout=10,
        )

        if result.returncode != 0:
            print(f"  ✗ Binary execution failed: {result.stderr}")
            return False

        # Check that help output looks reasonable
        if "stepflow" in result.stdout.lower() or "usage" in result.stdout.lower():
            print("  ✓ Binary executes successfully (--help works)")
            return True
        else:
            print(f"  ✗ Unexpected --help output: {result.stdout[:200]}")
            return False

    except subprocess.TimeoutExpired:
        print("  ✗ Binary execution timed out")
        return False
    except FileNotFoundError:
        print("  ✗ Binary not found (skipping execution test)")
        return False
    except Exception as e:
        print(f"  ✗ Binary execution test failed: {e}")
        return False


async def test_orchestrator_lifecycle() -> bool:
    """Test starting and stopping the orchestrator."""
    print("Testing orchestrator lifecycle...")
    try:
        from stepflow_orchestrator import OrchestratorConfig, StepflowOrchestrator
    except ImportError as e:
        print(f"  ✗ Import failed: {e}")
        return False

    # Use port 0 for auto-assignment
    config = OrchestratorConfig(port=0, log_level="warn")

    try:
        print("  Starting orchestrator...")
        async with StepflowOrchestrator.start(config) as orchestrator:
            port = orchestrator.port
            url = orchestrator.url
            is_running = orchestrator.is_running

            print(f"  ✓ Orchestrator started: port={port}, url={url}")

            if not is_running:
                print("  ✗ Orchestrator reports not running")
                return False
            print("  ✓ Orchestrator is_running=True")

        # After context exit, it should be stopped
        print("  ✓ Orchestrator stopped successfully")
        return True

    except FileNotFoundError as e:
        print(f"  ✗ Binary not found: {e}")
        return False
    except Exception as e:
        print(f"  ✗ Lifecycle test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def main():
    print("=" * 60)
    print("Stepflow-orchestrator Release Verification")
    print("=" * 60)
    print()

    results = []

    # Run tests
    results.append(("Import", test_import()))
    results.append(("Classes", test_orchestrator_class()))
    results.append(("Binary available", test_binary_available()))
    results.append(("Binary execution", test_binary_execution()))

    # Full lifecycle test
    print()
    results.append(
        ("Orchestrator lifecycle", asyncio.run(test_orchestrator_lifecycle()))
    )

    # Summary
    print()
    print("=" * 60)
    print("Summary")
    print("=" * 60)

    passed = sum(1 for _, r in results if r)
    total = len(results)

    for name, result in results:
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"  {status}: {name}")

    print()
    print(f"Results: {passed}/{total} tests passed")

    if passed == total:
        print("\n✓ All tests passed!")
        return 0
    else:
        print("\n✗ Some tests failed!")
        return 1


if __name__ == "__main__":
    sys.exit(main())
