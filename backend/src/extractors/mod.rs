//! Custom Axum extractors

mod auth;

pub use auth::{require_non_demo, AuthenticatedUser};
