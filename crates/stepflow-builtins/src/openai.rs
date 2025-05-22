use error_stack::ResultExt as _;
use openai_api_rs::v1::{api::OpenAIClient, chat_completion};
use serde::{Deserialize, Serialize};
use stepflow_protocol::component_info::ComponentInfo;
use stepflow_schema::ObjectSchema;
use stepflow_workflow::Value;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u16>,

    /// Temperature setting (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,

    /// API key override (optional - uses environment variable if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

/// Chat message format
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct ChatMessage {
    /// The role of the message sender (system, user, assistant)
    role: ChatMessageRole,

    /// The content of the message
    content: String,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ChatMessageRole {
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
        let input_schema = schemars::schema_for!(OpenAIInput);
        let output_schema = schemars::schema_for!(OpenAIOutput);

        tracing::info!("Input schema: {:?}", input_schema);
        tracing::info!("Output schema: {:?}", output_schema);

        let input_schema =
            ObjectSchema::try_new(input_schema.schema).change_context(BuiltinError::Internal)?;
        let output_schema =
            ObjectSchema::try_new(output_schema.schema).change_context(BuiltinError::Internal)?;
        Ok(ComponentInfo {
            input_schema,
            output_schema,
        })
    }

    async fn execute(&self, input: Value) -> Result<Value> {
        // DO NOT SUBMIT: untangle value?
        let input: OpenAIInput = serde_json::from_value(input.as_ref().clone())
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

    #[test]
    fn test_openai_component_info() {
        let component = OpenAIComponent::new("gpt-3.5-turbo");
        let info = component.component_info().unwrap();
        assert!(info.input_schema.has_field("messages"));
        assert!(info.output_schema.has_field("response"));
    }

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
        let output = component.execute(input.into()).await.unwrap();

        let output = serde_json::from_value::<OpenAIOutput>(output.as_ref().clone()).unwrap();
        assert!(output.response.contains("Hello"));
    }
}
