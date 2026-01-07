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
Test Python server for HTTP transport integration testing.

This server provides both bidirectional (context-using) and non-bidirectional components
to test different aspects of the HTTP transport layer.
"""

import argparse
import asyncio
import json
import sys
from typing import List, Dict, Any, Optional
import time
import uuid

try:
    from stepflow_py.server import StepflowServer
    from stepflow_py.http_server import StepflowHttpServer
    from stepflow_py import StepflowContext
    from stepflow_py.exceptions import StepflowExecutionError
    import msgspec
except ImportError:
    print("Error: This test requires the Python SDK", file=sys.stderr)
    print("Please install with: pip install stepflow-py[http]", file=sys.stderr)
    sys.exit(1)


# Input/Output Types for Non-Bidirectional Components


class EchoInput(msgspec.Struct):
    message: str


class EchoOutput(msgspec.Struct):
    echo: str
    timestamp: str


class MathInput(msgspec.Struct):
    a: float
    b: float
    operation: str  # "add", "subtract", "multiply", "divide"


class MathOutput(msgspec.Struct):
    result: float
    operation_performed: str


class ProcessListInput(msgspec.Struct):
    items: List[str]
    prefix: str = "processed"


class ProcessListOutput(msgspec.Struct):
    processed_items: List[str]
    count: int


# Input/Output Types for Error Testing Components


class ErrorTestInput(msgspec.Struct):
    mode: str  # "error", "success", "runtime_error"
    message: str = ""


class ErrorTestOutput(msgspec.Struct):
    result: str


# Input/Output Types for Bidirectional Components


class DataAnalysisInput(msgspec.Struct):
    data: List[float]
    analysis_type: str = "statistics"  # "statistics", "distribution"


class DataAnalysisOutput(msgspec.Struct):
    analysis_blob_id: str
    summary: str


class ChainProcessingInput(msgspec.Struct):
    initial_data: Dict[str, Any]
    processing_steps: List[str]


class ChainProcessingOutput(msgspec.Struct):
    final_result: Dict[str, Any]
    intermediate_blob_ids: List[str]


class MultiStepWorkflowInput(msgspec.Struct):
    workflow_config: Dict[str, Any]
    input_data: Any


class MultiStepWorkflowOutput(msgspec.Struct):
    workflow_result_blob_id: str
    step_count: int
    execution_time_ms: float


# Non-Bidirectional Components (no StepflowContext parameter)


def echo_component(input: EchoInput) -> EchoOutput:
    """Simple echo component that returns the input message with a timestamp."""
    return EchoOutput(
        echo=f"Echo: {input.message}", timestamp=time.strftime("%Y-%m-%d %H:%M:%S")
    )


def math_component(input: MathInput) -> MathOutput:
    """Perform basic mathematical operations."""
    operations = {
        "add": lambda a, b: a + b,
        "subtract": lambda a, b: a - b,
        "multiply": lambda a, b: a * b,
        "divide": lambda a, b: a / b if b != 0 else float("inf"),
    }

    if input.operation not in operations:
        raise ValueError(f"Unsupported operation: {input.operation}")

    result = operations[input.operation](input.a, input.b)

    return MathOutput(
        result=result,
        operation_performed=f"{input.a} {input.operation} {input.b} = {result}",
    )


def process_list_component(input: ProcessListInput) -> ProcessListOutput:
    """Process a list of items by adding a prefix to each."""
    processed_items = [f"{input.prefix}_{item}" for item in input.items]

    return ProcessListOutput(
        processed_items=processed_items, count=len(processed_items)
    )


def error_test_component(input: ErrorTestInput) -> ErrorTestOutput:
    """Test component that can raise different types of errors based on mode.

    Modes:
    - "error": Raises a StepflowExecutionError (expected error)
    - "runtime_error": Raises a RuntimeError (unexpected error, gets wrapped)
    - "success": Returns successfully
    """
    if input.mode == "error":
        raise StepflowExecutionError(f"Intentional error: {input.message}")
    elif input.mode == "runtime_error":
        raise RuntimeError(f"Unexpected runtime error: {input.message}")
    return ErrorTestOutput(result="success")


# Bidirectional Components (using StepflowContext for blob operations)


async def data_analysis_component(
    input: DataAnalysisInput, context: StepflowContext
) -> DataAnalysisOutput:
    """Analyze data and store detailed results as a blob."""

    print(
        f"Received data for analysis: {input.data} with analysis type: {input.analysis_type}",
        file=sys.stderr,
    )
    if input.analysis_type == "statistics":
        # Calculate basic statistics
        data = input.data
        analysis_result = {
            "type": "statistics",
            "count": len(data),
            "sum": sum(data),
            "mean": sum(data) / len(data) if data else 0,
            "min": min(data) if data else None,
            "max": max(data) if data else None,
            "sorted_data": sorted(data),
            "analysis_timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        }

        if len(data) > 1:
            mean = analysis_result["mean"]
            variance = sum((x - mean) ** 2 for x in data) / len(data)
            analysis_result["variance"] = variance
            analysis_result["std_dev"] = variance**0.5

    elif input.analysis_type == "distribution":
        # Create distribution analysis
        data = input.data
        analysis_result = {
            "type": "distribution",
            "histogram": _create_histogram(data, 10),
            "quartiles": _calculate_quartiles(data),
            "outliers": _find_outliers(data),
            "analysis_timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        }
    else:
        raise ValueError(f"Unsupported analysis type: {input.analysis_type}")

    # Store detailed analysis as blob
    analysis_blob_id = await context.put_blob(analysis_result)

    # Create summary
    if input.analysis_type == "statistics":
        summary = f"Statistics for {len(input.data)} data points: mean={analysis_result['mean']:.2f}, std_dev={analysis_result.get('std_dev', 0):.2f}"
    else:
        summary = f"Distribution analysis for {len(input.data)} data points with {len(analysis_result['outliers'])} outliers"

    return DataAnalysisOutput(analysis_blob_id=analysis_blob_id, summary=summary)


async def chain_processing_component(
    input: ChainProcessingInput, context: StepflowContext
) -> ChainProcessingOutput:
    """Process data through a chain of operations, storing intermediate results as blobs."""

    current_data = input.initial_data.copy()
    intermediate_blob_ids = []

    for i, step in enumerate(input.processing_steps):
        # Apply processing step
        if step == "uppercase_strings":
            for key, value in current_data.items():
                if isinstance(value, str):
                    current_data[key] = value.upper()

        elif step == "multiply_numbers":
            for key, value in current_data.items():
                if isinstance(value, (int, float)):
                    current_data[key] = value * 2

        elif step == "add_metadata":
            current_data["_metadata"] = {
                "step": i + 1,
                "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
                "step_name": step,
            }

        elif step == "filter_positive":
            current_data = {
                k: v
                for k, v in current_data.items()
                if not isinstance(v, (int, float)) or v >= 0
            }

        else:
            # Custom step: add step identifier
            current_data[f"custom_step_{i}"] = f"Applied: {step}"

        # Store intermediate result as blob
        intermediate_state = {
            "step_index": i,
            "step_name": step,
            "data": current_data.copy(),
            "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        }
        blob_id = await context.put_blob(intermediate_state)
        intermediate_blob_ids.append(blob_id)

    return ChainProcessingOutput(
        final_result=current_data, intermediate_blob_ids=intermediate_blob_ids
    )


async def multi_step_workflow_component(
    input: MultiStepWorkflowInput, context: StepflowContext
) -> MultiStepWorkflowOutput:
    """Execute a multi-step workflow with configurable operations."""

    start_time = time.time()

    workflow_config = input.workflow_config
    steps = workflow_config.get("steps", [])
    current_data = input.input_data

    workflow_log = {
        "workflow_id": str(uuid.uuid4()),
        "start_time": time.strftime("%Y-%m-%d %H:%M:%S"),
        "steps": [],
        "config": workflow_config,
    }

    for i, step_config in enumerate(steps):
        step_start = time.time()
        step_type = step_config.get("type", "unknown")
        step_params = step_config.get("params", {})

        step_log = {
            "step_index": i,
            "step_type": step_type,
            "step_params": step_params,
            "start_time": time.strftime("%Y-%m-%d %H:%M:%S"),
        }

        # Execute different step types
        if step_type == "transform":
            transformation = step_params.get("transformation", "identity")
            if transformation == "double":
                if isinstance(current_data, (int, float)):
                    current_data = current_data * 2
                elif isinstance(current_data, list):
                    current_data = [
                        x * 2 if isinstance(x, (int, float)) else x
                        for x in current_data
                    ]

        elif step_type == "aggregate":
            if isinstance(current_data, list):
                agg_type = step_params.get("type", "sum")
                if agg_type == "sum":
                    current_data = sum(
                        x for x in current_data if isinstance(x, (int, float))
                    )
                elif agg_type == "count":
                    current_data = len(current_data)
                elif agg_type == "max":
                    current_data = max(
                        x for x in current_data if isinstance(x, (int, float))
                    )

        elif step_type == "enrich":
            enrichment = step_params.get("enrichment", {})
            if isinstance(current_data, dict):
                current_data.update(enrichment)
            else:
                current_data = {"original_data": current_data, **enrichment}

        step_end = time.time()
        step_log.update(
            {
                "end_time": time.strftime("%Y-%m-%d %H:%M:%S"),
                "duration_ms": (step_end - step_start) * 1000,
                "result_type": type(current_data).__name__,
            }
        )

        workflow_log["steps"].append(step_log)

    end_time = time.time()
    execution_time_ms = (end_time - start_time) * 1000

    workflow_log.update(
        {
            "end_time": time.strftime("%Y-%m-%d %H:%M:%S"),
            "total_duration_ms": execution_time_ms,
            "final_result": current_data,
        }
    )

    # Store workflow execution log as blob
    workflow_result_blob_id = await context.put_blob(workflow_log)

    return MultiStepWorkflowOutput(
        workflow_result_blob_id=workflow_result_blob_id,
        step_count=len(steps),
        execution_time_ms=execution_time_ms,
    )


# Helper functions


def _create_histogram(data: List[float], bins: int) -> Dict[str, int]:
    """Create a simple histogram."""
    if not data:
        return {}

    min_val, max_val = min(data), max(data)
    if min_val == max_val:
        return {str(min_val): len(data)}

    bin_width = (max_val - min_val) / bins
    histogram = {}

    for value in data:
        bin_index = min(int((value - min_val) / bin_width), bins - 1)
        bin_start = min_val + bin_index * bin_width
        bin_end = bin_start + bin_width
        bin_key = f"{bin_start:.2f}-{bin_end:.2f}"
        histogram[bin_key] = histogram.get(bin_key, 0) + 1

    return histogram


def _calculate_quartiles(data: List[float]) -> Dict[str, float]:
    """Calculate quartiles for the data."""
    if not data:
        return {}

    sorted_data = sorted(data)
    n = len(sorted_data)

    return {
        "q1": sorted_data[n // 4] if n >= 4 else sorted_data[0],
        "median": sorted_data[n // 2],
        "q3": sorted_data[3 * n // 4] if n >= 4 else sorted_data[-1],
    }


def _find_outliers(data: List[float]) -> List[float]:
    """Find outliers using IQR method."""
    if len(data) < 4:
        return []

    quartiles = _calculate_quartiles(data)
    q1, q3 = quartiles["q1"], quartiles["q3"]
    iqr = q3 - q1

    lower_bound = q1 - 1.5 * iqr
    upper_bound = q3 + 1.5 * iqr

    return [x for x in data if x < lower_bound or x > upper_bound]


async def main():
    parser = argparse.ArgumentParser(
        description="Test Python Server for HTTP Transport"
    )
    parser.add_argument("--port", type=int, default=0, help="HTTP port (0 for auto-assign)")
    parser.add_argument("--host", type=str, default="127.0.0.1", help="HTTP host")

    args = parser.parse_args()

    # Create core server instance
    core_server = StepflowServer()

    # Register non-bidirectional components
    core_server.component(
        echo_component,
        name="echo",
        description="Echo component that returns the input message with timestamp",
    )

    core_server.component(
        math_component,
        name="math",
        description="Basic mathematical operations component",
    )

    core_server.component(
        process_list_component,
        name="process_list",
        description="Process a list of items by adding a prefix",
    )

    core_server.component(
        error_test_component,
        name="error_test",
        description="Test component for error handling scenarios",
    )

    # Register bidirectional components
    core_server.component(
        data_analysis_component,
        name="data_analysis",
        description="Analyze data and store detailed results as a blob",
    )

    core_server.component(
        chain_processing_component,
        name="chain_processing",
        description="Process data through a chain of operations with intermediate blob storage",
    )

    core_server.component(
        multi_step_workflow_component,
        name="multi_step_workflow",
        description="Execute a configurable multi-step workflow with blob storage",
    )

    # Create HTTP server (always runs in HTTP mode now)
    http_server = StepflowHttpServer(core_server, host=args.host, port=args.port)
    print(f"Starting HTTP test server on {args.host}:{args.port}", file=sys.stderr)
    await http_server.run()


if __name__ == "__main__":
    asyncio.run(main())
