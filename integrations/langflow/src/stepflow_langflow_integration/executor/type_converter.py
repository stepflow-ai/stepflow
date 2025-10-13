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

"""Type conversion between Langflow and Stepflow formats."""

import ast
import operator
from typing import Any


def _execute_calculator_tool(expression: str) -> str:
    """Execute calculator tool by evaluating the mathematical expression.

    Args:
        expression: Mathematical expression to evaluate

    Returns:
        String result of the calculation
    """
    try:
        # Parse the expression safely
        tree = ast.parse(expression, mode="eval")
        result = _eval_expr(tree.body)

        # Format result similar to the original CalculatorComponent
        formatted_result = f"{float(result):.6f}".rstrip("0").rstrip(".")
        return formatted_result

    except Exception as e:
        return f"Calculator error: {str(e)}"


def _eval_expr(node: ast.AST) -> float:
    """Evaluate an AST node recursively (from CalculatorComponent)."""
    OPERATORS = {
        ast.Add: operator.add,
        ast.Sub: operator.sub,
        ast.Mult: operator.mul,
        ast.Div: operator.truediv,
        ast.Pow: operator.pow,
    }

    if isinstance(node, ast.Constant):
        if isinstance(node.value, int | float):
            return float(node.value)
        raise TypeError(f"Unsupported constant type: {type(node.value).__name__}")

    if isinstance(node, ast.Num):  # For backwards compatibility
        if isinstance(node.n, int | float):
            return float(node.n)
        raise TypeError(f"Unsupported number type: {type(node.n).__name__}")

    if isinstance(node, ast.BinOp):
        op_type = type(node.op)
        if op_type not in OPERATORS:
            raise TypeError(f"Unsupported binary operator: {op_type.__name__}")

        left = _eval_expr(node.left)
        right = _eval_expr(node.right)
        result = OPERATORS[op_type](left, right)
        return float(result)

    raise TypeError(f"Unsupported operation or expression type: {type(node).__name__}")


def _create_tool_from_wrapper(tool_wrapper: dict[str, Any]) -> Any:
    """Create a LangChain StructuredTool from a tool wrapper.

    This function recreates a proper LangChain tool from the serialized
    tool wrapper, including the ability to execute the component.

    Args:
        tool_wrapper: Tool wrapper dict containing component code and metadata

    Returns:
        StructuredTool that can execute the component
    """
    try:
        from langchain_core.tools import StructuredTool
        from pydantic import BaseModel, create_model

        # Extract tool metadata
        tool_metadata = tool_wrapper.get("tool_metadata", {})
        tool_input_schema = tool_wrapper.get("tool_input_schema", {})
        static_inputs = tool_wrapper.get("static_inputs", {})
        component_type = tool_wrapper.get("component_type", "unknown")
        session_id = tool_wrapper.get("session_id", "default_session")

        # Handle both old format (component_code) and new format (code_blob_id)
        component_code = tool_wrapper.get("component_code")
        code_blob_id = tool_wrapper.get("code_blob_id")

        if component_code is None and code_blob_id is None:
            raise ValueError(
                "Tool wrapper missing both component_code and code_blob_id"
            )

        # Create input schema from tool wrapper
        properties = tool_input_schema.get("properties", {})

        # Convert to Pydantic field definitions
        field_definitions: dict[str, tuple[type, Any]] = {}
        for field_name, field_def in properties.items():
            field_type: type = str  # Default to string
            default_value = field_def.get("default", "")
            field_definitions[field_name] = (field_type, default_value)

        # Create input schema class dynamically
        input_schema: type[BaseModel]
        if field_definitions:
            # Type ignore needed because mypy doesn't understand that unpacking
            # field_definitions creates the correct keyword arguments for create_model
            input_schema = create_model("ToolInputSchema", **field_definitions)  # type: ignore[call-overload]
        else:

            class EmptySchema(BaseModel):
                pass

            input_schema = EmptySchema

        # Create tool execution function
        def tool_func(**kwargs) -> dict[str, Any]:
            """Execute the tool by running the component."""
            try:
                # Special handling for calculator tool - evaluate the expression
                if (
                    tool_metadata.get("name") == "evaluate_expression"
                    and "expression" in kwargs
                ):
                    result = _execute_calculator_tool(kwargs["expression"])
                    return {"result": result}

                # For other tools, merge static inputs with dynamic inputs and
                # session_id
                merged_inputs = {**static_inputs, **kwargs, "session_id": session_id}

                # TODO: Execute component using UDF executor or component recreation
                # For now, return a placeholder that shows the tool is working
                result_data = {
                    "result": (
                        f"Tool {tool_metadata.get('name', 'unknown')} "
                        f"executed with inputs: {merged_inputs}"
                    ),
                    "component_type": component_type,
                    "inputs": merged_inputs,
                    "status": "tool_wrapper_execution",
                }

                # Add code source info for debugging
                if code_blob_id:
                    result_data["code_blob_id"] = code_blob_id
                elif component_code:
                    result_data["has_component_code"] = True

                return result_data

            except Exception as e:
                return {
                    "error": f"Tool execution failed: {str(e)}",
                    "component_type": component_type,
                }

        # Create the StructuredTool
        return StructuredTool.from_function(
            func=tool_func,
            name=tool_metadata.get("name", "unknown_tool"),
            description=tool_metadata.get("description", ""),
            args_schema=input_schema,
        )

    except Exception as e:
        # Fallback to a simple object if StructuredTool creation fails
        class FailedToolWrapper:
            def __init__(self, tool_wrapper, error):
                self.tool_wrapper = tool_wrapper
                self.error = error
                self.name = tool_wrapper.get("tool_metadata", {}).get(
                    "name", "failed_tool"
                )

            def invoke(self, inputs):
                return {"error": f"Tool creation failed: {self.error}"}

        return FailedToolWrapper(tool_wrapper, str(e))


class TypeConverter:
    """Converts between Langflow and Stepflow type representations."""

    def serialize_langflow_object(self, obj: Any) -> Any:
        """Serialize Langflow objects with type metadata.

        Args:
            obj: Langflow object to serialize

        Returns:
            Serialized object with type metadata
        """
        # TODO(https://github.com/stepflow-ai/stepflow/issues/369)
        # Eliminate the type adaptation when `langflow` and `lfx` types
        # are interchangeable.

        # Import Langflow types dynamically to avoid import errors
        try:
            from langflow.schema.data import Data
            from langflow.schema.dataframe import DataFrame as LangflowDataFrame
            from langflow.schema.message import Message
        except ImportError:
            LangflowDataFrame = None
            Data = None
            Message = None

        # Also try to import lfx types (new Langflow split package)
        try:
            from lfx.schema.dataframe import DataFrame as LfxDataFrame
        except ImportError:
            LfxDataFrame = None

        # Create a tuple of DataFrame types to check
        dataframe_types = tuple(t for t in [LangflowDataFrame, LfxDataFrame] if t is not None)

        if not dataframe_types and not Data and not Message:
            # No Langflow types available, return obj as-is
            return obj

        if Message and isinstance(obj, Message):
            serialized = obj.model_dump(mode="json")
            serialized["__langflow_type__"] = "Message"
            return serialized
        elif Data and isinstance(obj, Data):
            serialized = obj.model_dump(mode="json")
            serialized["__langflow_type__"] = "Data"
            return serialized
        elif dataframe_types and isinstance(obj, dataframe_types):
            # Convert DataFrame to JSON using pandas to_json with columnar format
            # Using orient="split" is more efficient - it stores column names once
            # instead of repeating them for every row
            # Savings: ~45% for typical DataFrames with multiple columns
            json_str = obj.to_json(orient="split")
            return {
                "__langflow_type__": "DataFrame",
                "json_data": json_str,
                "text_key": getattr(obj, "text_key", "text"),
                "default_value": getattr(obj, "default_value", ""),
            }
        elif isinstance(obj, str | int | float | bool | type(None)):
            # Simple serializable types
            return obj
        elif isinstance(obj, list):
            # Recursively serialize list items (might contain tool wrappers or
            # other complex objects)
            return [self.serialize_langflow_object(item) for item in obj]
        elif isinstance(obj, dict):
            # For plain dicts, recursively serialize values
            return {
                key: self.serialize_langflow_object(value) for key, value in obj.items()
            }
        else:
            # Check if this is a tool wrapper from component_tool
            if isinstance(obj, dict) and obj.get("__tool_wrapper__"):
                return self._serialize_tool_wrapper(obj)

            # Try to serialize as BaseModel (pydantic)
            try:
                from pydantic import BaseModel

                if isinstance(obj, BaseModel):
                    # Serialize BaseModel with class metadata and proper handling of
                    # special types. Use model_dump with warnings=False to handle
                    # SecretStr and other special types
                    try:
                        # Try with warnings=False to handle SecretStr properly
                        serialized = obj.model_dump(mode="json", warnings=False)
                    except Exception:
                        # Fallback to regular model_dump if warnings parameter
                        # not supported
                        serialized = obj.model_dump(mode="json")

                    # Custom handling for SecretStr and other special Pydantic types
                    # TODO: Ideally, we'd have a way of creating an object that
                    # serializes
                    # with the secrets, but can be printed (for debugging) without the
                    # secrets. One option might be to have a special way of storing the
                    # secrets elsewhere?
                    serialized = self._handle_special_pydantic_types(obj, serialized)

                    serialized["__class_name__"] = obj.__class__.__name__
                    serialized["__module_name__"] = obj.__class__.__module__
                    return serialized
            except ImportError:
                pass

            # Complex object that can't be serialized
            raise ValueError(
                f"Cannot serialize object of type {type(obj)}. "
                "Only BaseModel objects and simple types are supported."
            )

    def deserialize_to_langflow_type(
        self, obj: Any, expected_type: type | None = None
    ) -> Any:
        """Deserialize objects back to Langflow types.

        Args:
            obj: Object to deserialize
            expected_type: Expected Langflow type

        Returns:
            Deserialized Langflow object
        """
        # Handle lists - recursively deserialize each item
        if isinstance(obj, list):
            return [
                self.deserialize_to_langflow_type(item, expected_type) for item in obj
            ]

        # Handle cross-package Message conversion (lfx.schema.message.Message -> langflow.schema.message.Message)
        # This is needed because lfx and langflow have separate Message classes that are functionally identical
        # but Python's type system treats them as different types
        if hasattr(obj, "__class__") and obj.__class__.__name__ == "Message":
            converted = self._convert_lfx_message_to_langflow(obj)
            if converted is not obj:
                return converted

        # Handle non-dict objects (primitives and unconverted objects)
        if not isinstance(obj, dict):
            return obj

        # Check for tool wrapper serialization
        if obj.get("__tool_wrapper__"):
            return self._deserialize_tool_wrapper(obj)

        # Import Langflow types dynamically
        try:
            from langflow.schema.data import Data
            from langflow.schema.dataframe import DataFrame as LangflowDataFrame
            from langflow.schema.message import Message
        except ImportError:
            Data = None
            LangflowDataFrame = None
            Message = None

        # Also try to import lfx types (new Langflow split package)
        try:
            from lfx.schema.dataframe import DataFrame as LfxDataFrame
        except ImportError:
            LfxDataFrame = None

        # Prefer lfx types over langflow types for DataFrame
        # This ensures compatibility with modern Langflow components that import from lfx.schema
        # When DataFrame.to_data_list() is called, it returns Data objects from the same module
        # which pass isinstance checks in component code (e.g., AstraDB imports from lfx.schema.data)
        DataFrame = LfxDataFrame if LfxDataFrame else LangflowDataFrame

        if not DataFrame and not Data and not Message:
            return obj

        langflow_type = obj.get("__langflow_type__")
        if langflow_type:
            # Remove type metadata
            obj_data = {k: v for k, v in obj.items() if k != "__langflow_type__"}

            if langflow_type == "Message" and Message:
                return Message(**obj_data)
            elif langflow_type == "Data" and Data:
                return Data(**obj_data)
            elif langflow_type == "DataFrame" and DataFrame:
                try:
                    import json
                    import pandas as pd

                    text_key = obj_data.get("text_key", "text")
                    default_value = obj_data.get("default_value", "")

                    # Deserialize from json_data format (columnar split format)
                    json_str = obj_data.get("json_data")
                    if not json_str:
                        raise ValueError("DataFrame missing required json_data field")

                    # Parse the JSON string
                    if isinstance(json_str, str):
                        split_data = json.loads(json_str)
                    else:
                        split_data = json_str

                    # Deserialize using pandas from split format
                    import io
                    json_io = io.StringIO(json_str if isinstance(json_str, str) else json.dumps(json_str))
                    pd_df = pd.read_json(json_io, orient="split")
                    data_list = pd_df.to_dict(orient="records")

                    # Replace NaN values with None to avoid JSON serialization errors
                    # Pandas converts null/None to NaN for numeric columns, but NaN is not
                    # JSON-compliant and causes errors in strict JSON serializers (e.g., AstraDB)
                    data_list = [
                        {k: (None if pd.isna(v) else v) for k, v in record.items()}
                        for record in data_list
                    ]

                    return DataFrame(
                        data=data_list,
                        text_key=text_key,
                        default_value=default_value,
                    )
                except Exception:
                    # Failed to reconstruct, return the original dict so recovery logic can handle it
                    # Don't return json_data string directly as that bypasses recovery
                    return obj

        # Check for BaseModel serialization
        class_name = obj.get("__class_name__")
        module_name = obj.get("__module_name__")
        if class_name and module_name:
            try:
                # Import the module and get the class
                import importlib

                module = importlib.import_module(module_name)
                class_type = getattr(module, class_name)

                # Remove metadata from object data
                obj_data = {
                    k: v
                    for k, v in obj.items()
                    if k not in ("__class_name__", "__module_name__")
                }

                # Recreate the object
                return class_type(**obj_data)
            except Exception:
                # Failed to reconstruct BaseModel, continue with other approaches
                pass

        # Use expected type if provided
        if expected_type:
            if expected_type == Message:
                return Message(**obj)
            elif expected_type == Data:
                return Data(**obj)

        return obj

    def _handle_special_pydantic_types(
        self, obj: Any, serialized: dict[str, Any]
    ) -> dict[str, Any]:
        """Handle special Pydantic types like SecretStr during serialization.

        Args:
            obj: Original BaseModel object
            serialized: Already serialized dict from model_dump()

        Returns:
            Updated serialized dict with special types properly handled
        """
        # Get the model's fields to check for SecretStr types
        try:
            # Check if we can access model fields (Pydantic v2 style)
            if hasattr(obj, "model_fields"):
                fields = obj.model_fields
                for field_name, field_info in fields.items():
                    if hasattr(field_info, "annotation"):
                        field_type = field_info.annotation
                        # Handle SecretStr fields
                        if self._is_secret_str_type(field_type):
                            field_value = getattr(obj, field_name, None)
                            if field_value is not None:
                                try:
                                    # Get the actual secret value
                                    secret_value = field_value.get_secret_value()
                                    # If the secret value looks like an environment
                                    # variable name, try to resolve it
                                    if (
                                        isinstance(secret_value, str)
                                        and secret_value.isupper()
                                        and "_" in secret_value
                                    ):
                                        import os

                                        resolved_value = os.getenv(secret_value)
                                        if resolved_value:
                                            secret_value = resolved_value
                                    serialized[field_name] = secret_value
                                except Exception:
                                    # If get_secret_value fails, keep the
                                    # serialized value
                                    pass
            elif hasattr(obj, "__fields__"):
                # Fallback for Pydantic v1 style
                fields = obj.__fields__
                for field_name, field_info in fields.items():
                    field_type = field_info.type_
                    if self._is_secret_str_type(field_type):
                        field_value = getattr(obj, field_name, None)
                        if field_value is not None:
                            try:
                                # Get the actual secret value
                                secret_value = field_value.get_secret_value()
                                # If the secret value looks like an environment
                                # variable name, try to resolve it
                                if (
                                    isinstance(secret_value, str)
                                    and secret_value.isupper()
                                    and "_" in secret_value
                                ):
                                    import os

                                    resolved_value = os.getenv(secret_value)
                                    if resolved_value:
                                        secret_value = resolved_value
                                serialized[field_name] = secret_value
                            except Exception:
                                pass
        except Exception:
            # If field introspection fails, return serialized as-is
            pass

        return serialized

    def _is_secret_str_type(self, field_type: Any) -> bool:
        """Check if a field type is SecretStr or similar secret type.

        Args:
            field_type: The field type annotation

        Returns:
            True if this is a secret type that needs special handling
        """
        try:
            # Handle both direct SecretStr and Optional[SecretStr] cases
            if hasattr(field_type, "__origin__"):
                # Handle Union types (like Optional[SecretStr])
                if (
                    field_type.__origin__ is type(None)
                    or str(field_type.__origin__) == "typing.Union"
                ):
                    if hasattr(field_type, "__args__"):
                        for arg in field_type.__args__:
                            if self._is_secret_str_type(arg):
                                return True

            # Check if it's SecretStr directly
            type_name = getattr(field_type, "__name__", str(field_type))
            return "SecretStr" in type_name or "Secret" in type_name
        except Exception:
            return False

    def _serialize_tool_wrapper(self, tool_wrapper: dict[str, Any]) -> dict[str, Any]:
        """Serialize a tool wrapper by ensuring all nested objects are serialized.

        Args:
            tool_wrapper: Tool wrapper dict from component_tool

        Returns:
            Serialized tool wrapper with all nested objects serialized
        """
        # Tool wrappers are already mostly serialized, but we may need to
        # serialize nested objects within the component code or inputs
        serialized_wrapper = {}

        for key, value in tool_wrapper.items():
            if key in (
                "__tool_wrapper__",
                "component_type",
                "tool_metadata",
                "tool_input_schema",
                "code_blob_id",
            ):
                # These are already serializable
                serialized_wrapper[key] = value
            else:
                # Recursively serialize other values (especially static_inputs)
                serialized_wrapper[key] = self.serialize_langflow_object(value)

        return serialized_wrapper

    def _deserialize_tool_wrapper(self, tool_wrapper: dict[str, Any]) -> Any:
        """Deserialize a tool wrapper into a callable tool object.

        Args:
            tool_wrapper: Serialized tool wrapper

        Returns:
            Callable tool object that can execute the component
        """
        return _create_tool_from_wrapper(tool_wrapper)

    def _convert_lfx_message_to_langflow(self, obj: Any) -> Any:
        """Convert lfx.schema.message.Message to langflow.schema.message.Message.

        Args:
            obj: Object that might be an lfx Message

        Returns:
            Langflow Message if conversion successful, otherwise original object
        """
        try:
            from lfx.schema.message import Message as LfxMessage
            from langflow.schema.message import Message as LangflowMessage

            # Check if this is an lfx Message that needs conversion
            if isinstance(obj, LfxMessage) and not isinstance(obj, LangflowMessage):
                # Convert by extracting all attributes
                return LangflowMessage(
                    text=obj.text if hasattr(obj, "text") else "",
                    sender=obj.sender if hasattr(obj, "sender") else None,
                    sender_name=obj.sender_name if hasattr(obj, "sender_name") else None,
                    session_id=obj.session_id if hasattr(obj, "session_id") else "",
                    files=obj.files if hasattr(obj, "files") else None,
                    properties=obj.properties if hasattr(obj, "properties") else None,
                    content_blocks=obj.content_blocks if hasattr(obj, "content_blocks") else None,
                )
        except ImportError:
            pass

        return obj
