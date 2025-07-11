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

import sys
import msgspec
import inspect
from typing import Any, Dict, List, Optional, Union
from stepflow_sdk.context import StepflowContext
from langflow.schema.data import Data
from langflow.schema.message import Message
from langflow.schema.dataframe import DataFrame

# Global cache for compiled functions by blob_id
_langflow_function_cache = {}


class LangflowUdfInput(msgspec.Struct):
    """Input structure for Langflow UDF execution."""
    blob_id: str
    input: Union[Data, DataFrame, Message, Dict[str, Any]]


async def langflow_udf(input: LangflowUdfInput, context: StepflowContext) -> Dict[str, Any]:
    """
    Execute Langflow user-defined function (UDF) using cached compiled functions from blobs.
    
    This is similar to the regular UDF but includes Langflow-specific types in the execution environment.

    Args:
        input: Contains blob_id (referencing stored code/schema) and input (langflow data)

    Returns:
        The result of the UDF execution, typically a Langflow type.
    """
    # Check if we have a cached function for this blob_id
    if input.blob_id in _langflow_function_cache:
        print(f"Using cached Langflow function for blob_id: {input.blob_id}", file=sys.stderr)
        compiled_func = _langflow_function_cache[input.blob_id]["function"]
    else:
        print(
            f"Loading and compiling Langflow function for blob_id: {input.blob_id}",
            file=sys.stderr,
        )

        # Get the blob containing the function definition
        try:
            blob_data = await context.get_blob(input.blob_id)
        except Exception as e:
            raise ValueError(f"Failed to retrieve blob {input.blob_id}: {e}")

        # Extract code and schema from blob
        if not isinstance(blob_data, dict):
            raise ValueError(f"Blob {input.blob_id} must contain a dictionary")

        code = blob_data.get("code")
        input_schema = blob_data.get("input_schema")
        function_name = blob_data.get("function_name")

        if not code:
            raise ValueError(f"Blob {input.blob_id} must contain 'code' field")
        if not input_schema:
            raise ValueError(f"Blob {input.blob_id} must contain 'input_schema' field")

        # Compile the function with validation built-in
        compiled_func = _compile_langflow_function(code, function_name, input_schema, context)

        # Cache the compiled function
        _langflow_function_cache[input.blob_id] = {
            "function": compiled_func,
            "input_schema": input_schema,
            "function_name": function_name,
        }

    # Execute the cached function (validation happens inside)
    try:
        if inspect.iscoroutinefunction(compiled_func):
            result = await compiled_func(input.input)
        else:
            result = compiled_func(input.input)
    except Exception as e:
        raise ValueError(f"Langflow function execution failed: {e}")

    print(f"Langflow result: {result}", file=sys.stderr)
    
    # Convert result to msgspec-compatible format
    return _convert_langflow_result(result)


def _convert_langflow_result(result: Any) -> Dict[str, Any]:
    """
    Convert Langflow types to msgspec-compatible dictionaries.
    """
    if isinstance(result, (Data, Message)):
        # Pydantic models with model_dump()
        return result.model_dump()
    elif isinstance(result, DataFrame):
        # DataFrame might not have model_dump, convert to dict representation
        try:
            if hasattr(result, 'model_dump'):
                return result.model_dump()
            else:
                # Convert DataFrame to dict
                return {
                    "data": result.to_dict('records') if hasattr(result, 'to_dict') else str(result),
                    "type": "dataframe"
                }
        except Exception:
            return {"data": str(result), "type": "dataframe"}
    elif isinstance(result, dict):
        # Recursively convert dict values
        converted = {}
        for key, value in result.items():
            converted[key] = _convert_langflow_result(value)
        return converted
    elif isinstance(result, list):
        # Recursively convert list items
        return [_convert_langflow_result(item) for item in result]
    else:
        # For other types, try to convert to a basic type
        if hasattr(result, 'model_dump'):
            return result.model_dump()
        elif hasattr(result, '__dict__'):
            return result.__dict__
        else:
            return result


def _compile_langflow_function(
    code: str, function_name: str | None, input_schema: dict, context: StepflowContext
):
    """
    Compile a Langflow function from code string and return the callable with validation.
    Includes Langflow-specific types in the execution environment.
    """
    import json
    import jsonschema

    # Create a safe execution environment with Langflow types
    safe_globals = {
        "__builtins__": {
            "len": len,
            "str": str,
            "int": int,
            "float": float,
            "bool": bool,
            "list": list,
            "dict": dict,
            "tuple": tuple,
            "set": set,
            "range": range,
            "sum": sum,
            "min": min,
            "max": max,
            "abs": abs,
            "round": round,
            "sorted": sorted,
            "reversed": reversed,
            "enumerate": enumerate,
            "zip": zip,
            "map": map,
            "filter": filter,
            "any": any,
            "all": all,
            "print": print,
            "isinstance": isinstance,
        },
        "json": json,
        "math": __import__("math"),
        "re": __import__("re"),
        "context": context,
        # Langflow-specific types
        "Data": Data,
        "DataFrame": DataFrame,
        "Message": Message,
    }

    def validate_input(data):
        """Validate input data against the schema."""
        try:
            jsonschema.validate(data, input_schema)
        except jsonschema.ValidationError as e:
            raise ValueError(f"Langflow input validation failed: {e.message}")
        except jsonschema.SchemaError as e:
            raise ValueError(f"Invalid Langflow schema: {e.message}")

    if function_name is not None:
        # Code contains function definition(s)
        local_scope = {}
        try:
            exec(code, safe_globals, local_scope)
        except Exception as e:
            raise ValueError(f"Langflow code execution failed: {e}")

        # Look for the specified function
        if function_name not in local_scope:
            raise ValueError(f"Langflow function '{function_name}' not found in code")

        func = local_scope[function_name]
        if not callable(func):
            raise ValueError(f"'{function_name}' is not a function")

        sig = inspect.signature(func)
        params = list(sig.parameters)
        if len(params) == 2 and params[1] == "context":
            # Function expects context as second parameter
            async def wrapper(input_data):
                validate_input(input_data)
                if inspect.iscoroutinefunction(func):
                    return await func(input_data, context)
                else:
                    return func(input_data, context)

            return wrapper
        else:
            # Function only expects input data
            def wrapper(input_data):
                validate_input(input_data)
                return func(input_data)

            return wrapper
    else:
        # Code is a function body - wrap it appropriately
        try:
            # Try as expression first (for simple cases)
            wrapped_code = f"lambda input: {code}"
            func = eval(wrapped_code, safe_globals)

            def wrapper(input_data):
                validate_input(input_data)
                return func(input_data)

            return wrapper
        except:
            # If that fails, try as statements in a function body
            try:
                # Properly indent each line of the code
                indented_lines = []
                for line in code.split("\n"):
                    if line.strip():  # Only indent non-empty lines
                        indented_lines.append("    " + line)
                    else:
                        indented_lines.append("")  # Keep empty lines as-is

                func_code = f"""def _temp_langflow_func(input, context):
{chr(10).join(indented_lines)}"""
                local_scope = {}
                exec(func_code, safe_globals, local_scope)
                temp_func = local_scope["_temp_langflow_func"]

                # Wrap to always pass context and validate
                async def wrapper(input_data):
                    validate_input(input_data)
                    if inspect.iscoroutinefunction(temp_func):
                        return await temp_func(input_data, context)
                    else:
                        return temp_func(input_data, context)

                return wrapper
            except Exception as e:
                raise ValueError(f"Langflow code compilation failed: {e}")