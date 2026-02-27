//! Utility modules for the Canary backend

pub mod descriptor;

pub use descriptor::{
    extract_address_from_descriptor, parse_multipath_descriptor, strip_key_origin,
};
