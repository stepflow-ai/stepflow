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
LangChain Integration Examples for Stepflow.

This demonstrates three practical approaches to using LangChain with Stepflow:
1. Decorated runnable: Using @server.langchain_component decorator
2. Named runnable: Using /invoke_named with import paths (cached, single-step)
3. UDF: Using /udf with user-provided Python code (via blob_id, self-contained)

Run with: python examples/langchain/langchain_server.py
"""

import asyncio

import msgspec

from stepflow_py import StepflowContext, StepflowServer

# Only run examples if LangChain is available
try:
    from langchain_core.runnables import RunnableLambda, RunnableParallel

    LANGCHAIN_AVAILABLE = True
except ImportError:
    print("LangChain not available. Install with: pip install langchain-core")
    LANGCHAIN_AVAILABLE = False
    exit(1)

# Create the server
server = StepflowServer()

if LANGCHAIN_AVAILABLE:
    # ============================================================================
    # DECORATED RUNNABLE: (@server.langchain_component decorator)
    # ============================================================================

    @server.langchain_component(name="text_analyzer")
    def create_text_analyzer():
        """Analyze text and return various metrics."""

        def analyze_text(text_input):
            text = text_input["text"]
            return {
                "word_count": len(text.split()),
                "char_count": len(text),
                "char_count_no_spaces": len(text.replace(" ", "")),
                "sentence_count": len([s for s in text.split(".") if s.strip()]),
                "uppercase_ratio": (
                    sum(1 for c in text if c.isupper()) / len(text) if text else 0
                ),
            }

        return RunnableLambda(analyze_text)

    @server.langchain_component(name="sentiment_classifier")
    def create_sentiment_classifier():
        """Simple sentiment classifier."""

        def classify_sentiment(input_data):
            text = input_data["text"].lower()

            positive_words = [
                "good",
                "great",
                "excellent",
                "amazing",
                "wonderful",
                "fantastic",
            ]
            negative_words = ["bad", "terrible", "awful", "horrible", "hate", "worst"]

            positive_score = sum(1 for word in positive_words if word in text)
            negative_score = sum(1 for word in negative_words if word in text)

            if positive_score > negative_score:
                return {
                    "sentiment": "positive",
                    "confidence": positive_score
                    / (positive_score + negative_score + 1),
                }
            elif negative_score > positive_score:
                return {
                    "sentiment": "negative",
                    "confidence": negative_score
                    / (positive_score + negative_score + 1),
                }
            else:
                return {"sentiment": "neutral", "confidence": 0.5}

        return RunnableLambda(classify_sentiment)

    @server.langchain_component(name="math_operations")
    def create_math_operations():
        """Perform parallel math operations on numbers."""

        def add_numbers(input_data):
            return input_data["a"] + input_data["b"]

        def multiply_numbers(input_data):
            return input_data["a"] * input_data["b"]

        def power_operation(input_data):
            return input_data["a"] ** input_data["b"]

        return RunnableParallel(
            {
                "sum": RunnableLambda(add_numbers),
                "product": RunnableLambda(multiply_numbers),
                "power": RunnableLambda(power_operation),
            }
        )

    # ============================================================================
    # NAMED RUNNABLE: (direct invocation with import paths)
    # ============================================================================

    # Component to directly invoke runnables from import paths (using SDK functions)
    class InvokeNamedInput(msgspec.Struct):
        import_path: str
        input: dict
        execution_mode: str = "invoke"
        config: dict | None = None

    @server.component
    async def invoke_named(input: InvokeNamedInput, context: StepflowContext) -> dict:
        """Directly invoke a runnable from Python import path with caching."""

        # Use the SDK function for the core functionality
        from stepflow_py.langchain_integration import invoke_named_runnable

        result = await invoke_named_runnable(
            import_path=input.import_path,
            input_data=input.input,
            config=input.config,
            context=context,
            use_cache=True,
        )

        return {"import_path": input.import_path, "result": result}


if __name__ == "__main__":
    import sys

    print("LangChain Stepflow Integration Examples")
    print("======================================")
    print()
    print("This server demonstrates three practical approaches to LangChain integration:")
    print("1. Decorated runnable: @server.langchain_component decorator")
    print("2. Named runnable: Direct invocation with import paths via /invoke_named")
    print("3. UDF: /udf with user-provided Python code (via blob_id, self-contained)")
    print()
    print("Available components:")

    components = server.get_components()
    for name, component in components.items():
        print(f"  - {name}: {component.description or 'No description'}")

    print()
    print("Starting Stepflow HTTP server...")
    asyncio.run(server.run())
