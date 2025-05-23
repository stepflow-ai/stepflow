//! Schema manipulation and validation types.

mod error;
mod schema;

pub use self::error::{Result, SchemaError};
pub use self::schema::{ObjectSchema, SchemaPart, SchemaPartMut};
