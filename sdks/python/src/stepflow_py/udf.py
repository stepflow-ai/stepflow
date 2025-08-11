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

import inspect
import sys
from typing import Any

import msgspec

from stepflow_py.context import StepflowContext
from stepflow_py.exceptions import StepflowValueError, CodeCompilationError


class UdfCompilationError(CodeCompilationError):
    """Raised when UDF code compilation fails."""
    
    def __init__(self, message: str, code: str, blob_id: str | None = None):
        super().__init__(message, code)
        self.blob_id = blob_id
        if blob_id:
            self.data["blob_id"] = blob_id
    
    def __str__(self):
        msg = super().__str__()
        if self.blob_id:
            msg = f"Blob '{self.blob_id}': {msg}"
        return msg


class WrapperConfig(msgspec.Struct, frozen=True):
    """Configuration for selecting the appropriate wrapper function."""

    is_async: bool
    use_input_wrapper: bool
    use_context: bool


# Global cache for compiled functions by blob_id
_function_cache: dict[str, Any] = {}


class _InputAccessError(Exception):
    """Raised when attempting to access input during runnable detection."""

    pass


class _ThrowingInputWrapper(dict):
    """Input wrapper that throws on any access - used for runnable detection."""

    def __getattr__(self, item):
        raise _InputAccessError(
            f"Attempted to access input.{item} during runnable detection"
        )

    def __getitem__(self, item):
        raise _InputAccessError(
            f"Attempted to access input[{item}] during runnable detection"
        )


# Pre-built wrapper factories to eliminate runtime conditionals
def _create_async_wrapped_context_wrapper(func, validate_input):
    """Async function with InputWrapper and context."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        return await func(_InputWrapper(input_data), context)

    return wrapper


def _create_async_wrapped_no_context_wrapper(func, validate_input):
    """Async function with InputWrapper, no context."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        return await func(_InputWrapper(input_data))

    return wrapper


def _create_async_direct_context_wrapper(func, validate_input):
    """Async function with direct input and context."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        return await func(input_data, context)

    return wrapper


def _create_async_direct_no_context_wrapper(func, validate_input):
    """Async function with direct input, no context."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        return await func(input_data)

    return wrapper


def _create_sync_wrapped_context_wrapper(func, validate_input):
    """Sync function with InputWrapper and context."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        return func(_InputWrapper(input_data), context)

    return wrapper


def _create_sync_wrapped_no_context_wrapper(func, validate_input):
    """Sync function with InputWrapper, no context."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        return func(_InputWrapper(input_data))

    return wrapper


def _create_sync_direct_context_wrapper(func, validate_input):
    """Sync function with direct input and context."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        return func(input_data, context)

    return wrapper


def _create_sync_direct_no_context_wrapper(func, validate_input):
    """Sync function with direct input, no context."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        return func(input_data)

    return wrapper


def _create_runnable_wrapper(func, validate_input, is_async, use_wrapper, use_context):
    """Create wrapper for LangChain runnables with compilation-time detection."""
    if is_async:
        if use_wrapper and use_context:

            async def wrapper(input_data, context):
                validate_input(input_data)
                result = await func(_InputWrapper(input_data), context)
                if hasattr(result, "ainvoke"):
                    return await result.ainvoke(input_data)
                else:
                    return result.invoke(input_data)

            return wrapper
        elif use_wrapper and not use_context:

            async def wrapper(input_data, context):
                validate_input(input_data)
                result = await func(_InputWrapper(input_data))
                if hasattr(result, "ainvoke"):
                    return await result.ainvoke(input_data)
                else:
                    return result.invoke(input_data)

            return wrapper
        elif not use_wrapper and use_context:

            async def wrapper(input_data, context):
                validate_input(input_data)
                result = await func(input_data, context)
                if hasattr(result, "ainvoke"):
                    return await result.ainvoke(input_data)
                else:
                    return result.invoke(input_data)

            return wrapper
        else:

            async def wrapper(input_data, context):
                validate_input(input_data)
                result = await func(input_data)
                if hasattr(result, "ainvoke"):
                    return await result.ainvoke(input_data)
                else:
                    return result.invoke(input_data)

            return wrapper
    else:
        if use_wrapper and use_context:

            async def wrapper(input_data, context):
                validate_input(input_data)
                result = func(_InputWrapper(input_data), context)
                if hasattr(result, "ainvoke"):
                    return await result.ainvoke(input_data)
                else:
                    return result.invoke(input_data)

            return wrapper
        elif use_wrapper and not use_context:

            async def wrapper(input_data, context):
                validate_input(input_data)
                result = func(_InputWrapper(input_data))
                if hasattr(result, "ainvoke"):
                    return await result.ainvoke(input_data)
                else:
                    return result.invoke(input_data)

            return wrapper
        elif not use_wrapper and use_context:

            async def wrapper(input_data, context):
                validate_input(input_data)
                result = func(input_data, context)
                if hasattr(result, "ainvoke"):
                    return await result.ainvoke(input_data)
                else:
                    return result.invoke(input_data)

            return wrapper
        else:

            async def wrapper(input_data, context):
                validate_input(input_data)
                result = func(input_data)
                if hasattr(result, "ainvoke"):
                    return await result.ainvoke(input_data)
                else:
                    return result.invoke(input_data)

            return wrapper


# Lookup table for non-runnable wrapper factories
_WRAPPER_FACTORIES = {
    WrapperConfig(
        is_async=True, use_input_wrapper=True, use_context=True
    ): _create_async_wrapped_context_wrapper,
    WrapperConfig(
        is_async=True, use_input_wrapper=True, use_context=False
    ): _create_async_wrapped_no_context_wrapper,
    WrapperConfig(
        is_async=True, use_input_wrapper=False, use_context=True
    ): _create_async_direct_context_wrapper,
    WrapperConfig(
        is_async=True, use_input_wrapper=False, use_context=False
    ): _create_async_direct_no_context_wrapper,
    WrapperConfig(
        is_async=False, use_input_wrapper=True, use_context=True
    ): _create_sync_wrapped_context_wrapper,
    WrapperConfig(
        is_async=False, use_input_wrapper=True, use_context=False
    ): _create_sync_wrapped_no_context_wrapper,
    WrapperConfig(
        is_async=False, use_input_wrapper=False, use_context=True
    ): _create_sync_direct_context_wrapper,
    WrapperConfig(
        is_async=False, use_input_wrapper=False, use_context=False
    ): _create_sync_direct_no_context_wrapper,
}


def _detect_runnable_at_compile_time(func, is_async, use_wrapper, use_context):
    """Test if function returns a runnable without accessing input data.

    Returns True if the function creates a runnable independent of input,
    False if it accesses input (and thus can't be cached as a runnable),
    or False if we can't determine (async functions, exceptions, etc.).
    """
    if is_async:
        # Skip async functions - we'd need to run an event loop to test them
        return False

    try:
        # Create throwing input that raises if accessed
        if use_wrapper:
            test_input = _ThrowingInputWrapper()
        else:
            test_input = _ThrowingInputWrapper()

        # Create a mock context
        mock_context = type("MockContext", (), {})()

        # Try to execute the function - it should create a runnable without
        # accessing input
        if use_context:
            result = func(test_input, mock_context)
        else:
            result = func(test_input)

        # If we got here without an exception, check if it's a runnable
        return hasattr(result, "invoke")

    except _InputAccessError:
        # Function tried to access input - it's not a cacheable runnable
        return False
    except Exception:
        # Any other error - assume it's not a runnable for safety
        return False


class UdfInput(msgspec.Struct):
    blob_id: str
    input: dict


async def udf(input: UdfInput, context: StepflowContext) -> Any:
    """Execute user-defined function (UDF) using cached compiled functions from blobs.

    Args:
        input: Contains blob_id (referencing stored code/schema) and input (data)

    Returns:
        The result of the UDF execution.
    """
    # Check if we have a cached function for this blob_id
    if input.blob_id in _function_cache:
        print(f"Using cached function for blob_id: {input.blob_id}", file=sys.stderr)
        compiled_func = _function_cache[input.blob_id]["function"]
    else:
        print(
            f"Loading and compiling function for blob_id: {input.blob_id}",
            file=sys.stderr,
        )

        # Get the blob containing the function definition
        try:
            blob_data = await context.get_blob(input.blob_id)
        except Exception as e:
            raise ValueError(f"Failed to retrieve blob {input.blob_id}: {e}") from e

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
        try:
            compiled_func = _compile_function(code, function_name, input_schema)
        except UdfCompilationError as e:
            # Re-raise with blob context for better tracing
            e.blob_id = input.blob_id
            if input.blob_id:
                e.data["blob_id"] = input.blob_id
            raise

        # Cache the compiled function with metadata
        _function_cache[input.blob_id] = {
            "function": compiled_func,
            "input_schema": input_schema,
            "function_name": function_name,
        }

    # Execute the cached function (validation happens inside)
    try:
        result = await compiled_func(input.input, context)
    except Exception as e:
        raise ValueError(f"Function execution failed: {e}") from e

    return result


class _InputWrapper(dict):
    def __init__(self, input_data: dict, path: list[str] | None = None):
        super().__init__(input_data)
        self.path = path or []

    def __getattr__(self, item):
        """Allow attribute access to input data."""
        return self[item]

    def __getitem__(self, item):
        """Allow dictionary-like access to input data."""
        if item in self:
            value = super().__getitem__(item)
            if isinstance(value, dict):
                return _InputWrapper(value, self.path + [item])
            else:
                return value
        raise StepflowValueError(f"Input has no attribute '{item}'")


def _compile_function(code: str, function_name: str | None, input_schema: dict):
    """Compile a function from code string and return the callable with validation."""
    import json

    import jsonschema

    # Create a safe execution environment
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
            "__import__": __import__,
            "getattr": getattr,
            "type": type,
        },
        "json": json,
        "math": __import__("math"),
        "re": __import__("re"),
    }

    # Add LangChain imports if available
    try:
        from langchain_core.runnables import RunnableLambda, RunnableParallel

        safe_globals["RunnableLambda"] = RunnableLambda
        safe_globals["RunnableParallel"] = RunnableParallel

        # Also add the langchain_core.runnables module for more comprehensive access
        import langchain_core.runnables as langchain_runnables

        safe_globals["langchain_core"] = type(
            "langchain_core", (), {"runnables": langchain_runnables}
        )
    except ImportError:
        # LangChain not available, continue without it
        pass

    def validate_input(data):
        """Validate input data against the schema."""
        try:
            jsonschema.validate(data, input_schema)
        except jsonschema.ValidationError as e:
            raise ValueError(f"Input validation failed: {e.message}") from e
        except jsonschema.SchemaError as e:
            raise ValueError(f"Invalid schema: {e.message}") from e

    if function_name is not None:
        # Code contains function definition(s)
        local_scope: dict[str, Any] = {}
        try:
            exec(code, safe_globals, local_scope)
        except Exception as e:
            raise ValueError(f"Code execution failed: {e}") from e

        # Look for the specified function
        if function_name not in local_scope:
            raise ValueError(f"Function '{function_name}' not found in code")

        func = local_scope[function_name]
        if not callable(func):
            raise ValueError(f"'{function_name}' is not a function")

        sig = inspect.signature(func)
        params = list(sig.parameters)

        input_annotation = sig.parameters[params[0]].annotation
        wrap_input = (
            input_annotation == inspect.Parameter.empty or input_annotation is dict
        )
        use_context = len(params) == 2 and params[1] == "context"
        is_async = inspect.iscoroutinefunction(func)

        # Test if function creates input-independent runnables at compile time
        is_runnable = _detect_runnable_at_compile_time(
            func, is_async, wrap_input, use_context
        )

        if is_runnable:
            # Function creates runnables - use runnable wrapper
            return _create_runnable_wrapper(
                func, validate_input, is_async, wrap_input, use_context
            )
        else:
            # Function doesn't create runnables - use optimized lookup
            wrapper_config = WrapperConfig(
                is_async=is_async, use_input_wrapper=wrap_input, use_context=use_context
            )
            wrapper_factory = _WRAPPER_FACTORIES[wrapper_config]
            return wrapper_factory(func, validate_input)
    else:
        # Code is a function body - two supported patterns:
        # 1. Try as statements in a function body with return (preferred pattern)
        # 2. Try as lambda expression

        # First, try as statements in a function body (handles return statements)
        # Check if the code contains any return statements - this suggests it's meant
        # to be a function body rather than an expression
        lines = [line for line in code.split("\n") if line.strip()]
        has_return_statement = any(
            line.strip().startswith("return ") 
            for line in lines
        )

        if has_return_statement:
            try:
                # Properly indent each line of the code
                indented_lines = []
                for line in code.split("\n"):
                    if line.strip():  # Only indent non-empty lines
                        indented_lines.append("    " + line)
                    else:
                        indented_lines.append("")  # Keep empty lines as-is

                func_code = f"""def _temp_func(input, context):
{chr(10).join(indented_lines)}"""
                local_scope = {}
                exec(func_code, safe_globals, local_scope)
                temp_func = local_scope["_temp_func"]

                # Test if this function creates input-independent runnables
                is_runnable = _detect_runnable_at_compile_time(
                    temp_func, False, True, True
                )

                if is_runnable:
                    # Function creates runnables - use runnable wrapper
                    async def wrapper(input_data, context):
                        validate_input(input_data)
                        result = temp_func(_InputWrapper(input_data), context)
                        if hasattr(result, "ainvoke"):
                            return await result.ainvoke(input_data)
                        else:
                            return result.invoke(input_data)

                    return wrapper
                else:
                    # Standard function - use optimized wrapper
                    return _create_sync_wrapped_context_wrapper(
                        temp_func, validate_input
                    )
            except Exception:
                pass  # Fall through to other approaches

        # Second, try as a lambda expression
        try:
            wrapped_code = f"lambda input: {code}"
            func = eval(wrapped_code, safe_globals)

            # Test if this lambda creates input-independent runnables
            is_runnable = _detect_runnable_at_compile_time(func, False, True, False)

            if is_runnable:
                # Lambda creates runnables - use runnable wrapper
                async def wrapper(input_data, context):
                    validate_input(input_data)
                    result = func(_InputWrapper(input_data))
                    if hasattr(result, "ainvoke"):
                        return await result.ainvoke(input_data)
                    else:
                        return result.invoke(input_data)

                return wrapper
            else:
                # Standard lambda - use optimized wrapper
                return _create_sync_wrapped_no_context_wrapper(func, validate_input)
        except Exception:
            pass

        # If we reach here, none of the compilation approaches worked
        raise UdfCompilationError(
            "Unable to compile code as function body with return statement or lambda expression. "
            "Code must either: (1) contain return statements for function body compilation, or "
            "(2) be a valid expression for lambda compilation.",
            code
        )
