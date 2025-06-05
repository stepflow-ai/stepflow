use error_stack::ResultExt as _;
use openai_api_rs::v1::{api::OpenAIClient, chat_completion};
use serde::{Deserialize, Serialize};
use stepflow_core::{FlowResult, component::ComponentInfo, schema::SchemaRef, workflow::ValueRef};
use stepflow_plugin::ExecutionContext;

use crate::{BuiltinComponent, Result, error::BuiltinError};

/// OpenAI component for generating text responses
pub struct OpenAIComponent {
    model: String,
}

impl OpenAIComponent {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }
}

/// Input for the OpenAI component
#[derive(Serialize, Deserialize, schemars::JsonSchema, Default)]
struct OpenAIInput {
    /// The messages to send to the API
    messages: Vec<ChatMessage>,

    /// Max tokens to generate (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u16>,

    /// Temperature setting (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,

    /// API key override (optional - uses environment variable if not provided)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

/// Chat message format
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChatMessage {
    /// The role of the message sender (system, user, assistant)
    pub role: ChatMessageRole,

    /// The content of the message
    pub content: String,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatMessageRole {
    System,
    User,
    Assistant,
}

impl From<ChatMessageRole> for chat_completion::MessageRole {
    fn from(role: ChatMessageRole) -> Self {
        match role {
            ChatMessageRole::System => chat_completion::MessageRole::system,
            ChatMessageRole::User => chat_completion::MessageRole::user,
            ChatMessageRole::Assistant => chat_completion::MessageRole::assistant,
        }
    }
}

/// Output from the OpenAI component
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct OpenAIOutput {
    /// The generated response text
    response: String,
}

impl BuiltinComponent for OpenAIComponent {
    fn component_info(&self) -> Result<ComponentInfo> {
        let input_schema = SchemaRef::for_type::<OpenAIInput>();
        let output_schema = SchemaRef::for_type::<OpenAIOutput>();

        Ok(ComponentInfo {
            input_schema,
            output_schema,
            description: Some(
                "Send messages to OpenAI's chat completion API and get a response".to_string(),
            ),
        })
    }

    async fn execute(&self, _context: ExecutionContext, input: ValueRef) -> Result<FlowResult> {
        let input: OpenAIInput = input
            .deserialize()
            .change_context(BuiltinError::InvalidInput)?;

        let api_key = if let Some(api_key) = &input.api_key {
            api_key.to_owned()
        } else if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
            api_key
        } else {
            error_stack::bail!(BuiltinError::MissingValueOrEnv("api_key", "OPENAI_API_KEY"));
        };

        let mut client = OpenAIClient::builder()
            .with_api_key(api_key)
            .build()
            .map_err(|e| {
                tracing::error!("OpenAI client error: {:?}", e);
                BuiltinError::OpenAI(e.to_string())
            })?;

        let messages = input
            .messages
            .into_iter()
            .map(|m| chat_completion::ChatCompletionMessage {
                role: m.role.into(),
                content: chat_completion::Content::Text(m.content),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            })
            .collect();

        let req = chat_completion::ChatCompletionRequest::new(self.model.clone(), messages);

        let result = client
            .chat_completion(req)
            .await
            .change_context_lazy(|| BuiltinError::OpenAI("generating completion".to_owned()))?;

        let response = result
            .choices
            .first()
            .ok_or_else(|| BuiltinError::OpenAI("missing completion".to_owned()))?;
        let response = response.message.content.clone().unwrap_or_default();
        let output = OpenAIOutput { response };
        let output = serde_json::to_value(output).change_context(BuiltinError::Internal)?;
        Ok(output.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_context::MockContext;

    #[tokio::test]
    #[test_with::env(OPENAI_API_KEY)]
    async fn test_openai_component() {
        let component = OpenAIComponent::new("gpt-3.5-turbo");
        let input = OpenAIInput {
            messages: vec![ChatMessage {
                role: ChatMessageRole::User,
                content: "Say Hello".to_string(),
            }],
            ..OpenAIInput::default()
        };

        let input = serde_json::to_value(input).unwrap();
        let mock = MockContext::new();
        let output = component
            .execute(mock.execution_context(), input.into())
            .await
            .unwrap();

        let output = output.success().unwrap();
        let object = output.as_object().unwrap();
        let response = object.get("response").unwrap();
        let response = response.as_str().unwrap();
        assert!(response.contains("Hello"));
    }
}
