use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Diagnostic level indicating severity and impact
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticLevel {
    /// Fatal: Prevents analysis from proceeding
    Fatal = 0,
    /// Error: Will definitely fail during execution
    Error = 1,
    /// Warning: Likely to cause problems during execution
    Warning = 2,
}
