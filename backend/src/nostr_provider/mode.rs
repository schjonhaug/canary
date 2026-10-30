//! Nostr DM format selection (NIP-17, legacy NIP-04 or automatic) and its instance setting.

use crate::metadata::MetadataDb;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub(super) const NOSTR_DM_MODE_SETTING_KEY: &str = "nostr_dm_mode";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NostrDmMode {
    #[default]
    Auto,
    Nip17,
    Nip04,
}

impl NostrDmMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Nip17 => "nip17",
            Self::Nip04 => "nip04",
        }
    }
}

impl FromStr for NostrDmMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "nip17" => Ok(Self::Nip17),
            "nip04" => Ok(Self::Nip04),
            _ => Err("Unsupported Nostr DM mode".to_string()),
        }
    }
}

pub async fn get_nostr_dm_mode(metadata_db: &MetadataDb) -> Result<NostrDmMode> {
    Ok(metadata_db
        .get_instance_setting(NOSTR_DM_MODE_SETTING_KEY)
        .await?
        .and_then(|value| NostrDmMode::from_str(&value).ok())
        .unwrap_or_default())
}

pub async fn set_nostr_dm_mode(metadata_db: &MetadataDb, dm_mode: NostrDmMode) -> Result<()> {
    metadata_db
        .set_instance_setting(NOSTR_DM_MODE_SETTING_KEY, dm_mode.as_str())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nostr_dm_modes() {
        assert_eq!("auto".parse::<NostrDmMode>().unwrap(), NostrDmMode::Auto);
        assert_eq!("nip17".parse::<NostrDmMode>().unwrap(), NostrDmMode::Nip17);
        assert_eq!("NIP04".parse::<NostrDmMode>().unwrap(), NostrDmMode::Nip04);
        assert!("kind4".parse::<NostrDmMode>().is_err());
    }

    #[test]
    fn formats_test_message_with_delivery_mode() {
        use crate::metadata::Language;
        use crate::test_notification::format_generic_nostr_test_message;

        assert!(
            format_generic_nostr_test_message(Language::English, NostrDmMode::Auto)
                .contains("DM format: Modern NIP-17.")
        );
        assert!(
            format_generic_nostr_test_message(Language::English, NostrDmMode::Nip17)
                .contains("DM format: Modern NIP-17.")
        );
        assert!(
            format_generic_nostr_test_message(Language::English, NostrDmMode::Nip04)
                .contains("DM format: Legacy NIP-04.")
        );
    }
}
