# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

"""
LangChain UDF component for executing LangChain runnables stored as blobs.

This extends the existing UDF pattern to support LangChain runnables that are
serialized and stored in StepFlow's blob system.
"""

from __future__ import annotations

import sys
from typing import Any, Dict

import msgspec

from stepflow_py.context import StepflowContext
from stepflow_py.exceptions import StepflowExecutionError
from stepflow_py.langchain_integration import (
    check_langchain_available,
    deserialize_runnable,
    execute_runnable,
    create_runnable_config,
    convert_stepflow_to_langchain_input,
)

__all__ = ["langchain_udf", "LangChainUdfInput", "create_langchain_runnable_blob"]

# Global cache for deserialized runnables by blob_id
_runnable_cache: Dict[str, Any] = {}


class LangChainUdfInput(msgspec.Struct):
    """Input structure for LangChain UDF component."""
    blob_id: str
    input: Any
    config: Dict[str, Any] | None = None


async def langchain_udf(input: LangChainUdfInput, context: StepflowContext) -> Any:
    """
    Execute a LangChain runnable stored as a blob.
    
    The blob should contain a dictionary with:
    - runnable_definition: Serialized LangChain runnable (from dumpd())
    - input_schema: JSON Schema for input validation (optional)
    - output_schema: JSON Schema for output validation (optional)
    - execution_mode: "invoke", "batch", or "stream" (default: "invoke")
    
    Args:
        input: Contains blob_id, input data, and optional config
        context: StepFlow context for blob operations
        
    Returns:
        The result of executing the LangChain runnable
    """
    check_langchain_available()

    # Check if we have a cached runnable for this blob_id
    if input.blob_id in _runnable_cache:
        print(f"Using cached LangChain runnable for blob_id: {input.blob_id}", file=sys.stderr)
        runnable_info = _runnable_cache[input.blob_id]
        runnable = runnable_info["runnable"]
        execution_mode = runnable_info.get("execution_mode", "invoke")
    else:
        print(f"Loading and deserializing LangChain runnable for blob_id: {input.blob_id}", file=sys.stderr)
        
        # Get the blob containing the runnable definition
        try:
            blob_data = await context.get_blob(input.blob_id)
        except Exception as e:
            raise StepflowExecutionError(f"Failed to retrieve blob {input.blob_id}: {e}") from e
        
        # Validate blob structure
        if not isinstance(blob_data, dict):
            raise StepflowExecutionError(f"Blob {input.blob_id} must contain a dictionary")
        
        runnable_definition = blob_data.get("runnable_definition")
        if not runnable_definition:
            raise StepflowExecutionError(
                f"Blob {input.blob_id} must contain 'runnable_definition' field"
            )
        
        # Extract optional fields
        input_schema = blob_data.get("input_schema")
        output_schema = blob_data.get("output_schema")
        execution_mode = blob_data.get("execution_mode", "invoke")
        
        # Validate execution mode
        if execution_mode not in ["invoke", "batch", "stream"]:
            raise StepflowExecutionError(
                f"Invalid execution_mode '{execution_mode}'. Must be 'invoke', 'batch', or 'stream'"
            )
        
        # Deserialize the runnable
        try:
            runnable = deserialize_runnable(runnable_definition)
        except Exception as e:
            raise StepflowExecutionError(
                f"Failed to deserialize LangChain runnable: {e}"
            ) from e
        
        # Validate input against schema if provided
        if input_schema:
            try:
                import jsonschema
                jsonschema.validate(input.input, input_schema)
            except ImportError:
                print("jsonschema not available, skipping input validation", file=sys.stderr)
            except jsonschema.ValidationError as e:
                raise StepflowExecutionError(f"Input validation failed: {e.message}") from e
            except jsonschema.SchemaError as e:
                raise StepflowExecutionError(f"Invalid input schema: {e.message}") from e
        
        # Cache the runnable and metadata
        _runnable_cache[input.blob_id] = {
            "runnable": runnable,
            "input_schema": input_schema,
            "output_schema": output_schema,
            "execution_mode": execution_mode,
        }
    
    # Prepare input for LangChain runnable
    langchain_input = input.input
    
    # Create runnable config from StepFlow input
    stepflow_input_dict = {
        "input": input.input,
        "config": input.config or {}
    }
    runnable_config = create_runnable_config(stepflow_input_dict, context)
    
    # Execute the runnable
    try:
        result = await execute_runnable(
            runnable,
            langchain_input,
            config=runnable_config,
            execution_mode=execution_mode
        )
        
        # Validate output against schema if provided
        cached_info = _runnable_cache[input.blob_id]
        output_schema = cached_info.get("output_schema")
        if output_schema:
            try:
                import jsonschema
                jsonschema.validate(result, output_schema)
            except ImportError:
                print("jsonschema not available, skipping output validation", file=sys.stderr)
            except jsonschema.ValidationError as e:
                raise StepflowExecutionError(f"Output validation failed: {e.message}") from e
            except jsonschema.SchemaError as e:
                raise StepflowExecutionError(f"Invalid output schema: {e.message}") from e
        
        print(f"LangChain runnable result: {result}", file=sys.stderr)
        return result
        
    except Exception as e:
        if isinstance(e, StepflowExecutionError):
            raise
        raise StepflowExecutionError(f"LangChain runnable execution failed: {e}") from e


def create_langchain_runnable_blob(
    runnable,
    input_schema: Dict[str, Any] | None = None,
    output_schema: Dict[str, Any] | None = None,
    execution_mode: str = "invoke"
) -> Dict[str, Any]:
    """
    Create a blob data structure for storing a LangChain runnable.
    
    This is a utility function to help create properly formatted blob data
    for use with the langchain_udf component.
    
    Args:
        runnable: The LangChain runnable to serialize
        input_schema: Optional JSON Schema for input validation
        output_schema: Optional JSON Schema for output validation
        execution_mode: Execution mode ("invoke", "batch", "stream")
        
    Returns:
        Dictionary ready to be stored as a blob
    """
    from stepflow_py.langchain_integration import serialize_runnable, get_runnable_schemas
    
    check_langchain_available()
    
    if execution_mode not in ["invoke", "batch", "stream"]:
        raise ValueError(f"Invalid execution_mode '{execution_mode}'. Must be 'invoke', 'batch', or 'stream'")
    
    # Serialize the runnable
    runnable_definition = serialize_runnable(runnable)
    
    # Auto-generate schemas if not provided
    if input_schema is None or output_schema is None:
        auto_input_schema, auto_output_schema = get_runnable_schemas(runnable)
        if input_schema is None:
            input_schema = auto_input_schema
        if output_schema is None:
            output_schema = auto_output_schema
    
    return {
        "runnable_definition": runnable_definition,
        "input_schema": input_schema,
        "output_schema": output_schema,
        "execution_mode": execution_mode,
    }