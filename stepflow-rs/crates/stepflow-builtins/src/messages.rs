// Copyright 2025 DataStax Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except
// in compliance with the License. You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software distributed under the License
// is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express
// or implied. See the License for the specific language governing permissions and limitations under
// the License.

use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::FlowResult;
use stepflow_core::workflow::Component;
use stepflow_core::{component::ComponentInfo, schema::SchemaRef, workflow::ValueRef};
use stepflow_plugin::ExecutionContext;

use crate::openai::{ChatMessage, ChatMessageRole};
use crate::{BuiltinComponent, Result, error::BuiltinError};

pub struct CreateMessagesComponent;

#[derive(Serialize, Deserialize, utoipa::ToSchema, Default)]
struct CreateMessagesInput {
    /// The system instructions to include in the message list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    system_instructions: Option<String>,

    /// The user prompt to include in the message list.
    user_prompt: String,
}

#[derive(Serialize, Deserialize, utoipa::ToSchema, Default)]

struct CreateMessagesOutput {
    messages: Vec<ChatMessage>,
}

impl BuiltinComponent for CreateMessagesComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<CreateMessagesInput>();
        let output_schema = SchemaRef::for_type::<CreateMessagesOutput>();

        Ok(ComponentInfo {
            component: Component::from_string("/create_messages"),
            input_schema: Some(input_schema),
            output_schema: Some(output_schema),
            description: Some(
                "Create a chat message list from system instructions and user prompt".to_string(),
            ),
        })
    }

    async fn execute(&self, _context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
        let CreateMessagesInput {
            system_instructions,
            user_prompt,
        } = serde_json::from_value(input.as_ref().clone())
            .change_context(BuiltinError::InvalidInput)?;

        let mut messages = Vec::with_capacity(2);
        if let Some(system_instructions) = system_instructions {
            messages.push(ChatMessage {
                role: ChatMessageRole::System,
                content: system_instructions,
            });
        }
        messages.push(ChatMessage {
            role: ChatMessageRole::User,
            content: user_prompt,
        });

        let result = CreateMessagesOutput { messages };
        let output = serde_json::to_value(result).change_context(BuiltinError::Internal)?;
        Ok(output.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;

    #[tokio::test]
    async fn test_create_messages_component() {
        let component = CreateMessagesComponent;
        let input = CreateMessagesInput {
            system_instructions: Some("You are a helpful assistant.".to_string()),
            user_prompt: "What is the capital of the moon?".to_string(),
        };
        let input = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();
        let output = component
            .execute(mock.execution_context(), input.into())
            .await
            .unwrap();
        let output: CreateMessagesOutput = output.success().unwrap().deserialize().unwrap();
        assert_eq!(output.messages.len(), 2);
        assert_eq!(output.messages[0].role, ChatMessageRole::System);
        assert_eq!(output.messages[1].role, ChatMessageRole::User);
    }
}
