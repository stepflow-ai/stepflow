//! HTTP server module for StepFlow
//!
//! This module provides a REST API for StepFlow workflow execution.
//! The API is organized into logical modules by functionality:
//! - health: System health endpoints
//! - execution: Ad-hoc workflow execution with debug capabilities
//! - executions: Execution management and querying
//! - endpoints: Named workflow endpoints with optional labels (CRUD)
//! - components: Available component discovery

pub mod api_type;
pub mod common;
pub mod components;
pub mod endpoints;
pub mod error;
pub mod execution;
pub mod executions;
pub mod health;
pub mod startup;

#[cfg(test)]
mod tests;

// Re-export the main server startup function
pub use startup::start_server;
