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
Custom Python component server for message manipulation
Used for load testing Stepflow with Python custom components (no OpenAI)
"""

import asyncio
import sys

import msgspec
from stepflow_worker import StepflowServer


class MessageInput(msgspec.Struct):
    """Input schema for message creation"""

    prompt: str
    system_message: str = "You are a helpful assistant."


class MessageOutput(msgspec.Struct):
    """Output schema for message creation"""

    messages: list
    message_count: int
    total_length: int


server = StepflowServer()


@server.component
def create_messages(input: MessageInput) -> MessageOutput:
    """
    Create message structure similar to create_messages builtin
    """
    messages = [
        {"role": "system", "content": input.system_message},
        {"role": "user", "content": input.prompt},
    ]

    total_length = len(input.system_message) + len(input.prompt)

    return MessageOutput(
        messages=messages, message_count=len(messages), total_length=total_length
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
        {"role": "assistant", "content": f"Processed: {len(input.prompt)} chars"},
    ]

    return MessageOutput(
        messages=messages,
        message_count=len(messages),
        total_length=len(processed_prompt) + len(processed_system),
    )


if __name__ == "__main__":
    asyncio.run(server.run())
