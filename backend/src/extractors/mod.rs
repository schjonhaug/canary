//! Custom Axum extractors

mod auth;

pub use auth::{AuthenticatedUser, require_non_demo};
