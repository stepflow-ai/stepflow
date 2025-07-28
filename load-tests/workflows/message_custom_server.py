#!/usr/bin/env python3
"""
Custom Python component server for message manipulation
Used for load testing Stepflow with Python custom components (no OpenAI)
"""

import msgspec
from stepflow_sdk import StepflowStdioServer


class MessageInput(msgspec.Struct):
    """Input schema for message creation"""
    prompt: str
    system_message: str = "You are a helpful assistant."


class MessageOutput(msgspec.Struct):
    """Output schema for message creation"""
    messages: list
    message_count: int
    total_length: int


server = StepflowStdioServer()


@server.component
def create_messages(input: MessageInput) -> MessageOutput:
    """
    Create message structure similar to create_messages builtin
    """
    messages = [
        {"role": "system", "content": input.system_message},
        {"role": "user", "content": input.prompt}
    ]
    
    total_length = len(input.system_message) + len(input.prompt)
    
    return MessageOutput(
        messages=messages,
        message_count=len(messages),
        total_length=total_length
    )


@server.component
def process_text(input: MessageInput) -> MessageOutput:
    """
    Process text with simple manipulations
    """
    processed_prompt = input.prompt.upper().replace(" ", "_")
    processed_system = input.system_message.lower()
    
    messages = [
        {"role": "system", "content": processed_system},
        {"role": "user", "content": processed_prompt},
        {"role": "assistant", "content": f"Processed: {len(input.prompt)} chars"}
    ]
    
    return MessageOutput(
        messages=messages,
        message_count=len(messages),
        total_length=len(processed_prompt) + len(processed_system)
    )


if __name__ == "__main__":
    server.run()