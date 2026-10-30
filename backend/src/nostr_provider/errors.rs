//! User-facing Nostr send error messages and the stable error codes the test-send API
//! returns for them.

pub(super) const NOSTR_DISCOVERY_FAILED_ERROR_CODE: &str = "nostr_discovery_failed";
pub(super) const NOSTR_INBOX_CONNECT_FAILED_ERROR_CODE: &str = "nostr_inbox_connect_failed";
pub(super) const NOSTR_INBOX_DISCOVERY_TIMEOUT_ERROR_CODE: &str = "nostr_inbox_discovery_timeout";
pub(super) const NOSTR_NO_DM_RELAYS_ERROR_CODE: &str = "nostr_no_dm_relays";
pub(super) const NOSTR_PUBLISH_TIMEOUT_ERROR_CODE: &str = "nostr_publish_timeout";
pub(super) const NOSTR_SEND_FAILED_ERROR_CODE: &str = "nostr_send_failed";
pub(super) const NOSTR_AUTH_FAILED_ERROR_CODE: &str = "nostr_auth_failed";
pub(super) const NOSTR_NIP04_FAILED_ERROR_CODE: &str = "nostr_nip04_failed";
pub(super) const NOSTR_ONION_SOCKS_REQUIRED_ERROR_CODE: &str = "nostr_onion_socks_required";
pub(super) const NOSTR_ONION_SOCKS_REQUIRED_ERROR: &str = "Recipient inbox relays are .onion; configure CANARY_NOSTR_SOCKS_PROXY (system Tor SOCKS, typically 127.0.0.1:9050)";

pub(super) fn format_nostr_provider_error(error_message: String) -> String {
    if error_message.starts_with("Nostr discovery relays failed:")
        || error_message.starts_with("Nostr inbox relay connection failed:")
        || error_message == "Nostr inbox relay discovery timed out"
        || error_message == "Recipient has no kind 10050 Nostr DM inbox relay list"
        || error_message == "Nostr publish timed out"
        || error_message == NOSTR_ONION_SOCKS_REQUIRED_ERROR
        || error_message.starts_with("Nostr legacy DM")
    {
        error_message
    } else {
        format!("Nostr send failed: {error_message}")
    }
}

pub fn nostr_test_error_code(error_message: Option<&str>) -> Option<&'static str> {
    match error_message {
        Some(message) if message.starts_with("Nostr discovery relays failed:") => {
            Some(NOSTR_DISCOVERY_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr inbox relay connection failed:") => {
            Some(NOSTR_INBOX_CONNECT_FAILED_ERROR_CODE)
        }
        Some("Nostr inbox relay discovery timed out") => {
            Some(NOSTR_INBOX_DISCOVERY_TIMEOUT_ERROR_CODE)
        }
        Some("Recipient has no kind 10050 Nostr DM inbox relay list") => {
            Some(NOSTR_NO_DM_RELAYS_ERROR_CODE)
        }
        Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR) => Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR_CODE),
        Some("Nostr publish timed out") => Some(NOSTR_PUBLISH_TIMEOUT_ERROR_CODE),
        Some(message)
            if message.contains("authentication failed")
                || message.contains("failed to authenticate") =>
        {
            Some(NOSTR_AUTH_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr send failed:") => {
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr publish failed:") => {
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr inbox relay discovery failed:") => {
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        }
        Some(message) if message.starts_with("Nostr legacy DM") => {
            Some(NOSTR_NIP04_FAILED_ERROR_CODE)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_test_send_errors_to_codes() {
        assert_eq!(
            nostr_test_error_code(Some("Nostr inbox relay discovery timed out")),
            Some(NOSTR_INBOX_DISCOVERY_TIMEOUT_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Nostr inbox relay connection failed: wss://example.com"
            )),
            Some(NOSTR_INBOX_CONNECT_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some("Nostr publish timed out")),
            Some(NOSTR_PUBLISH_TIMEOUT_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Recipient has no kind 10050 Nostr DM inbox relay list"
            )),
            Some(NOSTR_NO_DM_RELAYS_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some("Nostr send failed: relay disconnected")),
            Some(NOSTR_SEND_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some("Nostr discovery relays failed: wss://example.com")),
            Some(NOSTR_DISCOVERY_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Nostr legacy DM publish failed: no relay accepted the message"
            )),
            Some(NOSTR_NIP04_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Nostr publish failed: ws://haven.local/chat: authentication failed"
            )),
            Some(NOSTR_AUTH_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(
                "Nostr publish failed: ws://haven.local/chat: failed to authenticate"
            )),
            Some(NOSTR_AUTH_FAILED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR)),
            Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR_CODE)
        );
        assert_eq!(
            nostr_test_error_code(Some(&format_nostr_provider_error(
                NOSTR_ONION_SOCKS_REQUIRED_ERROR.to_string()
            ))),
            Some(NOSTR_ONION_SOCKS_REQUIRED_ERROR_CODE)
        );
        assert_eq!(nostr_test_error_code(Some("different error")), None);
    }
}
