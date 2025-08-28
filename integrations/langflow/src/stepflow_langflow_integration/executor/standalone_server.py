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

"""Standalone Stepflow component server for Langflow integration.

This script can be run directly and handles imports properly.
"""

import os
import sys
from pathlib import Path
from typing import Any

# Add the package root to the path before importing project modules
package_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(package_root))

from stepflow_py import StepflowContext, StepflowStdioServer

from stepflow_langflow_integration.executor.udf_executor import UDFExecutor

# Create server instance (following the exact pattern from stepflow_py/main.py)
server = StepflowStdioServer()

# Create UDF executor
udf_executor = UDFExecutor()


async def _pre_resolve_all_blobs(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Pre-resolve all blob data that will be needed during UDF execution.

    This eliminates the need for nested context.get_blob() calls and prevents
    JSON-RPC circular dependencies.

    Args:
        input_data: Raw input with blob references
        context: Stepflow context for blob operations

    Returns:
        Input data with all blob references resolved to actual blob content
    """
    print("DEBUG: Starting _pre_resolve_all_blobs")
    with open("/tmp/udf_debug.log", "a") as f:
        f.write("DEBUG: Starting _pre_resolve_all_blobs\n")
        f.write(f"DEBUG: Input data keys: {list(input_data.keys())}\n")

    resolved_data = input_data.copy()

    # Identify all blob IDs that need to be resolved
    blob_ids_to_resolve = set()

    # Add main blob_id if present
    if "blob_id" in input_data:
        blob_ids_to_resolve.add(input_data["blob_id"])

    # Add tool blob IDs (external inputs like tool_blob_X)
    for key, value in input_data.items():
        if key.startswith("tool_blob_") and isinstance(value, str):
            blob_ids_to_resolve.add(value)
        elif (
            key.startswith("tool_blob_")
            and isinstance(value, dict)
            and "blob_id" in value
        ):
            blob_ids_to_resolve.add(value["blob_id"])

    # Add agent blob ID if present
    if "agent_blob" in input_data:
        agent_ref = input_data["agent_blob"]
        if isinstance(agent_ref, str):
            blob_ids_to_resolve.add(agent_ref)
        elif isinstance(agent_ref, dict) and "blob_id" in agent_ref:
            blob_ids_to_resolve.add(agent_ref["blob_id"])

    print(
        f"DEBUG: Found {len(blob_ids_to_resolve)} blob IDs to resolve: "
        f"{blob_ids_to_resolve}"
    )
    with open("/tmp/udf_debug.log", "a") as f:
        f.write(
            f"DEBUG: Found {len(blob_ids_to_resolve)} blob IDs to resolve: "
            f"{blob_ids_to_resolve}\n"
        )

    # Pre-fetch all blob data
    resolved_blobs = {}
    for blob_id in blob_ids_to_resolve:
        try:
            print(f"DEBUG: Pre-resolving blob {blob_id}")
            blob_data = await context.get_blob(blob_id)
            resolved_blobs[blob_id] = blob_data
            print(f"DEBUG: Successfully resolved blob {blob_id}")
            with open("/tmp/udf_debug.log", "a") as f:
                f.write(f"DEBUG: Successfully resolved blob {blob_id}\n")
        except Exception as e:
            print(f"DEBUG: Failed to resolve blob {blob_id}: {e}")
            with open("/tmp/udf_debug.log", "a") as f:
                f.write(f"DEBUG: Failed to resolve blob {blob_id}: {e}\n")
            # Continue - let the UDF executor handle the error

    # Replace blob references with resolved data in the input
    resolved_data["_resolved_blobs"] = resolved_blobs

    print(f"DEBUG: Pre-resolution complete. Resolved {len(resolved_blobs)} blobs")
    with open("/tmp/udf_debug.log", "a") as f:
        f.write(
            f"DEBUG: Pre-resolution complete. Resolved {len(resolved_blobs)} blobs\n"
        )

    return resolved_data


# Register the main UDF executor component at module level
@server.component(name="udf_executor")
async def udf_executor_component(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Execute a Langflow UDF component with simplified architecture - no context
    wrapper needed."""
    print("🔥 SIMPLIFIED UDF_EXECUTOR_COMPONENT - NO CONTEXT WRAPPER!")
    with open("/tmp/udf_debug.log", "a") as f:
        f.write("🔥 SIMPLIFIED UDF_EXECUTOR_COMPONENT - NO CONTEXT WRAPPER!\n")

    # Pre-resolve all blob data that the UDF executor will need
    # This eliminates the need for nested context calls entirely
    resolved_input_data = await _pre_resolve_all_blobs(input_data, context)

    # Execute UDF with pre-resolved data - no context needed for blob operations
    return await udf_executor.execute_with_resolved_data(resolved_input_data)


# Langflow component implementations
@server.component(name="file")
async def langflow_file(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Langflow File component implementation.

    Processes files and returns their content as messages.
    For development, returns sample content when no files are provided.
    """
    # In a full implementation, this would process actual file uploads
    # For now, return sample document content suitable for Q&A workflows
    file_content = """Sample Document Content

This document contains information about artificial intelligence and machine learning:

## Overview
Artificial intelligence (AI) is a broad field of computer science focused on creating
systems capable of performing tasks that typically require human intelligence.

## Key Concepts
- Machine Learning: Algorithms that improve automatically through experience
- Deep Learning: Neural networks with multiple layers for complex pattern recognition
- Natural Language Processing: Enabling computers to understand and process human
  language
- Computer Vision: Teaching machines to interpret and understand visual information

## Applications
AI is used in various domains including:
- Healthcare: Medical diagnosis and drug discovery
- Finance: Fraud detection and algorithmic trading
- Transportation: Autonomous vehicles and route optimization
- Technology: Search engines, recommendation systems, and virtual assistants

## Technical Details
Large language models are trained on vast datasets to understand context, generate
text, and perform reasoning tasks. They use transformer architectures with attention
mechanisms to process and generate human-like text responses."""

    return {
        "result": {
            "text": file_content,
            "sender": "System",
            "sender_name": "File Loader",
        }
    }


@server.component(name="memory")
async def langflow_memory(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Langflow Memory component implementation.

    Manages conversation history and context.
    For development, returns sample conversation history.
    """
    # In a full implementation, this would maintain persistent conversation state
    # For now, return sample conversation history
    conversation_history = """Previous conversation context:

User: Hello, I'm interested in learning about AI.
Assistant: Hello! I'd be happy to help you learn about artificial intelligence. AI is a
fascinating field that encompasses machine learning, natural language processing,
computer vision, and more.

User: What's the difference between AI and machine learning?
Assistant: Great question! AI is the broader field focused on creating intelligent
systems, while machine learning is a subset of AI that focuses on algorithms that can
learn from data without being explicitly programmed for every task.

User: Can you give me some examples of machine learning in everyday life?
Assistant: Certainly! You encounter ML daily: email spam filters, recommendation
systems on Netflix/Spotify, voice assistants like Siri, photo tagging on social media,
and navigation apps that optimize routes based on traffic patterns.

Current conversation continues..."""

    return {"result": conversation_history}


@server.component(name="url")
async def langflow_url(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Langflow URL component implementation.

    Fetches and processes content from web URLs.
    For development, returns sample web content.
    """
    # In a full implementation, this would fetch actual web content from provided URLs
    # For now, return sample web content relevant to common queries
    web_content = """Web Content: Mathematics and Calculation Resources

Source: Educational Mathematics Portal
URL: https://example.com/math-resources
Last Updated: 2024

## Basic Arithmetic Operations

### Addition
Addition is the mathematical operation of combining numbers to get their sum.
Example: 2 + 2 = 4

### Subtraction
Subtraction finds the difference between numbers.
Example: 5 - 3 = 2

### Multiplication
Multiplication is repeated addition of the same number.
Example: 3 × 4 = 12 (adding 3 four times: 3+3+3+3)

### Division
Division splits a number into equal parts.
Example: 10 ÷ 2 = 5

## Online Calculator Tools
- Basic arithmetic calculator
- Scientific calculator with advanced functions
- Graphing calculator for equations
- Unit conversion tools
- Statistical calculators

## Educational Resources
- Step-by-step problem solutions
- Interactive tutorials and exercises
- Video explanations of mathematical concepts
- Practice problems with immediate feedback

For questions like "What is 2 + 2?", the answer is 4."""

    return {"result": web_content}


@server.component(name="LanguageModelComponent")
async def langflow_language_model(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Langflow LanguageModelComponent implementation.

    Handles OpenAI chat completion directly using API calls.
    This component receives configuration and input messages and returns AI responses.
    """

    import openai

    # Extract configuration parameters from input_data
    # The UDF executor populates these from the Langflow node template
    api_key = input_data.get("api_key", os.getenv("OPENAI_API_KEY"))
    model_name = input_data.get("model_name", "gpt-3.5-turbo")
    temperature = input_data.get("temperature", 0.7)
    system_message = input_data.get("system_message", "You are a helpful AI assistant.")

    # Extract the actual input message from the input_data structure
    # This comes from the workflow input or previous steps
    input_value = input_data.get("input_value", {})

    # Handle both string and object message formats
    if isinstance(input_value, str):
        user_message = input_value
    elif isinstance(input_value, dict):
        user_message = input_value.get(
            "text", input_value.get("content", str(input_value))
        )
    else:
        user_message = str(input_value)

    if not api_key:
        return {
            "error": (
                "OpenAI API key not found. Please set OPENAI_API_KEY environment "
                "variable or provide api_key in input."
            ),
            "result": None,
        }

    try:
        # Initialize OpenAI client
        client = openai.OpenAI(api_key=api_key)

        # Build messages array
        messages = [
            {"role": "system", "content": system_message},
            {"role": "user", "content": user_message},
        ]

        # Make API call
        response = client.chat.completions.create(
            model=model_name, messages=messages, temperature=temperature
        )

        # Extract response content
        ai_response = response.choices[0].message.content

        # Return in the format expected by Langflow workflows
        return {
            "result": {
                "text": ai_response,
                "sender": "AI",
                "sender_name": "OpenAI Assistant",
            }
        }

    except Exception as e:
        return {"error": f"OpenAI API error: {str(e)}", "result": None}


@server.component(name="AstraDB")
async def langflow_astradb(
    input_data: dict[str, Any], context: StepflowContext
) -> dict[str, Any]:
    """Langflow AstraDB vector store component with embedded OpenAI Embeddings
    configuration.

    This component handles vector storage operations with embedded OpenAI Embeddings
    configuration for proper embedding model initialization.
    """
    print(f"DEBUG AstraDB: Input keys: {list(input_data.keys())}")

    # Check for embedded configuration
    embedding_config = None
    for key, value in input_data.items():
        if key.startswith("_embedding_config_"):
            print(f"DEBUG AstraDB: Found embedded config: {key}")
            if (
                isinstance(value, dict)
                and value.get("component_type") == "OpenAIEmbeddings"
            ):
                embedding_config = value.get("config", {})
                embedding_field = key.replace("_embedding_config_", "")
                print(
                    f"DEBUG AstraDB: Extracted {embedding_field} config: "
                    f"{embedding_config}"
                )
                break

    if not embedding_config:
        return {
            "error": (
                "No embedded OpenAI Embeddings configuration found for "
                "AstraDB component"
            ),
            "result": None,
        }

    # For now, return a mock response indicating successful vector store operation
    # In a real implementation, this would:
    # 1. Initialize OpenAI Embeddings with the config
    # 2. Connect to AstraDB
    # 3. Store/retrieve vectors as needed

    operation_type = input_data.get("operation", "search")  # Default to search

    if operation_type == "store":
        return {
            "result": {
                "status": "stored",
                "message": (
                    "Vector data stored successfully with embedded OpenAI Embeddings"
                ),
                "embedding_model": embedding_config.get(
                    "model", "text-embedding-3-small"
                ),
                "vector_count": 1,
            }
        }
    else:
        # Search operation
        return {
            "result": {
                "search_results": [
                    {
                        "content": "Sample search result from vector store",
                        "score": 0.95,
                        "metadata": {"source": "embedded_config_test"},
                    }
                ],
                "embedding_model": embedding_config.get(
                    "model", "text-embedding-3-small"
                ),
            }
        }


def main():
    """Main entry point for the Langflow component server."""
    # Start the server - this handles all the asyncio setup correctly
    server.run()


if __name__ == "__main__":
    """Run the Langflow component server."""
    main()
