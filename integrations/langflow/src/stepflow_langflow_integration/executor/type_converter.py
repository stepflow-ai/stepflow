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

"""Type conversion between Langflow and Stepflow formats."""

from typing import Any, Dict, Optional, Type


class TypeConverter:
    """Converts between Langflow and Stepflow type representations."""
    
    def serialize_langflow_object(self, obj: Any) -> Any:
        """Serialize Langflow objects with type metadata.
        
        Args:
            obj: Langflow object to serialize
            
        Returns:
            Serialized object with type metadata
        """
        # Import Langflow types dynamically to avoid import errors
        try:
            from langflow.schema.message import Message
            from langflow.schema.data import Data
            from langflow.schema.dataframe import DataFrame
        except ImportError:
            # Fallback if Langflow not available
            return obj
        
        if isinstance(obj, Message):
            serialized = obj.model_dump(mode="json")
            serialized["__langflow_type__"] = "Message"
            return serialized
        elif isinstance(obj, Data):
            serialized = obj.model_dump(mode="json")
            serialized["__langflow_type__"] = "Data"
            return serialized
        elif isinstance(obj, DataFrame):
            try:
                # Convert DataFrame to serializable format
                data_list = (
                    obj.to_data_list() if hasattr(obj, "to_data_list") 
                    else obj.to_dict("records")
                )
                return {
                    "__langflow_type__": "DataFrame",
                    "data": [
                        item.model_dump(mode="json") if hasattr(item, "model_dump") 
                        else item for item in data_list
                    ],
                    "text_key": getattr(obj, "text_key", "text"),
                    "default_value": getattr(obj, "default_value", ""),
                }
            except Exception:
                # Fallback to basic serialization
                return {
                    "__langflow_type__": "DataFrame",
                    "data": obj.to_dict("records") if hasattr(obj, "to_dict") else str(obj)
                }
        elif isinstance(obj, (str, int, float, bool, list, dict, type(None))):
            # Simple serializable types
            return obj
        else:
            # Complex object that can't be serialized
            raise ValueError(
                f"Cannot serialize object of type {type(obj)}. "
                "Consider using component fusion for complex objects."
            )
    
    def deserialize_to_langflow_type(
        self, 
        obj: Any, 
        expected_type: Optional[Type] = None
    ) -> Any:
        """Deserialize objects back to Langflow types.
        
        Args:
            obj: Object to deserialize
            expected_type: Expected Langflow type
            
        Returns:
            Deserialized Langflow object
        """
        if not isinstance(obj, dict):
            return obj
        
        # Import Langflow types dynamically
        try:
            from langflow.schema.message import Message
            from langflow.schema.data import Data
            from langflow.schema.dataframe import DataFrame
        except ImportError:
            return obj
        
        langflow_type = obj.get("__langflow_type__")
        if langflow_type:
            # Remove type metadata
            obj_data = {k: v for k, v in obj.items() if k != "__langflow_type__"}
            
            if langflow_type == "Message":
                return Message(**obj_data)
            elif langflow_type == "Data":
                return Data(**obj_data)
            elif langflow_type == "DataFrame":
                try:
                    data_list = obj_data.get("data", [])
                    text_key = obj_data.get("text_key", "text")
                    default_value = obj_data.get("default_value", "")
                    
                    # Convert back to Data objects if needed
                    if data_list and isinstance(data_list[0], dict):
                        data_objects = [
                            Data(**item) if "__langflow_type__" not in item
                            else Data(**{k: v for k, v in item.items() if k != "__langflow_type__"})
                            for item in data_list
                        ]
                    else:
                        data_objects = data_list
                    
                    return DataFrame(
                        data=data_objects,
                        text_key=text_key,
                        default_value=default_value
                    )
                except Exception:
                    # Failed to reconstruct, return raw data
                    return obj_data.get("data", obj_data)
        
        # Use expected type if provided
        if expected_type:
            if expected_type == Message:
                return Message(**obj)
            elif expected_type == Data:
                return Data(**obj)
        
        return obj