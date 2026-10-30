//! Reads `AppConfig` from command line arguments and the environment.
//!
//! This is the only place that reads configuration variables. Loading goes
//! through [`EnvSource`] so tests can supply variables without touching the
//! process environment.

use std::collections::HashSet;
use std::ffi::OsString;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Parser;

use super::settings::{
    BtcPayCloudSettings, EmailSettings, SecretString, StripeSettings, SyncIntervalOverrides,
    SystemLocale, TwilioSettings,
};
use super::{
    AppConfig, ManagedNtfyAccessToken, NetworkConfig, NtfyServerConfig, OperatingMode,
    STARTOS_NTFY_SERVER_ID, TxExplorerConfig, UMBREL_NTFY_SERVER_ID,
};

/// Where the administrator TOTP secrets are read from unless
/// `CANARY_ADMIN_MFA_SECRETS_FILE` says otherwise.
pub(super) const DEFAULT_ADMIN_MFA_SECRETS_FILE: &str = "/run/secrets/canary-admin-mfa.json";

const NOSTR_SOCKS_PROXY_ENV: &str = "CANARY_NOSTR_SOCKS_PROXY";

#[derive(Debug, Clone, Parser)]
#[command(name = "canary")]
#[command(about = "Bitcoin wallet management service")]
pub(crate) struct AppConfigArgs {
    /// Bitcoin network to use
    #[arg(long, value_enum)]
    pub network: Option<NetworkConfig>,

    /// Electrum server URL to connect to
    #[arg(long)]
    pub electrum_url: Option<String>,

    /// Server bind address
    #[arg(long)]
    pub bind_address: Option<String>,

    /// Data directory path
    #[arg(long)]
    pub data_dir: Option<String>,
}

/// A source of configuration variables.
pub(crate) trait EnvSource {
    fn var_os(&self, key: &str) -> Option<OsString>;
}

/// The process environment, used by [`AppConfig::load`].
struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn var_os(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }
}

#[cfg(test)]
impl EnvSource for std::collections::HashMap<&str, &str> {
    fn var_os(&self, key: &str) -> Option<OsString> {
        self.get(key).map(OsString::from)
    }
}

/// Typed readers over an [`EnvSource`].
pub(super) struct Env<'a>(pub(super) &'a dyn EnvSource);

impl Env<'_> {
    /// The value, if set and valid unicode (like `std::env::var(..).ok()`).
    fn var(&self, key: &str) -> Option<String> {
        self.var_os(key).and_then(|value| value.into_string().ok())
    }

    fn var_os(&self, key: &str) -> Option<OsString> {
        self.0.var_os(key)
    }

    fn secret(&self, key: &str) -> Option<SecretString> {
        self.var(key).map(SecretString::new)
    }

    fn parsed<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.var(key).and_then(|value| value.parse().ok())
    }

    /// The trimmed value, treating blank values as unset.
    fn non_empty(&self, key: &str) -> Option<String> {
        self.var(key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    pub(super) fn url(&self, key: &str) -> Option<String> {
        self.var(key)
            .and_then(|url| normalize_url_env_value(key, &url))
    }

    pub(super) fn url_list(&self, key: &str) -> Vec<String> {
        let mut urls = Vec::new();

        if let Some(raw_urls) = self.var(key) {
            for raw_url in raw_urls.split(',') {
                if let Some(url) = normalize_url_env_value(key, raw_url) {
                    if !urls.contains(&url) {
                        urls.push(url);
                    }
                }
            }
        }

        urls
    }

    fn ntfy_topic(&self, key: &str) -> Option<String> {
        let topic = self.non_empty(key)?;
        let is_valid_topic = topic.len() <= 64
            && topic
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');

        if is_valid_topic {
            Some(topic)
        } else {
            tracing::warn!("{} contains an invalid ntfy topic and will be ignored", key);
            None
        }
    }
}

fn normalize_url_env_value(var_name: &str, url: &str) -> Option<String> {
    let trimmed_url = url.trim();
    if trimmed_url.is_empty() {
        tracing::warn!("{} contains a blank URL and it will be ignored", var_name);
        None
    } else if trimmed_url.starts_with("http://") || trimmed_url.starts_with("https://") {
        Some(trimmed_url.trim_end_matches('/').to_string())
    } else {
        tracing::warn!(
            "{} contains a URL that does not start with http:// or https://: '{}' - ignoring",
            var_name,
            url
        );
        None
    }
}

/// Self-hosted ntfy servers detected from platform-provided variables.
struct NtfyLoad {
    servers: Vec<NtfyServerConfig>,
    managed_access_token: Option<ManagedNtfyAccessToken>,
    fallback_url: String,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        // Note: .env file is loaded at the start of main() before logging init

        // Parse command line arguments
        let args = AppConfigArgs::parse();
        Self::load_from_args(args)
    }

    pub(crate) fn load_from_args(args: AppConfigArgs) -> Result<Self> {
        Self::load_from(args, &ProcessEnv)
    }

    /// Builds the configuration from `args`, falling back to `source` for
    /// every value the command line does not set.
    pub(crate) fn load_from(args: AppConfigArgs, source: &dyn EnvSource) -> Result<Self> {
        let env = Env(source);

        // Resolve network configuration (CLI args override env vars)
        let Some(network) = args
            .network
            .or_else(|| env.var("CANARY_NETWORK").and_then(|v| v.parse().ok()))
        else {
            return Err(anyhow!(
                "CANARY_NETWORK is required. Set it via --network CLI arg or CANARY_NETWORK environment variable.\nValid values: regtest, testnet, mainnet"
            ));
        };

        // Resolve electrum URL (CLI args override env vars)
        let electrum_url = args.electrum_url.or_else(|| env.var("CANARY_ELECTRUM_URL"));

        // Resolve bind address
        let Some(bind_address) = args.bind_address.or_else(|| env.var("CANARY_BIND_ADDRESS"))
        else {
            return Err(anyhow!(
                "CANARY_BIND_ADDRESS is required. Set it via --bind-address CLI arg or CANARY_BIND_ADDRESS environment variable.\nExample: 127.0.0.1:3000"
            ));
        };

        // Resolve data directory
        let Some(data_dir) = args.data_dir.or_else(|| env.var("CANARY_DATA_DIR")) else {
            return Err(anyhow!(
                "CANARY_DATA_DIR is required. Set it via --data-dir CLI arg or CANARY_DATA_DIR environment variable.\nExample: ./database"
            ));
        };

        // Resolve operating mode (required, no default)
        let operating_mode = match env.var("CANARY_MODE") {
            Some(mode_str) => mode_str.parse::<OperatingMode>()?,
            None => {
                return Err(anyhow!(
                    "CANARY_MODE is required. Set it via CANARY_MODE environment variable.\nValid values: cloud, self-hosted\n\nTo get started:\n  - For self-hosted mode: cp .env.example.self-hosted .env\n  - For cloud mode: cp .env.example.cloud .env"
                ));
            }
        };

        // Load the canonical frontend URL plus any additional browser origins.
        let frontend_url = env.var("FRONTEND_URL");
        let frontend_urls = env.url_list("FRONTEND_URLS");

        // Load authentication configuration
        let jwt_secret = env.var("JWT_SECRET");
        let self_hosted_admin_password = env.var("CANARY_SELF_HOSTED_ADMIN_PASSWORD");

        let tx_explorers = load_tx_explorers(&env);
        let ntfy = load_ntfy(&env, operating_mode);
        let nostr_socks_proxy = load_nostr_socks_proxy(&env)?;

        if operating_mode == OperatingMode::SelfHosted {
            require_self_hosted_secret(jwt_secret.as_deref(), "JWT_SECRET")?;
            require_self_hosted_secret(
                self_hosted_admin_password.as_deref(),
                "CANARY_SELF_HOSTED_ADMIN_PASSWORD",
            )?;
        }

        Ok(AppConfig {
            electrum_url,
            bind_address,
            data_dir,
            operating_mode,
            frontend_url,
            frontend_urls,
            jwt_secret: jwt_secret.map(SecretString::new),
            self_hosted_admin_password: self_hosted_admin_password.map(SecretString::new),
            tx_explorers,
            ntfy_servers: ntfy.servers,
            ntfy_fallback_url: ntfy.fallback_url,
            managed_ntfy_access_token: ntfy.managed_access_token,
            // Load BTCPay configuration (optional, cloud mode only)
            btcpay_url: env.var("BTCPAY_URL"),
            btcpay_api_key: env.var("BTCPAY_API_KEY").map(SecretString::new),
            btcpay_store_id: env.var("BTCPAY_STORE_ID"),
            btcpay_offering_id: env.var("BTCPAY_OFFERING_ID"),
            btcpay_plan_id: env.var("BTCPAY_PLAN_ID"),
            btcpay_cloud: load_btcpay_cloud(&env),
            stripe: StripeSettings {
                secret_key: env.secret("STRIPE_SECRET_KEY"),
                webhook_secret: env.secret("STRIPE_WEBHOOK_SECRET"),
            },
            email: load_email(&env),
            twilio: load_twilio(&env),
            restore_drill: is_restore_drill_value(env.var("CANARY_RESTORE_DRILL").as_deref()),
            sync_interval: env.parsed("CANARY_SYNC_INTERVAL"),
            sync_interval_overrides: load_sync_interval_overrides(&env, &network),
            admin_notification_topic: env.secret("ADMIN_NOTIFICATION_TOPIC"),
            trusted_proxy_ips: load_trusted_proxy_ips(&env),
            telegram_bot_token: env.non_empty("TELEGRAM_BOT_TOKEN").map(SecretString::new),
            nostr_socks_proxy,
            admin_mfa_secrets_file: env.var_os("CANARY_ADMIN_MFA_SECRETS_FILE").map_or_else(
                || PathBuf::from(DEFAULT_ADMIN_MFA_SECRETS_FILE),
                PathBuf::from,
            ),
            system_locale: SystemLocale {
                lang: env.var("LANG"),
                lc_all: env.var("LC_ALL"),
            },
            network,
        })
    }
}

/// Whether a `CANARY_RESTORE_DRILL` value turns the restore drill on.
fn is_restore_drill_value(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("1" | "true" | "yes" | "on"))
}

fn require_self_hosted_secret(value: Option<&str>, name: &str) -> Result<()> {
    if value.map(str::trim).is_none_or(str::is_empty) {
        return Err(anyhow!(
            "{name} required for self-hosted mode - check your .env file"
        ));
    }
    Ok(())
}

/// Loads the optional self-hosted transaction explorers.
fn load_tx_explorers(env: &Env<'_>) -> Vec<TxExplorerConfig> {
    let tx_explorer_platform = env.var("CANARY_TX_EXPLORER_PLATFORM");
    if let Some(platform) = tx_explorer_platform
        .as_deref()
        .map(str::trim)
        .filter(|platform| !platform.is_empty())
    {
        if !matches!(platform, "mynode" | "umbrel" | "startos") {
            tracing::warn!(
                "CANARY_TX_EXPLORER_PLATFORM contains an unrecognized platform '{}' - local explorer settings will fall back to the generic local label",
                platform
            );
        }
    }

    [
        ("mempool", "Mempool", "CANARY_MEMPOOL"),
        ("bitfeed", "Bitfeed", "CANARY_BITFEED"),
        (
            "btc-rpc-explorer",
            "BTC RPC Explorer",
            "CANARY_BTC_RPC_EXPLORER",
        ),
    ]
    .into_iter()
    .filter_map(|(id, name, prefix)| {
        TxExplorerConfig::new(
            id,
            name,
            env.url(&format!("{prefix}_URL")),
            &env.url_list(&format!("{prefix}_URLS")),
            env.parsed(&format!("{prefix}_PORT")),
            tx_explorer_platform.as_deref(),
        )
    })
    .collect()
}

/// Loads the ntfy fallback URL and, in self-hosted mode, the ntfy servers
/// provided by `StartOS` and Umbrel.
fn load_ntfy(env: &Env<'_>, operating_mode: OperatingMode) -> NtfyLoad {
    let startos_ntfy_url = env.url("CANARY_NTFY_SERVER_URL");
    let startos_ntfy_token = env.non_empty("CANARY_NTFY_TOKEN");
    let startos_ntfy_topic = env.ntfy_topic("CANARY_NTFY_TOPIC");
    let umbrel_ntfy_url = env.url("CANARY_UMBREL_NTFY_URL");
    let configured_ntfy_fallback_url = env.url("NTFY_SERVER_URL");
    if operating_mode == OperatingMode::Cloud
        && (startos_ntfy_url.is_some()
            || startos_ntfy_token.is_some()
            || startos_ntfy_topic.is_some())
    {
        tracing::warn!(
            "CANARY_NTFY_* variables are only used in self-hosted mode; ignoring provisioned ntfy defaults in cloud mode"
        );
    }
    if operating_mode == OperatingMode::Cloud && umbrel_ntfy_url.is_some() {
        tracing::warn!(
            "CANARY_UMBREL_NTFY_URL is only used in self-hosted mode; ignoring detected ntfy server in cloud mode"
        );
    }
    if operating_mode == OperatingMode::SelfHosted
        && startos_ntfy_url.is_none()
        && (startos_ntfy_token.is_some() || startos_ntfy_topic.is_some())
    {
        tracing::warn!(
            "CANARY_NTFY_TOKEN or CANARY_NTFY_TOPIC is set without CANARY_NTFY_SERVER_URL; provisioned ntfy defaults will be ignored"
        );
    }
    if operating_mode == OperatingMode::SelfHosted
        && startos_ntfy_url.is_some()
        && startos_ntfy_token.is_none()
    {
        tracing::warn!(
            "CANARY_NTFY_SERVER_URL is set without CANARY_NTFY_TOKEN; provisioned ntfy server will be available without managed auth"
        );
    }
    if operating_mode == OperatingMode::SelfHosted
        && startos_ntfy_url.is_some()
        && umbrel_ntfy_url.is_some()
    {
        tracing::warn!(
            "Both CANARY_NTFY_SERVER_URL and CANARY_UMBREL_NTFY_URL are set; multiple detected ntfy servers will be listed and no local ntfy default will be selected automatically"
        );
    }
    if operating_mode == OperatingMode::SelfHosted && configured_ntfy_fallback_url.is_some() {
        if startos_ntfy_url.is_some() {
            tracing::warn!(
                "CANARY_NTFY_SERVER_URL is set; detected StartOS ntfy server will take precedence over NTFY_SERVER_URL"
            );
        }
        if umbrel_ntfy_url.is_some() {
            tracing::warn!(
                "CANARY_UMBREL_NTFY_URL is set; detected Umbrel ntfy server will take precedence over NTFY_SERVER_URL"
            );
        }
    }
    let has_managed_startos_auth = startos_ntfy_url.is_some() && startos_ntfy_token.is_some();
    let servers = if operating_mode == OperatingMode::SelfHosted {
        [
            NtfyServerConfig::new_with_defaults(
                STARTOS_NTFY_SERVER_ID,
                "ntfy",
                startos_ntfy_url,
                Some("startos"),
                startos_ntfy_topic,
                has_managed_startos_auth,
            ),
            NtfyServerConfig::new(
                UMBREL_NTFY_SERVER_ID,
                "ntfy",
                umbrel_ntfy_url,
                Some("umbrel"),
            ),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else {
        Vec::new()
    };
    let managed_access_token =
        if operating_mode == OperatingMode::SelfHosted && has_managed_startos_auth {
            startos_ntfy_token.map(ManagedNtfyAccessToken)
        } else {
            None
        };

    NtfyLoad {
        servers,
        managed_access_token,
        fallback_url: configured_ntfy_fallback_url.unwrap_or_else(|| "https://ntfy.sh".to_string()),
    }
}

pub(super) fn load_btcpay_cloud(env: &Env<'_>) -> BtcPayCloudSettings {
    let positive_price = |key: &str| {
        env.non_empty(key)
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
    };

    BtcPayCloudSettings {
        offering_id: env.non_empty("BTCPAY_CLOUD_OFFERING_ID"),
        personal_plan_id: env.non_empty("BTCPAY_CLOUD_PERSONAL_PLAN_ID"),
        team_plan_id: env.non_empty("BTCPAY_CLOUD_TEAM_PLAN_ID"),
        currency: env.non_empty("BTCPAY_CLOUD_CURRENCY"),
        personal_monthly_price: positive_price("BTCPAY_CLOUD_PERSONAL_PRICE"),
        team_monthly_price: positive_price("BTCPAY_CLOUD_TEAM_PRICE"),
        webhook_secret: env
            .non_empty("BTCPAY_WEBHOOK_SECRET")
            .map(SecretString::new),
    }
}

fn load_email(env: &Env<'_>) -> EmailSettings {
    EmailSettings {
        resend_api_key: env.secret("RESEND_API_KEY"),
        resend_from_email: env.var("RESEND_FROM_EMAIL"),
        resend_from_name: env.var("RESEND_FROM_NAME"),
        resend_audience_id: env.var("RESEND_AUDIENCE_ID"),
        contact_form_email: env.var("CONTACT_FORM_EMAIL"),
    }
}

fn load_twilio(env: &Env<'_>) -> TwilioSettings {
    TwilioSettings {
        account_sid: env.var("TWILIO_ACCOUNT_SID"),
        auth_token: env.secret("TWILIO_AUTH_TOKEN"),
        sender_id: env.var("TWILIO_SENDER_ID"),
        verify_service_sid: env.var("TWILIO_VERIFY_SERVICE_SID"),
    }
}

/// Reads the tier overrides for `network` only; overrides for other networks
/// never apply to this process.
fn load_sync_interval_overrides(env: &Env<'_>, network: &NetworkConfig) -> SyncIntervalOverrides {
    let suffix = match network {
        NetworkConfig::Regtest => "REGTEST",
        NetworkConfig::Testnet => "TESTNET",
        NetworkConfig::Mainnet => "MAINNET",
    };
    SyncIntervalOverrides {
        personal_secs: env.parsed(&format!("CANARY_SYNC_INTERVAL_PERSONAL_{suffix}")),
        team_secs: env.parsed(&format!("CANARY_SYNC_INTERVAL_TEAM_{suffix}")),
    }
}

/// Proxies whose `X-Forwarded-For` header is trusted for client addresses.
/// Entries that are not IP addresses are ignored.
fn load_trusted_proxy_ips(env: &Env<'_>) -> HashSet<IpAddr> {
    env.var("CANARY_TRUSTED_PROXY_IPS")
        .map(|value| {
            value
                .split(',')
                .filter_map(|entry| entry.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn load_nostr_socks_proxy(env: &Env<'_>) -> Result<Option<SocketAddr>> {
    let Some(value) = env.var_os(NOSTR_SOCKS_PROXY_ENV) else {
        return Ok(None);
    };
    let value = value
        .into_string()
        .map_err(|_| anyhow!("Invalid {NOSTR_SOCKS_PROXY_ENV}: value is not valid unicode"))?;
    crate::nostr_provider::parse_nostr_onion_socks_proxy(&value).map_err(|error| anyhow!(error))
}
