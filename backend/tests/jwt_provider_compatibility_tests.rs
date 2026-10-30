use canary::auth::AuthService;

// Synthetic HS256 fixture generated independently with Python's hmac/sha256.
const TOKEN: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzeW50aGV0aWMtdXNlciIsImVtYWlsIjoidGVzdEBleGFtcGxlLmludmFsaWQiLCJpc19hZG1pbiI6ZmFsc2UsImlzX2RlbW8iOmZhbHNlLCJleHAiOjQxMDI0NDQ4MDAsImlhdCI6MTcwMDAwMDAwMCwianRpIjoic3ludGhldGljLWNvbXBhdGliaWxpdHkifQ.UpWUqfI8pwwH3EEzcOA7UR2bf_fpVrpoJ_-mAaZ9QC4";

#[test]
fn independent_hs256_tokens_remain_compatible() {
    let claims = AuthService::validate_token_with_secret(TOKEN, "synthetic-compatibility-secret")
        .expect("existing HS256 tokens must survive a provider change");
    assert_eq!(claims.sub, "synthetic-user");
    assert!(!claims.is_admin);
    assert!(AuthService::validate_token_with_secret(TOKEN, "wrong-secret").is_err());
}
