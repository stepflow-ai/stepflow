use error_stack::ResultExt as _;
use schemars::{
    JsonSchema,
    schema::{InstanceType, SchemaObject},
};
use serde_json::Value;

use crate::{Result, SchemaError};

/// Wrapper around a JSON Schema.
#[repr(transparent)]
pub struct SchemaPart<'a>(&'a SchemaObject);

#[repr(transparent)]
pub struct SchemaPartMut<'a>(&'a mut SchemaObject);

/// Wrapper around a JSON object schema.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Default, JsonSchema)]
#[repr(transparent)]
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

fn extract_uses(uses: Option<&Value>) -> Result<u32> {
    match uses {
        None => Ok(0),
        Some(Value::Number(n)) => {
            let n = n
                .as_u64()
                .ok_or(error_stack::report!(SchemaError::InvalidUses(
                    Value::Number(n.to_owned())
                )))?;
            Ok(n as u32)
        }
        Some(value) => Err(error_stack::report!(SchemaError::InvalidUses(
            value.clone()
        ))),
    }
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

    pub fn uses(&self) -> Result<u32> {
        extract_uses(self.0.extensions.get("uses"))
    }

    pub fn set_uses(&mut self, uses: u32) -> () {
        self.0
            .extensions
            .insert("uses".to_owned(), Value::Number(uses.into()));
    }

    /// Get the field of a specific schema.
    pub fn field(&self, name: &str) -> Option<SchemaPart<'_>> {
        todo!()
    }

    pub fn field_mut(&mut self, name: &str) -> Option<SchemaPartMut<'_>> {
        todo!()
    }

    pub fn fields(&self) -> impl Iterator<Item = (&str, &SchemaPart<'_>)> + '_ {
        vec![].into_iter()
    }

    pub fn fields_mut(&mut self) -> impl Iterator<Item = (&str, &mut SchemaPartMut<'_>)> + '_ {
        vec![].into_iter()
    }
}

impl<'a> SchemaPart<'a> {
    pub fn uses(&self) -> Result<u32> {
        extract_uses(self.0.extensions.get("uses"))
    }
}

impl<'a> SchemaPartMut<'a> {
    pub fn uses(&self) -> Result<u32> {
        extract_uses(self.0.extensions.get("uses"))
    }

    pub fn set_uses(&mut self, uses: u32) -> () {
        self.0
            .extensions
            .insert("uses".to_owned(), Value::Number(uses.into()));
    }
}
