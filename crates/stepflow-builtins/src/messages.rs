use error_stack::ResultExt as _;
use serde::{Deserialize, Serialize};
use stepflow_core::FlowResult;
use stepflow_core::{component::ComponentInfo, schema::SchemaRef, workflow::ValueRef};
use stepflow_plugin::ExecutionContext;

use crate::openai::{ChatMessage, ChatMessageRole};
use crate::{BuiltinComponent, Result, error::BuiltinError};

pub struct CreateMessagesComponent;

#[derive(Serialize, Deserialize, schemars::JsonSchema, Default)]
struct CreateMessagesInput {
    /// The system instructions to include in the message list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    system_instructions: Option<String>,

    /// The user prompt to include in the message list.
    user_prompt: String,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, Default)]

struct CreateMessagesOutput {
    messages: Vec<ChatMessage>,
}

impl BuiltinComponent for CreateMessagesComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<CreateMessagesInput>();
        let output_schema = SchemaRef::for_type::<CreateMessagesOutput>();

        Ok(ComponentInfo {
            input_schema,
            output_schema,
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
        let output =
            serde_json::from_value::<CreateMessagesOutput>(output.success().unwrap().clone())
                .unwrap();
        assert_eq!(output.messages.len(), 2);
        assert_eq!(output.messages[0].role, ChatMessageRole::System);
        assert_eq!(output.messages[1].role, ChatMessageRole::User);
    }
}
