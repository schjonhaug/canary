//! ntfy server selection, trust and managed authentication.

use crate::ntfy_provider::NtfyAuth;

use super::{AppConfig, NtfyServerConfig, PUBLIC_NTFY_SERVER_ID};

impl AppConfig {
    /// Get configured self-hosted ntfy servers.
    pub fn ntfy_servers(&self) -> &[NtfyServerConfig] {
        &self.ntfy_servers
    }

    fn single_detected_ntfy_server(&self) -> Option<&NtfyServerConfig> {
        if !self.is_self_hosted_mode() {
            return None;
        }

        match self.ntfy_servers.as_slice() {
            [server] => Some(server),
            servers if servers.len() > 1 => {
                tracing::debug!(
                    count = servers.len(),
                    "Multiple detected ntfy servers configured; falling back to explicit default selection"
                );
                None
            }
            _ => None,
        }
    }

    /// Get the default ntfy server id for API config responses.
    pub fn default_ntfy_server_id(&self) -> String {
        if let Some(server) = self.single_detected_ntfy_server() {
            server.id.clone()
        } else {
            PUBLIC_NTFY_SERVER_ID.to_string()
        }
    }

    /// Check if ntfy provider should be enabled
    pub fn is_ntfy_enabled(&self) -> bool {
        // ntfy is always available except during isolated restore drills.
        // Self-hosted mode may also register local-only providers.
        !self.is_restore_drill()
    }

    /// Get the default ntfy server URL.
    /// Detected self-hosted integrations take precedence over environment fallback.
    pub fn ntfy_server_url(&self) -> String {
        if let Some(server) = self.single_detected_ntfy_server() {
            return server.base_url.clone();
        }

        self.ntfy_fallback_url.clone()
    }

    /// Check whether a URL is one of the currently detected self-hosted ntfy servers.
    pub fn is_detected_ntfy_server_url(&self, server_url: &str) -> bool {
        let normalized_server_url = server_url.trim().trim_end_matches('/');
        self.is_self_hosted_mode()
            && self
                .ntfy_servers
                .iter()
                .any(|server| server.base_url.trim_end_matches('/') == normalized_server_url)
    }

    /// Private ntfy destinations are supported on every self-hosted deployment.
    pub fn ntfy_outbound_policy(&self) -> crate::outbound_target::OutboundTargetPolicy {
        use crate::outbound_target::OutboundTargetPolicy;
        if self.is_self_hosted_mode() {
            OutboundTargetPolicy::SelfHostedNtfy
        } else {
            OutboundTargetPolicy::PublicOnly
        }
    }

    pub fn ntfy_provider(
        &self,
        server_url: &str,
        auth: NtfyAuth,
        user_configured_server_url: Option<&str>,
    ) -> crate::ntfy_provider::NtfyProvider {
        use crate::ntfy_provider::NtfyProvider;
        if self.should_trust_ntfy_server_url(server_url, user_configured_server_url) {
            NtfyProvider::with_trusted_auth(server_url, auth)
        } else {
            NtfyProvider::with_policy(server_url, auth, self.ntfy_outbound_policy())
        }
    }

    /// Whether an ntfy server may bypass public-network validation.
    ///
    /// Operator-configured defaults are trusted when the user has not selected
    /// another server. A saved user preference is trusted only when it exactly
    /// matches a detected self-hosted integration. This keeps migrated Umbrel
    /// and `StartOS` preferences working with their existing trusted-client behavior.
    pub fn should_trust_ntfy_server_url(
        &self,
        server_url: &str,
        user_configured_server_url: Option<&str>,
    ) -> bool {
        let Some(user_configured_server_url) = user_configured_server_url else {
            return true;
        };

        let normalized_server_url = server_url.trim().trim_end_matches('/');
        let normalized_user_url = user_configured_server_url.trim().trim_end_matches('/');
        normalized_server_url == normalized_user_url
            && self.is_detected_ntfy_server_url(normalized_server_url)
    }

    /// Return package-managed ntfy auth only for the exact detected server URL it belongs to.
    pub fn managed_ntfy_access_token_for_url(
        &self,
        server_url: &str,
        user_configured_server_url: Option<&str>,
    ) -> Option<String> {
        let normalized_server_url = server_url.trim().trim_end_matches('/');
        let matches_user_configured_url = user_configured_server_url
            .map(|url| url.trim().trim_end_matches('/'))
            .filter(|url| !url.is_empty())
            .is_some_and(|url| url == normalized_server_url);

        if matches_user_configured_url {
            return None;
        }

        let matches_managed_server = self.is_self_hosted_mode()
            && self.ntfy_servers.iter().any(|server| {
                server.managed_auth
                    && server.base_url.trim_end_matches('/') == normalized_server_url
            });

        if matches_managed_server {
            self.managed_ntfy_access_token
                .as_ref()
                .map(|token| token.0.clone())
        } else {
            None
        }
    }

    /// Apply package-managed ntfy auth when no explicit user auth applies.
    pub fn with_managed_ntfy_auth(
        &self,
        ntfy_auth: NtfyAuth,
        server_url: &str,
        user_configured_server_url: Option<&str>,
    ) -> NtfyAuth {
        if !matches!(ntfy_auth, NtfyAuth::None) {
            return ntfy_auth;
        }

        self.managed_ntfy_access_token_for_url(server_url, user_configured_server_url)
            .map_or(NtfyAuth::None, NtfyAuth::AccessToken)
    }

    /// Auth may be sent only to explicitly configured user URLs or detected local integrations.
    pub fn should_use_ntfy_auth_for_url(
        &self,
        server_url: &str,
        user_configured_server_url: Option<&str>,
    ) -> bool {
        let normalized_server_url = server_url.trim().trim_end_matches('/');
        let matches_user_configured_url = user_configured_server_url
            .map(|url| url.trim().trim_end_matches('/'))
            .filter(|url| !url.is_empty())
            .is_some_and(|url| url == normalized_server_url);

        matches_user_configured_url || self.is_detected_ntfy_server_url(server_url)
    }
}
