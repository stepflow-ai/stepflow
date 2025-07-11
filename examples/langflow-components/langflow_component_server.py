#!/usr/bin/env python3
"""
Minimal Langflow component server for StepFlow integration.

This demonstrates a basic langflow component that can be used
with StepFlow workflows.
"""

from stepflow_langflow import StepflowLangflowServer
from stepflow_sdk import StepflowContext
import msgspec
from typing import Dict, Any, List, Optional

# Create the server
server = StepflowLangflowServer(default_protocol_prefix="langflow")

# Langflow domain types
class TextProcessorInput(msgspec.Struct):
    """Input for text processing component."""
    text: str

class TextProcessorOutput(msgspec.Struct):
    """Output for text processing component."""
    processed_text: str

class DataTransformerInput(msgspec.Struct):
    """Input for data transformation component."""
    name: str
    records: List[Dict[str, Any]]
    count: int

class DataTransformerOutput(msgspec.Struct):
    """Output for data transformation component."""
    original: Dict[str, Any]
    metadata: Dict[str, Any]

class DataStorageInput(msgspec.Struct):
    """Input for data storage component."""
    processed_text: str
    transformed_data: Dict[str, Any]

class DataStorageOutput(msgspec.Struct):
    """Output for data storage component."""
    blob_id: str
    message: str

# Langflow components
@server.component
def text_processor(input: TextProcessorInput) -> TextProcessorOutput:
    """Process text data - uppercase transformation."""
    
    processed_text = input.text.upper()
    
    return TextProcessorOutput(processed_text=processed_text)

@server.component
def data_transformer(input: DataTransformerInput) -> DataTransformerOutput:
    """Transform data by adding metadata."""
    
    original_data = {
        "name": input.name,
        "records": input.records,
        "count": input.count
    }
    
    metadata = {
        "processed_by": "langflow_component_server",
        "data_size": len(str(original_data))
    }
    
    return DataTransformerOutput(
        original=original_data,
        metadata=metadata
    )

@server.component
async def data_storage(input: DataStorageInput, context: StepflowContext) -> DataStorageOutput:
    """Store data as a blob and return reference."""
    
    # Prepare data for storage
    data_to_store = {
        "processed_text": input.processed_text,
        "transformed_data": input.transformed_data
    }
    
    # Store the input data as a blob
    blob_id = await context.put_blob(data_to_store)
    
    return DataStorageOutput(
        blob_id=blob_id,
        message=f"Data stored with ID: {blob_id}"
    )

if __name__ == "__main__":
    server.run()