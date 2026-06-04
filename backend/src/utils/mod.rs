//! Utility modules for the Canary backend

pub mod descriptor;
pub mod time;

pub use descriptor::{
    extract_address_from_descriptor, extract_pubkey_from_descriptor, parse_multipath_descriptor,
    strip_key_origin, DescriptorError,
};
pub use time::current_unix_timestamp;
