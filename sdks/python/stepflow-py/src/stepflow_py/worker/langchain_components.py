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
Direct LangChain components for Stepflow.

This module provides built-in components for executing LangChain runnables
directly without requiring blob storage.
"""

from typing import Any

import msgspec

from stepflow_py.worker.context import StepflowContext
from stepflow_py.worker.exceptions import StepflowExecutionError
from stepflow_py.worker.langchain_integration import (
    check_langchain_available,
    create_runnable_config,
    deserialize_runnable,
)


class LangChainInvokeInput(msgspec.Struct):
    """
    Input structure for the /langchain/invoke component.

    This component allows direct execution of LangChain runnables by providing
    the serialized runnable definition in the input.
    """

    runnable_definition: dict[str, Any]
    input: Any
    config: dict[str, Any] | None = None
    execution_mode: str = "invoke"


async def langchain_invoke(
    input: LangChainInvokeInput, context: StepflowContext
) -> Any:
    """
    Execute a LangChain runnable directly from its serialized definition.

    This component provides a direct way to execute LangChain runnables without
    requiring blob storage. The runnable definition is provided directly in the
    component input.

    Args:
        input: Contains runnable definition, input data, and optional config
        context: Stepflow context for runtime services

    Returns:
        The result of executing the LangChain runnable
    """
    check_langchain_available()

    # Validate execution mode (only invoke is supported)
    if input.execution_mode != "invoke":
        raise StepflowExecutionError(
            f"Invalid execution_mode '{input.execution_mode}'. "
            "Only 'invoke' mode is supported."
        )

    # Deserialize the runnable
    try:
        runnable = deserialize_runnable(input.runnable_definition)
    except Exception as e:
        raise StepflowExecutionError(
            f"Failed to deserialize LangChain runnable: {e}"
        ) from e

    # Prepare input for LangChain runnable
    langchain_input = input.input

    # Create runnable config from Stepflow input
    stepflow_input_dict = {"input": input.input, "config": input.config or {}}
    runnable_config = create_runnable_config(stepflow_input_dict, context)

    # Execute the runnable
    try:
        return await runnable.ainvoke(langchain_input, config=runnable_config)
    except Exception as e:
        if isinstance(e, StepflowExecutionError):
            raise
        raise StepflowExecutionError(f"LangChain runnable execution failed: {e}") from e


def register_langchain_components(server):
    """
    Register all built-in LangChain components with a Stepflow server.

    This function registers the following components:
    - /langchain/invoke: Execute LangChain runnables directly

    Args:
        server: The Stepflow server instance to register components with
    """
    # Only register if LangChain is available
    try:
        check_langchain_available()
    except StepflowExecutionError:
        # LangChain not available, skip registration
        return

    # Register the invoke component
    server.component(langchain_invoke, name="/langchain/invoke")
