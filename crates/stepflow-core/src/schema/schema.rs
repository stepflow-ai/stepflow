use error_stack::ResultExt as _;
use schemars::{
    JsonSchema,
    schema::{InstanceType, SchemaObject},
};

use super::{Result, SchemaError};

/// Wrapper around a JSON Schema.
#[repr(transparent)]
pub struct SchemaPart<'a>(&'a SchemaObject);

#[repr(transparent)]
pub struct SchemaPartMut<'a>(&'a mut SchemaObject);

/// Wrapper around a JSON object schema.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, JsonSchema)]
#[repr(transparent)]
#[derive(Default)]
pub struct ObjectSchema(SchemaObject);

fn ensure_simple_object(schema: &SchemaObject) -> Result<()> {
    error_stack::ensure!(schema.array.is_none(), SchemaError::Unexpected("array"));
    error_stack::ensure!(
        schema.const_value.is_none(),
        SchemaError::Unexpected("const_values")
    );
    // TODO: Resolve references?
    error_stack::ensure!(
        schema.reference.is_none(),
        SchemaError::Unexpected("reference")
    );
    error_stack::ensure!(
        schema.enum_values.is_none(),
        SchemaError::Unexpected("enum_values")
    );
    error_stack::ensure!(
        schema.has_type(InstanceType::Object),
        SchemaError::Unexpected("instance_type")
    );
    error_stack::ensure!(schema.format.is_none(), SchemaError::Unexpected("format"));
    error_stack::ensure!(schema.number.is_none(), SchemaError::Unexpected("number"));
    error_stack::ensure!(schema.string.is_none(), SchemaError::Unexpected("string"));
    error_stack::ensure!(
        schema.subschemas.is_none(),
        SchemaError::Unexpected("subschema")
    );
    error_stack::ensure!(schema.object.is_some(), SchemaError::MissingObject);
    Ok(())
}

impl ObjectSchema {
    pub fn try_new(schema: SchemaObject) -> Result<Self> {
        ensure_simple_object(&schema)?;

        Ok(Self(schema))
    }

    pub fn parse(s: &str) -> Result<Self> {
        let schema =
            serde_json::from_str::<SchemaObject>(s).change_context(SchemaError::InvalidSchema)?;
        Self::try_new(schema)
    }

    /// Get the field of a specific schema.
    pub fn field(&self, _name: &str) -> Option<SchemaPart<'_>> {
        todo!()
    }

    pub fn field_mut(&mut self, _name: &str) -> Option<SchemaPartMut<'_>> {
        todo!()
    }

    pub fn fields(&self) -> impl Iterator<Item = (&str, &SchemaPart<'_>)> + '_ {
        vec![].into_iter()
    }

    pub fn fields_mut(&mut self) -> impl Iterator<Item = (&str, &mut SchemaPartMut<'_>)> + '_ {
        vec![].into_iter()
    }

    pub fn has_field(&self, name: &str) -> bool {
        self.0
            .object
            .as_ref()
            .expect("should be a map")
            .properties
            .contains_key(name)
    }
}
