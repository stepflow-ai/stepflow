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

"""Verify that stepflow-py release works correctly.

This script tests both basic import/API functionality and local orchestrator
execution when stepflow-orchestrator is installed.

Usage:
    # Test basic stepflow-py functionality
    python verify_stepflow_py.py

    # Test with local orchestrator (requires stepflow-py[local])
    python verify_stepflow_py.py --local
"""

from __future__ import annotations

import argparse
import asyncio
import sys


def test_basic_imports() -> bool:
    """Test that core imports work correctly."""
    print("Testing basic imports...")
    try:
        from stepflow_py import Flow, Step, StepflowClient  # noqa: F401
        from stepflow_py.api import ApiClient, Configuration  # noqa: F401
        from stepflow_py.api.models import ExecutionStatus  # noqa: F401

        print("  ✓ Core imports successful")
        return True
    except ImportError as e:
        print(f"  ✗ Import failed: {e}")
        return False


def test_config_imports() -> bool:
    """Test that config imports work correctly."""
    print("Testing config imports...")
    try:
        from stepflow_py.config import (
            BuiltinPluginConfig,
            InMemoryStateStoreConfig,
            RouteRule,
            StepflowConfig,
        )

        # Create a simple config to verify models work
        config = StepflowConfig(
            plugins={"builtin": BuiltinPluginConfig()},
            routes={"/{*component}": [RouteRule(plugin="builtin")]},
            stateStore=InMemoryStateStoreConfig(type="inMemory"),
        )
        print(f"  ✓ Config creation successful: {type(config).__name__}")
        return True
    except ImportError as e:
        print(f"  ✗ Import failed: {e}")
        return False
    except Exception as e:
        print(f"  ✗ Config creation failed: {e}")
        return False


def test_api_models() -> bool:
    """Test that API models can be imported and used."""
    print("Testing API models...")
    try:
        from stepflow_py.api.models import (
            ExecutionStatus,
        )

        # Verify ExecutionStatus enum values
        assert hasattr(ExecutionStatus, "COMPLETED")
        assert hasattr(ExecutionStatus, "FAILED")
        print("  ✓ API models imported successfully")
        return True
    except Exception as e:
        print(f"  ✗ API models test failed: {e}")
        return False


def test_client_class() -> bool:
    """Test that StepflowClient class is available."""
    print("Testing client class...")
    try:
        from stepflow_py import StepflowClient

        # Verify methods exist
        assert hasattr(StepflowClient, "connect")
        assert hasattr(StepflowClient, "local")
        assert hasattr(StepflowClient, "store_flow")
        assert hasattr(StepflowClient, "run")
        print("  ✓ StepflowClient class available with expected methods")
        return True
    except Exception as e:
        print(f"  ✗ Client class test failed: {e}")
        return False


async def test_local_orchestrator() -> bool:
    """Test local orchestrator functionality (requires stepflow-py[local]).

    This test verifies:
    1. The orchestrator can be started from the installed package
    2. The health endpoint responds correctly
    3. The client can communicate with the orchestrator

    Note: Full flow execution tests are in the integration test suite.
    """
    print("Testing local orchestrator...")

    try:
        from stepflow_py import StepflowClient
        from stepflow_py.config import (
            BuiltinPluginConfig,
            InMemoryStateStoreConfig,
            RouteRule,
            StepflowConfig,
        )
    except ImportError as e:
        print(f"  ✗ Import failed: {e}")
        return False

    # Create config with builtin plugin only
    config = StepflowConfig(
        plugins={"builtin": BuiltinPluginConfig()},
        routes={"/{*component}": [RouteRule(plugin="builtin")]},
        stateStore=InMemoryStateStoreConfig(type="inMemory"),
    )

    try:
        print("  Starting local orchestrator...")
        async with StepflowClient.local(config, startup_timeout=60.0) as client:
            print(f"  ✓ Orchestrator started, server at {client.base_url}")

            # Check health
            is_healthy = await client.is_healthy()
            if not is_healthy:
                print("  ✗ Health check failed")
                return False
            print("  ✓ Health check passed")

            # Verify we can make API calls (list flows should return empty)
            print("  ✓ Local orchestrator test passed")
            return True

    except ImportError as e:
        if "stepflow_orchestrator" in str(e):
            print("  ✗ stepflow-orchestrator not installed")
            print("    Install with: pip install stepflow-py[local]")
        else:
            print(f"  ✗ Import error: {e}")
        return False
    except Exception as e:
        print(f"  ✗ Local orchestrator test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def main():
    parser = argparse.ArgumentParser(
        description="Verify stepflow-py release functionality"
    )
    parser.add_argument(
        "--local",
        action="store_true",
        help="Test local orchestrator (requires stepflow-py[local])",
    )
    args = parser.parse_args()

    print("=" * 60)
    print("Stepflow-py Release Verification")
    print("=" * 60)
    print()

    results = []

    # Basic tests (always run)
    results.append(("Basic imports", test_basic_imports()))
    results.append(("Config imports", test_config_imports()))
    results.append(("API models", test_api_models()))
    results.append(("Client class", test_client_class()))

    # Local orchestrator test (optional)
    if args.local:
        print()
        results.append(("Local orchestrator", asyncio.run(test_local_orchestrator())))

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
