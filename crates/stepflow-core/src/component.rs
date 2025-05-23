//! Component information and metadata types.

use serde::{Deserialize, Serialize};
use crate::schema::ObjectSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// The input schema for the component.
    ///
    /// This should be a JSON object.
    pub input_schema: ObjectSchema,

    /// The output schema for the component.
    ///
    /// This should be a JSON object.
    pub output_schema: ObjectSchema,
}
