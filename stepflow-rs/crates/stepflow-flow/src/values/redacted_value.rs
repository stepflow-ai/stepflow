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

use std::{collections::HashMap, fmt};

use crate::schema::SchemaRef;

#[derive(Debug, PartialEq, Clone)]
pub struct Secrets {
    is_secret: bool,
    fields: Option<HashMap<String, Secrets>>,
}

impl Default for Secrets {
    fn default() -> Self {
        Self::new()
    }
}

impl Secrets {
    const EMPTY: Secrets = Secrets {
        is_secret: false,
        fields: None,
    };

    pub const fn empty() -> &'static Self {
        &Self::EMPTY
    }

    pub fn new() -> Self {
        Self {
            is_secret: false,
            fields: None,
        }
    }

    /// Return true if the current value is marked as a secret.
    pub fn is_secret(&self) -> bool {
        self.is_secret
    }

    /// Get secret information about the given field, if any.
    pub fn field(&'_ self, field_name: &str) -> &'_ Self {
        if let Some(fields) = &self.fields {
            fields.get(field_name).unwrap_or(&Self::EMPTY)
        } else {
            &Self::EMPTY
        }
    }

    fn reduce(mut self) -> Option<Self> {
        // Assumption: All fields have already been reduced

        // First, simplify the fields. We only need to keep things that either
        // mark secrets or have nested secret fields.
        self.fields = self.fields.and_then(|mut fields| {
            fields.retain(|_, v| v.is_secret || v.fields.is_some());
            if fields.is_empty() {
                None
            } else {
                Some(fields)
            }
        });

        // Second, simplify this node. We need it if it's marked as a secret
        // or if it has any fields (which are reduced and must lead to a secret).
        if self.is_secret || self.fields.is_some() {
            Some(self)
        } else {
            None
        }
    }

    pub fn add_field(&mut self, field_name: &str, secrets: Secrets) {
        let Some(secrets) = secrets.reduce() else {
            // No secrets in this subtree, nothing to add
            return;
        };

        if self.fields.is_none() {
            self.fields = Some(HashMap::new());
        }
        let fields = self.fields.as_mut().unwrap();
        fields.insert(field_name.to_string(), secrets);
    }

    pub fn add_secret_field(&mut self, field_name: &str) {
        self.add_field(
            field_name,
            Secrets {
                is_secret: true,
                fields: None,
            },
        );
    }

    pub fn from_schema(schema: &serde_json::Value) -> Secrets {
        let schema = schema.as_object().expect("Schema must be an object");
        let type_name = schema.get("type").and_then(|s| s.as_str());

        let mut secrets = match type_name {
            Some("object") => {
                let mut secrets = Secrets::new();
                if let Some(properties) = schema.get("properties").and_then(|s| s.as_object()) {
                    for (prop_name, prop_schema) in properties {
                        let mut prop_secrets = Self::from_schema(prop_schema);
                        prop_secrets.is_secret = prop_schema
                            .get("is_secret")
                            .and_then(|s| s.as_bool())
                            .unwrap_or(false);
                        secrets.add_field(prop_name, prop_secrets);
                    }
                }
                secrets
            }
            Some("array") => {
                if let Some(items) = schema.get("items") {
                    Secrets::from_schema(items)
                } else {
                    Secrets::new()
                }
            }
            _ => {
                // If it's not an object or array, it can't have nested secrets.
                Secrets::new()
            }
        };

        secrets.is_secret = schema
            .get("is_secret")
            .and_then(|s| s.as_bool())
            .unwrap_or(false);

        secrets
    }

    pub fn redacted<'a>(&'a self, value: &'a serde_json::Value) -> RedactedValue<'a> {
        RedactedValue::new(value, self)
    }
}

impl From<&SchemaRef> for Secrets {
    fn from(value: &SchemaRef) -> Self {
        let secrets = Secrets::from_schema(value.as_value());
        secrets.reduce().unwrap_or_default()
    }
}

/// A wrapper for ValueRef that implements safe Display formatting with secret redaction.
///
/// This ensures that values marked as secrets in the schema are redacted when printed,
/// preventing accidental exposure of sensitive data in logs, error messages, and debug output.
pub struct RedactedValue<'a>(&'a serde_json::Value, &'a Secrets);

impl<'a> RedactedValue<'a> {
    pub fn new(value: &'a serde_json::Value, secrets: &'a Secrets) -> Self {
        Self(value, secrets)
    }
}

impl<'a> fmt::Display for RedactedValue<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl<'a> fmt::Debug for RedactedValue<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.0;
        let secrets = self.1;

        match value {
            _ if secrets.is_secret => {
                write!(f, "\"[REDACTED]\"")
            }
            serde_json::Value::Object(obj) => {
                let mut f = f.debug_map();
                for (key, val) in obj {
                    let secrets = secrets.field(key);
                    f.entry(key, &RedactedValue(val, secrets));
                }
                f.finish()
            }
            serde_json::Value::Array(arr) => {
                let mut f = f.debug_list();
                let secrets = secrets.field("$item");
                for val in arr {
                    f.entry(&RedactedValue(val, secrets));
                }
                f.finish()
            }
            _ => {
                // For primitive values, just write them directly
                write!(f, "{}", value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::values::ValueRef;

    use super::*;
    use serde_json::json;

    #[test]
    fn test_secrets_from_schema() {
        let secrets = Secrets::from_schema(&json!({
            "type": "object",
            "properties": {
                "username": {
                    "type": "string"
                },
                "password": {
                    "type": "string",
                    "is_secret": true
                },
                "api_key": {
                    "type": "string",
                    "is_secret": true
                },
                "config": {
                    "type": "object",
                    "properties": {
                        "timeout": {
                            "type": "number"
                        },
                        "secret_token": {
                            "type": "string",
                            "is_secret": true
                        }
                    }
                },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string"},
                            "secret_data": {"type": "string", "is_secret": true}
                        }
                    }
                }
            }
        }));

        assert!(!secrets.is_secret);
        assert!(!secrets.field("username").is_secret());
        assert!(secrets.field("password").is_secret());
        assert!(secrets.field("api_key").is_secret());

        let config_secrets = secrets.field("config");
        assert!(!config_secrets.is_secret());
        assert!(!config_secrets.field("timeout").is_secret());
        assert!(config_secrets.field("secret_token").is_secret());

        let items_secrets = secrets.field("items");
        assert!(!items_secrets.is_secret());
        assert!(!items_secrets.field("id").is_secret());
        assert!(items_secrets.field("secret_data").is_secret());
    }

    #[test]
    fn test_secret_array_from_schema() {
        let secrets = Secrets::from_schema(&json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {"type": "string"},
                            "secret_data": {"type": "string", "is_secret": true}
                        }
                    },
                    "is_secret": true,
                }
            }
        }));

        assert!(!secrets.is_secret);
        let items_secrets = secrets.field("items");
        assert!(items_secrets.is_secret());
        assert!(!items_secrets.field("id").is_secret());
        assert!(items_secrets.field("secret_data").is_secret());
    }

    #[test]
    fn test_secrets_from_primitive_secret() {
        let secret_schema = SchemaRef::parse_json(
            r#"{
            "type": "string",
            "is_secret": true
        }"#,
        )
        .unwrap();

        let secrets = Secrets::from(&secret_schema);
        assert!(secrets.is_secret());
        assert!(secrets.fields.is_none());
    }

    fn create_test_secrets() -> Secrets {
        let mut secrets = Secrets::new();
        secrets.add_secret_field("password");
        secrets.add_secret_field("api_key");

        let mut config_secrets = Secrets::new();
        config_secrets.add_secret_field("secret_token");
        secrets.add_field("config", config_secrets);

        let mut item_secrets = Secrets::new();
        item_secrets.add_secret_field("secret_data");
        let mut items_secrets = Secrets::new();
        items_secrets.add_field("$item", item_secrets);
        secrets.add_field("items", items_secrets);

        secrets
    }

    #[test]
    fn test_redact_secrets_in_object() {
        let value_json = json!({
            "username": "alice",
            "password": "secret123",
            "api_key": "sk-abcd1234"
        });

        let value = ValueRef::new(value_json);

        insta::assert_snapshot!(value.redacted(&create_test_secrets()), @r#"{"username": "alice", "password": "[REDACTED]", "api_key": "[REDACTED]"}"#);
    }

    #[test]
    fn test_redacted_nested_secrets() {
        let value_json = json!({
            "username": "alice",
            "config": {
                "timeout": 30,
                "secret_token": "token123"
            }
        });

        let value = ValueRef::new(value_json);
        insta::assert_snapshot!(value.redacted(&create_test_secrets()), @r#"{"username": "alice", "config": {"timeout": 30, "secret_token": "[REDACTED]"}}"#);
    }

    #[test]
    fn test_redacted_array_with_secret_fields() {
        let value_json = json!({
            "items": [
                {"id": "1", "secret_data": "data1"},
                {"id": "2", "secret_data": "data2"}
            ]
        });

        let value = ValueRef::new(value_json);
        insta::assert_snapshot!(value.redacted(&create_test_secrets()), @r#"{"items": [{"id": "1", "secret_data": "[REDACTED]"}, {"id": "2", "secret_data": "[REDACTED]"}]}"#);
    }

    #[test]
    fn test_redacted_secret_array() {
        let mut secrets = Secrets::new();
        secrets.add_secret_field("items");

        let value_json = json!({
            "items": [
                {"id": "1", "secret_data": "data1"},
                {"id": "2", "secret_data": "data2"}
            ]
        });

        let value = ValueRef::new(value_json);
        insta::assert_snapshot!(value.redacted(&secrets), @r#"{"items": "[REDACTED]"}"#);
    }

    #[test]
    fn test_no_schema_shows_all_values() {
        let value_json = json!({
            "password": "secret123",
            "username": "alice"
        });

        let value = ValueRef::new(value_json);
        insta::assert_snapshot!(value.redacted(&Secrets::EMPTY), @r#"{"password": "secret123", "username": "alice"}"#);
    }

    #[test]
    fn test_redacted_primitive_secret() {
        // Test a primitive value that's marked as secret
        let secrets = Secrets {
            is_secret: true,
            fields: None,
        };

        let value = ValueRef::new(json!("secret_token"));
        insta::assert_snapshot!(value.redacted(&secrets), @r#""[REDACTED]""#);
    }
}
