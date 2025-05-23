//! Component information and metadata types.

use serde::{Deserialize, Serialize};
use crate::schema::SchemaRef;

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
}
