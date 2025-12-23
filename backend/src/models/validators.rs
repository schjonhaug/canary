//! Input validation utilities

use phonenumber::PhoneNumber;
use std::str::FromStr;

/// Validates and normalizes a phone number
pub fn validate_phone_number(phone: &str) -> Result<String, String> {
    // Check if phone number starts with country code
    if !phone.starts_with('+') {
        return Err(
            "Phone number must include country code (e.g., +1 for US, +44 for UK, +47 for Norway)"
                .to_string(),
        );
    }

    // Parse phone number using the phonenumber crate
    let parsed_number =
        PhoneNumber::from_str(phone).map_err(|_| "Invalid phone number format".to_string())?;

    // Check if it's a valid number
    if !parsed_number.is_valid() {
        return Err("Invalid phone number".to_string());
    }

    // Return normalized E.164 format
    Ok(parsed_number
        .format()
        .mode(phonenumber::Mode::E164)
        .to_string())
}
