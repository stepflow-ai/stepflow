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

import inspect
import logging
from typing import Any

import msgspec

from stepflow_py.worker.context import StepflowContext
from stepflow_py.worker.exceptions import CodeCompilationError, StepflowValueError
from stepflow_py.worker.observability import get_tracer

logger = logging.getLogger(__name__)


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


# Helper functions to reduce wrapper duplication
def _create_standard_wrapper(
    func, validate_input, is_async, use_input_wrapper, use_context
):
    """Create a standard wrapper function based on configuration."""

    async def wrapper(input_data, context):
        validate_input(input_data)

        # Prepare arguments based on configuration
        args = []
        if use_input_wrapper:
            args.append(_InputWrapper(input_data))
        else:
            args.append(input_data)

        if use_context:
            args.append(context)

        # Call function based on whether it's async or sync
        if is_async:
            return await func(*args)
        else:
            return func(*args)

    return wrapper


async def _invoke_runnable_result(result, input_data):
    """Helper to invoke a runnable result with proper async handling."""
    return await result.ainvoke(input_data)


# Specific runnable wrapper factories (no runtime conditionals for performance)
def _create_async_wrapped_context_runnable_wrapper(func, validate_input):
    """Async function with InputWrapper and context that returns runnables."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        result = await func(_InputWrapper(input_data), context)
        return await result.ainvoke(input_data)

    return wrapper


def _create_async_wrapped_no_context_runnable_wrapper(func, validate_input):
    """Async function with InputWrapper, no context that returns runnables."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        result = await func(_InputWrapper(input_data))
        return await result.ainvoke(input_data)

    return wrapper


def _create_async_direct_context_runnable_wrapper(func, validate_input):
    """Async function with direct input and context that returns runnables."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        result = await func(input_data, context)
        return await result.ainvoke(input_data)

    return wrapper


def _create_async_direct_no_context_runnable_wrapper(func, validate_input):
    """Async function with direct input, no context that returns runnables."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        result = await func(input_data)
        return await result.ainvoke(input_data)

    return wrapper


def _create_sync_wrapped_context_runnable_wrapper(func, validate_input):
    """Sync function with InputWrapper and context that returns runnables."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        result = func(_InputWrapper(input_data), context)
        return await result.ainvoke(input_data)

    return wrapper


def _create_sync_wrapped_no_context_runnable_wrapper(func, validate_input):
    """Sync function with InputWrapper, no context that returns runnables."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        result = func(_InputWrapper(input_data))
        return await result.ainvoke(input_data)

    return wrapper


def _create_sync_direct_context_runnable_wrapper(func, validate_input):
    """Sync function with direct input and context that returns runnables."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        result = func(input_data, context)
        return await result.ainvoke(input_data)

    return wrapper


def _create_sync_direct_no_context_runnable_wrapper(func, validate_input):
    """Sync function with direct input, no context that returns runnables."""

    async def wrapper(input_data, context):
        validate_input(input_data)
        result = func(input_data)
        return await result.ainvoke(input_data)

    return wrapper


# Pre-built wrapper factories to eliminate runtime conditionals
def _create_async_wrapped_context_wrapper(func, validate_input):
    """Async function with InputWrapper and context."""
    return _create_standard_wrapper(func, validate_input, True, True, True)


def _create_async_wrapped_no_context_wrapper(func, validate_input):
    """Async function with InputWrapper, no context."""
    return _create_standard_wrapper(func, validate_input, True, True, False)


def _create_async_direct_context_wrapper(func, validate_input):
    """Async function with direct input and context."""
    return _create_standard_wrapper(func, validate_input, True, False, True)


def _create_async_direct_no_context_wrapper(func, validate_input):
    """Async function with direct input, no context."""
    return _create_standard_wrapper(func, validate_input, True, False, False)


def _create_sync_wrapped_context_wrapper(func, validate_input):
    """Sync function with InputWrapper and context."""
    return _create_standard_wrapper(func, validate_input, False, True, True)


def _create_sync_wrapped_no_context_wrapper(func, validate_input):
    """Sync function with InputWrapper, no context."""
    return _create_standard_wrapper(func, validate_input, False, True, False)


def _create_sync_direct_context_wrapper(func, validate_input):
    """Sync function with direct input and context."""
    return _create_standard_wrapper(func, validate_input, False, False, True)


def _create_sync_direct_no_context_wrapper(func, validate_input):
    """Sync function with direct input, no context."""
    return _create_standard_wrapper(func, validate_input, False, False, False)


# Lookup tables for wrapper factories (eliminates runtime conditionals)
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

_RUNNABLE_WRAPPER_FACTORIES = {
    WrapperConfig(
        is_async=True, use_input_wrapper=True, use_context=True
    ): _create_async_wrapped_context_runnable_wrapper,
    WrapperConfig(
        is_async=True, use_input_wrapper=True, use_context=False
    ): _create_async_wrapped_no_context_runnable_wrapper,
    WrapperConfig(
        is_async=True, use_input_wrapper=False, use_context=True
    ): _create_async_direct_context_runnable_wrapper,
    WrapperConfig(
        is_async=True, use_input_wrapper=False, use_context=False
    ): _create_async_direct_no_context_runnable_wrapper,
    WrapperConfig(
        is_async=False, use_input_wrapper=True, use_context=True
    ): _create_sync_wrapped_context_runnable_wrapper,
    WrapperConfig(
        is_async=False, use_input_wrapper=True, use_context=False
    ): _create_sync_wrapped_no_context_runnable_wrapper,
    WrapperConfig(
        is_async=False, use_input_wrapper=False, use_context=True
    ): _create_sync_direct_context_runnable_wrapper,
    WrapperConfig(
        is_async=False, use_input_wrapper=False, use_context=False
    ): _create_sync_direct_no_context_runnable_wrapper,
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
        logger.debug(f"Using cached function for blob_id: {input.blob_id}")
        compiled_func = _function_cache[input.blob_id]["function"]
    else:
        logger.debug(f"Loading and compiling function for blob_id: {input.blob_id}")

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

        if not code:
            raise ValueError(f"Blob {input.blob_id} must contain 'code' field")
        if not input_schema:
            raise ValueError(f"Blob {input.blob_id} must contain 'input_schema' field")

        # Compile the function with validation built-in
        tracer = get_tracer(__name__)
        with tracer.start_as_current_span(
            "compile_function",
        ):
            try:
                compiled_func = _compile_function(code, input_schema)
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


def _compile_function(code: str, input_schema: dict):
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
            # Exception types for error handling
            "Exception": Exception,
            "ValueError": ValueError,
            "TypeError": TypeError,
            "RuntimeError": RuntimeError,
            "KeyError": KeyError,
            "IndexError": IndexError,
            "AttributeError": AttributeError,
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

    # Try different patterns in order of preference
    func = None

    # 1. Try to execute as a complete code block (for function definitions)
    try:
        local_scope: dict[str, Any] = {}
        exec(code, safe_globals, local_scope)

        # If we got here, check if there's exactly one item defined that's callable
        # This covers the case: def foo(input): return input["a"]\nfoo
        callables = [
            (name, obj)
            for name, obj in local_scope.items()
            if callable(obj) and not name.startswith("_")
        ]

        if len(callables) == 1:
            # Found exactly one callable - use it
            _, func = callables[0]

    except Exception:
        pass

    # 2. If we don't have a function yet, try as expression
    if func is None:
        try:
            func = eval(code, safe_globals)
            if not callable(func):
                func = None
        except Exception:
            pass

    # 3. If still no function, try lambda pattern
    if func is None:
        try:
            wrapped_code = f"lambda input: {code}"
            func = eval(wrapped_code, safe_globals)
        except Exception:
            pass

    # 4. If still no function, try function body pattern
    if func is None:
        try:
            # Check if the code contains return statements or other valid statements
            import re

            has_return = re.search(r"^\s*return\b", code, re.MULTILINE)
            has_raise = re.search(r"^\s*raise\b", code, re.MULTILINE)
            has_if = re.search(r"^\s*if\b", code, re.MULTILINE)
            has_statements = has_return or has_raise or has_if

            if has_statements:
                # Try as function body with input/context detection
                # Properly indent each line of the code
                indented_lines = []
                for line in code.split("\n"):
                    if line.strip():  # Only indent non-empty lines
                        indented_lines.append("    " + line)
                    else:
                        indented_lines.append("")  # Keep empty lines as-is

                test_code_for_input = f"""def _test_func(input, context):
{chr(10).join(indented_lines)}
_test_func"""
                local_scope = {}
                exec(test_code_for_input, safe_globals, local_scope)
                func = local_scope["_test_func"]
        except Exception:
            pass

    # If we still don't have a callable, raise an error
    if func is None or not callable(func):
        raise UdfCompilationError(
            "Unable to compile code. Code must be one of: "
            "(1) A Python expression, "
            "(2) A function definition followed by the function name, "
            "(3) Code that results in a callable object, "
            "(4) Function body statements (with return, raise, if, etc.).",
            code,
        )

    # Now analyze the function to determine how to wrap it
    sig = inspect.signature(func)
    params = list(sig.parameters.keys())
    is_async = inspect.iscoroutinefunction(func)

    # Determine parameter usage by trying to call with test parameters
    use_input_wrapper = True
    use_context = False

    if len(params) >= 1:
        # Check if function accesses input or context by testing parameter
        # names and doing runtime detection
        try:
            # Try to determine input wrapper usage from annotation or name
            first_param = list(sig.parameters.values())[0]
            if (
                first_param.annotation != inspect.Parameter.empty
                and first_param.annotation is not dict
            ):
                use_input_wrapper = False
        except:  # noqa: E722
            pass

    if len(params) >= 2:
        # Check if second parameter is context
        if params[1] == "context":
            use_context = True
        elif len(params) == 2:
            # Try runtime detection for context usage
            use_context = True

    # Test if function returns a runnable by trying to execute it
    is_runnable = _detect_runnable_at_compile_time(
        func, is_async, use_input_wrapper, use_context
    )

    wrapper_config = WrapperConfig(
        is_async=is_async, use_input_wrapper=use_input_wrapper, use_context=use_context
    )

    if is_runnable:
        # Function creates runnables - use runnable wrapper factory
        wrapper_factory = _RUNNABLE_WRAPPER_FACTORIES[wrapper_config]
        return wrapper_factory(func, validate_input)
    else:
        # Standard function - use standard wrapper factory
        wrapper_factory = _WRAPPER_FACTORIES[wrapper_config]
        return wrapper_factory(func, validate_input)
