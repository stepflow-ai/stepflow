//! StepFlow HTTP server library
//!
//! This crate provides the core HTTP server functionality for StepFlow.
//! It contains all the API endpoints, request/response types, and server startup logic.

// Server modules
pub mod components;
pub mod debug;
pub mod endpoints;
pub mod error;
pub mod executions;
pub mod health;
pub mod startup;
pub mod workflows;

// Re-export the main server function
pub use startup::start_server;
