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

# Copyright {{ year }} {{ authors }}
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

"""
Custom Python component server for OpenAI API calls
Used for load testing Stepflow with Python custom components
"""

import os
import msgspec
import openai
from stepflow_py import StepflowStdioServer


class OpenAIChatInput(msgspec.Struct):
    """Input schema for OpenAI chat completion"""

    prompt: str
    system_message: str = "You are a helpful assistant."
    max_tokens: int = 150
    temperature: float = 0.7


class OpenAIChatOutput(msgspec.Struct):
    """Output schema for OpenAI chat completion"""

    response: str
    model: str
    usage: dict


server = StepflowStdioServer()


@server.component
def openai_chat(input: OpenAIChatInput) -> OpenAIChatOutput:
    """
    Call OpenAI chat completion API with the provided prompt
    """
    # Initialize OpenAI client
    client = openai.OpenAI(api_key=os.getenv("OPENAI_API_KEY"))

    try:
        # Make API call
        response = client.chat.completions.create(
            model="gpt-4o-mini",
            messages=[
                {"role": "system", "content": input.system_message},
                {"role": "user", "content": input.prompt},
            ],
            max_tokens=input.max_tokens,
            temperature=input.temperature,
        )

        return OpenAIChatOutput(
            response=response.choices[0].message.content,
            model=response.model,
            usage={
                "prompt_tokens": response.usage.prompt_tokens,
                "completion_tokens": response.usage.completion_tokens,
                "total_tokens": response.usage.total_tokens,
            },
        )
    except Exception as e:
        # Return error as part of response for debugging
        return OpenAIChatOutput(
            response=f"Error: {str(e)}",
            model="error",
            usage={"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
        )


if __name__ == "__main__":
    server.run()
