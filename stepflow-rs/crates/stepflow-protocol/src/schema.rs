//! The schema for protocol messages.

pub mod component_execute;
pub mod component_info;
pub mod get_blob;
pub mod initialization;
pub mod list_components;
mod messages;
pub mod put_blob;

pub use messages::*;
