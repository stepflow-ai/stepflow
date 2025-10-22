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
Simple Component Server for Kubernetes Batch Demo

Provides basic mathematical and test components for demonstrating
distributed component execution in Kubernetes.
"""

from stepflow_py import StepflowServer, StepflowContext
import msgspec
import logging
import time
import os
import uuid

# Get logger - configuration is handled by SDK's setup_observability()
logger = logging.getLogger(__name__)

# Generate instance ID for load balancer routing
POD_NAME = os.getenv("POD_NAME", "local")
STARTUP_UUID = uuid.uuid4().hex[:8]
INSTANCE_ID = f"{POD_NAME}-{STARTUP_UUID}"

logger.info(f"Component server starting with instance ID: {INSTANCE_ID}")

# Create the component server
server = StepflowServer()

# Define component schemas using msgspec

class NumberInput(msgspec.Struct):
    """Input with a single number"""
    value: float

class NumberOutput(msgspec.Struct):
    """Output with a single result number"""
    result: float

class TwoNumberInput(msgspec.Struct):
    """Input with two numbers"""
    a: float
    b: float

class ProcessingInput(msgspec.Struct):
    """Input for compute-intensive tasks"""
    value: float
    iterations: int = 1000

class ProcessingOutput(msgspec.Struct):
    """Output from compute-intensive tasks"""
    result: float
    processing_time_ms: float

# Register components

@server.component
def double(input: NumberInput) -> NumberOutput:
    """Double the input value"""
    logger.info(f"double: {input.value} -> {input.value * 2}")
    return NumberOutput(result=input.value * 2)

@server.component
def square(input: NumberInput) -> NumberOutput:
    """Square the input value"""
    logger.info(f"square: {input.value} -> {input.value ** 2}")
    return NumberOutput(result=input.value ** 2)

@server.component
def add(input: TwoNumberInput) -> NumberOutput:
    """Add two numbers"""
    result = input.a + input.b
    logger.info(f"add: {input.a} + {input.b} = {result}")
    return NumberOutput(result=result)

@server.component
def multiply(input: TwoNumberInput) -> NumberOutput:
    """Multiply two numbers"""
    result = input.a * input.b
    logger.info(f"multiply: {input.a} * {input.b} = {result}")
    return NumberOutput(result=result)

@server.component
def compute_intensive_task(input: ProcessingInput) -> ProcessingOutput:
    """
    Simulate compute-intensive work.
    Useful for demonstrating load distribution and scaling.
    """
    start_time = time.time()

    # Simulate work with some computation
    result = input.value
    for i in range(input.iterations):
        result = (result * 1.1 + 0.5) % 1000000

    processing_time = (time.time() - start_time) * 1000

    logger.info(f"compute_intensive: processed {input.value} in {processing_time:.2f}ms")
    return ProcessingOutput(
        result=result,
        processing_time_ms=processing_time
    )

@server.component
def fibonacci(input: NumberInput) -> NumberOutput:
    """Calculate Fibonacci number (recursive, slower)"""
    def fib(n: int) -> int:
        if n <= 1:
            return n
        return fib(n - 1) + fib(n - 2)

    n = int(input.value)
    if n < 0:
        raise ValueError("Fibonacci requires non-negative integer")
    if n > 30:
        raise ValueError("Fibonacci input too large (max 30 for demo)")

    result = float(fib(n))
    logger.info(f"fibonacci: fib({n}) = {result}")
    return NumberOutput(result=result)

# Bidirectional component (uses StepflowContext for blob operations)
class BlobDataInput(msgspec.Struct):
    """Input with data to store as blob"""
    data: dict

class BlobDataOutput(msgspec.Struct):
    """Output with blob ID"""
    blob_id: str
    retrieved_data: dict

@server.component
async def store_and_retrieve_blob(input: BlobDataInput, context: StepflowContext) -> BlobDataOutput:
    """
    Bidirectional component that stores data as blob and retrieves it.
    This demonstrates instance affinity - responses must route back to same instance.
    """
    logger.info(f"[{INSTANCE_ID}] Storing blob with data: {input.data}")

    # Store data as blob (requires bidirectional communication)
    blob_id = await context.put_blob(input.data)
    logger.info(f"[{INSTANCE_ID}] Blob stored with ID: {blob_id}")

    # Retrieve the blob to verify
    retrieved_data = await context.get_blob(blob_id)
    logger.info(f"[{INSTANCE_ID}] Blob retrieved successfully")

    return BlobDataOutput(
        blob_id=blob_id,
        retrieved_data=retrieved_data
    )

# Main entry point
if __name__ == "__main__":
    import sys

    # Check if HTTP mode flag is passed
    if "--http" in sys.argv:
        logger.info(f"Starting component server in HTTP mode on port 8080")
        logger.info(f"Instance ID: {INSTANCE_ID}")

        # Import and start HTTP server with instance ID
        from stepflow_py.http_server import StepflowHttpServer
        import asyncio

        http_server = StepflowHttpServer(
            server=server,
            host="0.0.0.0",
            port=8080,
            instance_id=INSTANCE_ID
        )

        asyncio.run(http_server.run())
    else:
        logger.info("Starting component server in STDIO mode")
        logger.info(f"Instance ID: {INSTANCE_ID}")
        server.run()
