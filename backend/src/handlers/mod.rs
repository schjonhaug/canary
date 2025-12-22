//! API request handlers organized by domain

mod providers;
mod user_preferences;

pub use providers::get_providers;
pub use user_preferences::{get_user_preferences, update_user_preferences};
