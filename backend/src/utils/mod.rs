//! Utility modules for the Canary backend

pub mod descriptor;

pub use descriptor::{parse_multipath_descriptor, strip_key_origin};
