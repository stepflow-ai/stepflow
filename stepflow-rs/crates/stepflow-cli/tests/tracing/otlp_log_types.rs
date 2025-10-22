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

#![allow(dead_code)]

//! Minimal OTLP log type definitions for parsing JSONL log exports

use serde::Deserialize;

/// Top-level OTLP logs export structure
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtlpLogs {
    pub resource_logs: Vec<ResourceLogs>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLogs {
    pub scope_logs: Vec<ScopeLogs>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeLogs {
    pub log_records: Vec<LogRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    #[serde(rename = "timeUnixNano")]
    pub time_unix_nano: String,
    pub severity_text: String,
    pub body: LogBody,
    pub attributes: Vec<KeyValue>,
    #[serde(default)]
    pub trace_id: String,
    #[serde(default)]
    pub span_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogBody {
    pub string_value: String,
}

#[derive(Debug, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Value {
    #[serde(default)]
    pub string_value: Option<String>,
    #[serde(default)]
    pub int_value: Option<String>,
    #[serde(default)]
    pub bool_value: Option<bool>,
}

impl OtlpLogs {
    /// Get all log records across all resource/scope logs
    pub fn all_log_records(&self) -> impl Iterator<Item = &LogRecord> {
        self.resource_logs
            .iter()
            .flat_map(|r| r.scope_logs.iter())
            .flat_map(|s| s.log_records.iter())
    }
}

impl LogRecord {
    /// Get the message body
    pub fn message(&self) -> &str {
        &self.body.string_value
    }

    /// Get attribute value by key
    pub fn get_attribute(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|kv| kv.key == key)
            .and_then(|kv| kv.value.string_value.as_deref())
    }

    /// Check if this log belongs to a specific trace
    pub fn belongs_to_trace(&self, trace_id: &str) -> bool {
        !self.trace_id.is_empty()
            && self.trace_id.to_lowercase() == trace_id.to_lowercase()
    }

    /// Get the normalized trace_id (lowercase)
    pub fn trace_id(&self) -> String {
        self.trace_id.to_lowercase()
    }

    /// Get the normalized span_id (lowercase)
    pub fn span_id(&self) -> String {
        self.span_id.to_lowercase()
    }
}

impl std::fmt::Display for LogRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LogRecord {{ severity: '{}', message: '{}', trace_id: {}, span_id: {} }}",
            self.severity_text,
            self.message(),
            if self.trace_id.is_empty() {
                "NONE".to_string()
            } else {
                format!("{:.8}", self.trace_id)
            },
            if self.span_id.is_empty() {
                "NONE".to_string()
            } else {
                format!("{:.8}", self.span_id)
            }
        )
    }
}
