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

/// A single entry in an error stack for detailed error reporting
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ErrorStackEntry {
    pub error: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backtrace: Option<String>,
}

/// Error stack information for detailed system error debugging
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ErrorStack {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub stack: Vec<ErrorStackEntry>,
}

impl ErrorStack {
    /// Create an ErrorStack from an error_stack::Report, preserving the full stack trace
    pub fn from_error_stack<T: error_stack::Context>(report: error_stack::Report<T>) -> Self {
        let mut stack_entries: Vec<ErrorStackEntry> = Vec::new();
        let mut current_attachments: Vec<String> = Vec::new();

        // Extract global backtrace if available
        let global_backtrace = {
            let mut backtrace_iter = report.frames().filter_map(|frame| {
                frame
                    .sources()
                    .iter()
                    .find_map(|source| source.downcast_ref::<std::backtrace::Backtrace>())
            });
            backtrace_iter.next().map(|bt| bt.to_string())
        };

        for frame in report.frames() {
            match frame.kind() {
                error_stack::FrameKind::Context(context) => {
                    // If we have accumulated attachments, add them to the previous entry
                    if !current_attachments.is_empty() && !stack_entries.is_empty() {
                        if let Some(last_entry) = stack_entries.last_mut() {
                            last_entry.attachments.append(&mut current_attachments);
                        }
                    }

                    // Add the context as a new stack entry
                    // Only include backtrace on the first (top-level) error to avoid duplication
                    let backtrace = if stack_entries.is_empty() {
                        global_backtrace.clone()
                    } else {
                        None
                    };

                    stack_entries.push(ErrorStackEntry {
                        error: context.to_string(),
                        attachments: vec![],
                        backtrace,
                    });
                }
                error_stack::FrameKind::Attachment(attachment_kind) => match attachment_kind {
                    error_stack::AttachmentKind::Printable(printable) => {
                        current_attachments.push(printable.to_string());
                    }
                    _ => {
                    }
                },
            }
        }

        // Add any remaining attachments to the last entry
        if !current_attachments.is_empty() && !stack_entries.is_empty() {
            if let Some(last_entry) = stack_entries.last_mut() {
                last_entry.attachments.extend(current_attachments);
            }
        }

        Self {
            stack: stack_entries,
        }
    }
}

impl<T: error_stack::Context> From<error_stack::Report<T>> for ErrorStack {
    fn from(report: error_stack::Report<T>) -> Self {
        Self::from_error_stack(report)
    }
}
