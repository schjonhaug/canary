//! Utility modules for the Canary Wallet backend

pub mod descriptor;
pub mod time;

pub use descriptor::{
    compact_wallet_key_input, extract_address_from_descriptor, extract_pubkey_from_descriptor,
    parse_multipath_descriptor, strip_key_origin, DescriptorError,
};
pub use time::current_unix_timestamp;
