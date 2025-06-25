//! Component information and metadata types.

use crate::schema::SchemaRef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// The input schema for the component.
    ///
    /// Can be any valid JSON schema (object, primitive, array, etc.).
    pub input_schema: SchemaRef,

    /// The output schema for the component.
    ///
    /// Can be any valid JSON schema (object, primitive, array, etc.).
    pub output_schema: SchemaRef,

    /// Optional description of what the component does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
