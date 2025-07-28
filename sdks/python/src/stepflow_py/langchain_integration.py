# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The Apache licenses this
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
Core utilities for LangChain integration with StepFlow.
"""

from __future__ import annotations

import asyncio
from typing import Any, Dict, Optional, Type

# Import cache for runnable import paths
_import_cache: Dict[str, Any] = {}

try:
    from langchain_core.runnables import Runnable
    from langchain_core.runnables.config import RunnableConfig
    from langchain_core.load import load
    from langchain_core.load.dump import dumpd
    LANGCHAIN_AVAILABLE = True
except ImportError:
    LANGCHAIN_AVAILABLE = False
    Runnable = None
    RunnableConfig = None

import msgspec

from stepflow_py.context import StepflowContext
from stepflow_py.exceptions import StepflowExecutionError


def check_langchain_available():
    """Check if LangChain is available and raise error if not."""
    if not LANGCHAIN_AVAILABLE:
        raise StepflowExecutionError(
            "LangChain integration requires langchain-core. "
            "Install with: pip install stepflow-py[langchain]"
        )


def create_runnable_config(
    stepflow_input: Dict[str, Any], 
    context: Optional[StepflowContext] = None
) -> RunnableConfig:
    """
    Create a RunnableConfig from StepFlow input and context.
    
    Args:
        stepflow_input: The StepFlow component input
        context: Optional StepFlow context for runtime services
        
    Returns:
        RunnableConfig object for LangChain execution
    """
    check_langchain_available()
    
    config = RunnableConfig()
    
    # Extract LangChain-specific config from input
    if "config" in stepflow_input:
        config_data = stepflow_input["config"]
        
        # Map common config fields
        if "run_name" in config_data:
            config["run_name"] = config_data["run_name"]
        if "tags" in config_data:
            config["tags"] = config_data["tags"]
        if "metadata" in config_data:
            config["metadata"] = config_data["metadata"]
        if "max_concurrency" in config_data:
            config["max_concurrency"] = config_data["max_concurrency"]
    
    # Add StepFlow context to config for advanced use cases
    if context:
        if "metadata" not in config:
            config["metadata"] = {}
        config["metadata"]["stepflow_context"] = context
    
    return config


async def execute_runnable(
    runnable: Runnable,
    input_data: Any,
    config: Optional[RunnableConfig] = None,
    execution_mode: str = "invoke"
) -> Any:
    """
    Execute a LangChain runnable with proper async handling.
    
    Args:
        runnable: The LangChain runnable to execute
        input_data: Input data for the runnable
        config: Optional runnable configuration
        execution_mode: Execution mode ("invoke", "batch", "stream")
        
    Returns:
        The result of runnable execution
        
    Raises:
        StepflowExecutionError: If execution fails
    """
    check_langchain_available()
    
    try:
        if execution_mode == "invoke":
            # Use ainvoke if available, otherwise run invoke in thread pool
            if hasattr(runnable, 'ainvoke'):
                return await runnable.ainvoke(input_data, config=config)
            else:
                loop = asyncio.get_event_loop()
                return await loop.run_in_executor(
                    None, 
                    lambda: runnable.invoke(input_data, config=config)
                )
        
        elif execution_mode == "batch":
            # Handle batch execution
            if hasattr(runnable, 'abatch'):
                return await runnable.abatch(input_data, config=config)
            else:
                loop = asyncio.get_event_loop()
                return await loop.run_in_executor(
                    None,
                    lambda: runnable.batch(input_data, config=config)
                )
        
        elif execution_mode == "stream":
            # Handle streaming execution
            if hasattr(runnable, 'astream'):
                result = []
                async for chunk in runnable.astream(input_data, config=config):
                    result.append(chunk)
                return result
            else:
                # Fallback to sync stream in thread pool
                def sync_stream():
                    return list(runnable.stream(input_data, config=config))
                
                loop = asyncio.get_event_loop()
                return await loop.run_in_executor(None, sync_stream)
        
        else:
            raise StepflowExecutionError(f"Unsupported execution mode: {execution_mode}")
            
    except Exception as e:
        raise StepflowExecutionError(f"LangChain runnable execution failed: {str(e)}") from e


def serialize_runnable(runnable: Runnable) -> Dict[str, Any]:
    """
    Serialize a LangChain runnable to a dictionary for blob storage.
    
    Args:
        runnable: The runnable to serialize
        
    Returns:
        Dictionary representation of the runnable
    """
    check_langchain_available()
    
    try:
        return dumpd(runnable)
    except Exception as e:
        raise StepflowExecutionError(f"Failed to serialize runnable: {str(e)}") from e


def deserialize_runnable(runnable_data: Dict[str, Any]) -> Runnable:
    """
    Deserialize a LangChain runnable from dictionary data.
    
    Args:
        runnable_data: Dictionary representation of the runnable
        
    Returns:
        The deserialized runnable
    """
    check_langchain_available()
    
    try:
        return load(runnable_data)
    except Exception as e:
        raise StepflowExecutionError(f"Failed to deserialize runnable: {str(e)}") from e


def get_runnable_schemas(runnable: Runnable) -> tuple[Dict[str, Any], Dict[str, Any]]:
    """
    Extract input and output schemas from a LangChain runnable.
    
    Args:
        runnable: The runnable to inspect
        
    Returns:
        Tuple of (input_schema, output_schema) as JSON Schema dictionaries
    """
    check_langchain_available()
    
    try:
        # Get schemas using LangChain's schema methods
        input_schema = {}
        output_schema = {}
        
        # Try to get input schema
        if hasattr(runnable, 'get_input_schema'):
            try:
                input_schema = runnable.get_input_schema().model_json_schema()
            except Exception:
                # Fallback to generic schema
                input_schema = {"type": "object", "additionalProperties": True}
        else:
            input_schema = {"type": "object", "additionalProperties": True}
        
        # Try to get output schema  
        if hasattr(runnable, 'get_output_schema'):
            try:
                output_schema = runnable.get_output_schema().model_json_schema()
            except Exception:
                # Fallback to generic schema
                output_schema = {"type": "object", "additionalProperties": True}
        else:
            output_schema = {"type": "object", "additionalProperties": True}
        
        return input_schema, output_schema
        
    except Exception as e:
        # Return generic schemas on any error
        generic_schema = {"type": "object", "additionalProperties": True}
        return generic_schema, generic_schema


def convert_stepflow_to_langchain_input(stepflow_input: Dict[str, Any]) -> Any:
    """
    Convert StepFlow input format to LangChain runnable input.
    
    For most cases, this extracts the 'input' field from the StepFlow component input.
    The 'config' field is handled separately by create_runnable_config.
    
    Args:
        stepflow_input: The StepFlow component input
        
    Returns:
        The input data for the LangChain runnable
    """
    # If input has both 'input' and 'config' fields, extract just the input
    if isinstance(stepflow_input, dict) and "input" in stepflow_input:
        return stepflow_input["input"]
    
    # Otherwise, use the entire input as-is
    return stepflow_input


def get_runnable_from_import_path(import_path: str, use_cache: bool = True) -> Runnable:
    """
    Import and return a runnable from a Python import path, with optional caching.
    
    Args:
        import_path: Python import path like "mymodule.my_runnable" or "package.submodule:runnable_var"
        use_cache: Whether to use the import cache (default: True)
        
    Returns:
        The imported runnable object
        
    Raises:
        StepflowExecutionError: If import fails or object is not a runnable
    """
    check_langchain_available()
    
    # Check cache first
    if use_cache and import_path in _import_cache:
        return _import_cache[import_path]
    
    try:
        # Handle both "module.attribute" and "module:attribute" syntax
        if ":" in import_path:
            module_path, attr_name = import_path.split(":", 1)
        else:
            # Split on last dot for "module.attribute" syntax
            parts = import_path.split(".")
            if len(parts) < 2:
                raise StepflowExecutionError(f"Invalid import path format: {import_path}")
            module_path = ".".join(parts[:-1])
            attr_name = parts[-1]
        
        # Import the module
        import importlib
        try:
            module = importlib.import_module(module_path)
        except ImportError as e:
            raise StepflowExecutionError(f"Failed to import module '{module_path}': {str(e)}") from e
        
        # Get the attribute
        if not hasattr(module, attr_name):
            raise StepflowExecutionError(f"Module '{module_path}' has no attribute '{attr_name}'")
        
        runnable = getattr(module, attr_name)
        
        # Verify it's a runnable
        if not isinstance(runnable, Runnable):
            raise StepflowExecutionError(
                f"Object at '{import_path}' is not a LangChain Runnable (found {type(runnable).__name__})"
            )
        
        # Cache the runnable if caching is enabled
        if use_cache:
            _import_cache[import_path] = runnable
        
        return runnable
        
    except StepflowExecutionError:
        raise
    except Exception as e:
        raise StepflowExecutionError(f"Failed to import runnable from '{import_path}': {str(e)}") from e


def clear_import_cache() -> None:
    """Clear the import cache for runnables."""
    _import_cache.clear()


async def invoke_named_runnable(
    import_path: str,
    input_data: Any,
    execution_mode: str = "invoke",
    config: Optional[Dict[str, Any]] = None,
    context: Optional[StepflowContext] = None,
    use_cache: bool = True
) -> Any:
    """
    Directly invoke a runnable from a Python import path with caching.
    
    This is the core function that powers the /invoke_named component.
    It imports the runnable (with caching) and executes it directly.
    
    Args:
        import_path: Python import path like "mymodule.my_runnable"
        input_data: Input data for the runnable
        execution_mode: Execution mode ("invoke", "batch", "stream")
        config: Optional runnable configuration
        context: Optional StepFlow context for runtime services
        use_cache: Whether to use the import cache (default: True)
        
    Returns:
        The result of runnable execution
        
    Raises:
        StepflowExecutionError: If import or execution fails
    """
    check_langchain_available()
    
    try:
        # Import the runnable (with caching)
        runnable = get_runnable_from_import_path(import_path, use_cache=use_cache)
        
        # Create runnable config from input
        runnable_config = create_runnable_config(
            {"input": input_data, "config": config or {}}, 
            context
        )
        
        # Execute the runnable directly
        result = await execute_runnable(
            runnable=runnable,
            input_data=input_data,
            config=runnable_config,
            execution_mode=execution_mode
        )
        
        return result
        
    except Exception as e:
        raise StepflowExecutionError(f"Failed to invoke runnable from '{import_path}': {str(e)}") from e


import msgspec

class InvokeNamedInput(msgspec.Struct):
    """Input structure for the /invoke_named component."""
    import_path: str
    input: dict
    execution_mode: str = "invoke"
    config: dict | None = None


def create_invoke_named_component(server):
    """
    Create the /invoke_named component for a StepFlow server.
    
    This function adds the /invoke_named component to the provided server,
    allowing users to directly invoke runnables from import paths.
    
    Args:
        server: StepflowServer instance to add the component to
        
    Returns:
        The component function for registration
    """
    check_langchain_available()

    @server.component
    async def invoke_named(
        input: InvokeNamedInput,
        context: StepflowContext
    ) -> dict:
        """Directly invoke a runnable from Python import path with caching."""
        
        result = await invoke_named_runnable(
            import_path=input.import_path,
            input_data=input.input,
            execution_mode=input.execution_mode,
            config=input.config,
            context=context,
            use_cache=True
        )
        
        return {
            "import_path": input.import_path,
            "result": result
        }
    
    return invoke_named


