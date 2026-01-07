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

"""
Example of using the Stepflow Python SDK to create loop-based components
that demonstrate the new flow evaluation capabilities.

This demonstrates:
1. Using evaluate_flow for nested workflow execution
2. Implementing iterate logic in Python components
3. Implementing map logic in Python components
4. Working with complex flow control structures
"""

import asyncio
import sys

from stepflow_py import StepflowServer, StepflowContext
import msgspec
from typing import List, Any, Dict

# Create the server
server = StepflowServer()


# Input/Output types for iterate component
class IterateInput(msgspec.Struct):
    """Input for the iterate component."""

    flow_id: str
    """Blob ID of the flow to apply for each iteration. Must return either {"result": value} or {"next": value}."""

    initial_input: Any  # Initial input to the workflow
    max_iterations: int = 1000  # Safety limit


class IterateOutput(msgspec.Struct):
    """Output from the iterate component."""

    result: Any  # Final result value
    iterations: int  # Number of iterations performed
    terminated: bool  # Whether terminated by max_iterations


# Input/Output types for map component
class MapInput(msgspec.Struct):
    """Input for the map component."""

    flow_id: str
    """Blob ID of the flow to apply to each item."""

    items: List[Any]  # Items to process


class MapOutput(msgspec.Struct):
    """Output from the map component."""

    results: List[Any]  # Results from processing each item


@server.component
async def iterate(input: IterateInput, context: StepflowContext) -> IterateOutput:
    """
    Iteratively apply a workflow until it returns a result instead of next.

    The workflow must return either {"result": value} or {"next": value}.
    If any iteration is skipped or fails, the whole iterate component will be skipped or failed.
    """
    current_input = input.initial_input
    iterations = 0

    while iterations < input.max_iterations:
        # Evaluate the workflow using the blob ID - exceptions will propagate and cause the whole iterate to skip/fail
        result_value = await context.evaluate_flow_by_id(input.flow_id, current_input)
        iterations += 1

        # Check for "result" field (termination)
        if isinstance(result_value, dict) and "result" in result_value:
            return IterateOutput(
                result=result_value["result"],
                iterations=iterations,
                terminated=False,
            )

        # Check for "next" field (continue iteration)
        elif isinstance(result_value, dict) and "next" in result_value:
            current_input = result_value["next"]
            continue

        # Result doesn't have expected structure
        else:
            from stepflow_py.exceptions import StepflowValueError

            raise StepflowValueError(
                "Iteration flow output must contain either 'result' or 'next' field"
            )

    # Max iterations reached
    return IterateOutput(result=current_input, iterations=iterations, terminated=True)


@server.component
async def map(input: MapInput, context: StepflowContext) -> MapOutput:
    """
    Apply a workflow to each item in a list and collect the results.

    Uses the unified runs API for efficient parallel processing.
    If any item is skipped or fails, the whole map component will be skipped or failed.
    """
    if not input.items:
        return MapOutput(results=[])

    # Use unified runs API with flow_id directly (no need to convert to Flow object)
    results = await context.evaluate_run_by_id(input.flow_id, input.items)

    return MapOutput(results=results)


# Example helper components for testing
class CounterInput(msgspec.Struct):
    """Input for counter example."""

    current: int
    target: int


@server.component
def counter_step(input: CounterInput) -> Dict[str, Any]:
    """
    Example component that counts towards a target.
    Returns 'next' to continue or 'result' when done.
    """
    if input.current >= input.target:
        return {"result": f"Reached target {input.target}"}
    else:
        return {"next": {"current": input.current + 1, "target": input.target}}


class DoubleInput(msgspec.Struct):
    """Input for double example."""

    value: float


@server.component
def double_value(input: DoubleInput) -> Dict[str, Any]:
    """Example component that doubles a value."""
    return {"result": input.value * 2}


if __name__ == "__main__":
    print(
        f"Registered components: {list(server.get_components().keys())}",
        file=sys.stderr,
    )

    # Check if the component is actually registered
    components = server.get_components()
    print(f"'iterate' in components: {'iterate' in components}", file=sys.stderr)
    print(
        f"Component details: {components.get('iterate', 'NOT FOUND')}", file=sys.stderr
    )
    print(f"Server initialized: {server.is_initialized()}", file=sys.stderr)

    asyncio.run(server.run())
