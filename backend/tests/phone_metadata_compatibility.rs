use canary::models::validate_phone_number;

#[test]
fn embedded_phone_metadata_still_validates_and_normalizes() {
    // Reserved fictional numbers exercise metadata deserialization and lookup.
    assert_eq!(
        validate_phone_number("+1 202 555 0123").unwrap(),
        "+12025550123"
    );
    assert_eq!(
        validate_phone_number("+1 (202) 555-0123").unwrap(),
        "+12025550123"
    );
    for invalid in ["2025550123", "+1", "+000123", "+not-a-number"] {
        assert!(
            validate_phone_number(invalid).is_err(),
            "accepted {invalid}"
        );
    }
}
