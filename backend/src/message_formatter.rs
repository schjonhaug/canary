use crate::metadata::{EventType, Language, TransactionNotification};
use num_format::{Locale, ToFormattedString};

pub struct MessageFormatter;

impl MessageFormatter {
    /// Format Bitcoin amount based on language preference
    pub fn format_btc_amount(amount_sats: i64, language: &Language) -> String {
        let btc_amount = amount_sats as f64 / 100_000_000.0;

        // Always format with 8 decimal places
        let formatted_with_decimals = format!("{:.8}", btc_amount);

        // Split into integer and decimal parts
        let parts: Vec<&str> = formatted_with_decimals.split('.').collect();
        let integer_part = parts[0];
        let decimal_part = parts.get(1).unwrap_or(&"00000000");

        // Parse integer part for locale formatting
        let integer_value: i64 = integer_part.parse().unwrap_or(0);

        // Format integer part with locale-specific thousands separators
        let formatted_integer = match language {
            Language::Norwegian | Language::German | Language::French | Language::Spanish | Language::Portuguese => {
                // These languages use space as thousands separator
                // Replace non-breaking space with regular space
                integer_value
                    .to_formatted_string(&Locale::nb)
                    .replace('\u{a0}', " ")
            }
            Language::English | Language::Japanese => {
                // English and Japanese use comma as thousands separator
                integer_value.to_formatted_string(&Locale::en)
            }
        };

        // Combine with decimal separator based on language
        match language {
            Language::Norwegian | Language::German | Language::French | Language::Spanish | Language::Portuguese => {
                format!("{},{}", formatted_integer, decimal_part)
            }
            Language::English | Language::Japanese => {
                format!("{}.{}", formatted_integer, decimal_part)
            }
        }
    }

    /// Format fiat amount based on language preference
    pub fn format_fiat_amount(amount: f64, currency: &str, language: &Language) -> String {
        // Format with 2 decimal places for fiat
        let integer_part = amount.floor() as i64;
        let decimal_part = ((amount - amount.floor()) * 100.0).round() as i64;

        // Format integer part with locale-specific thousands separators
        let formatted_integer = match language {
            Language::Norwegian | Language::German | Language::French | Language::Spanish | Language::Portuguese => {
                // These languages use space as thousands separator
                integer_part
                    .to_formatted_string(&Locale::nb)
                    .replace('\u{a0}', " ")
            }
            Language::English | Language::Japanese => {
                // English and Japanese use comma as thousands separator
                integer_part.to_formatted_string(&Locale::en)
            }
        };

        // Combine with decimal separator based on language
        let formatted_amount = match language {
            Language::Norwegian | Language::German | Language::French | Language::Spanish | Language::Portuguese => {
                format!("{},{:02}", formatted_integer, decimal_part)
            }
            Language::English | Language::Japanese => {
                format!("{}.{:02}", formatted_integer, decimal_part)
            }
        };

        // Add currency symbol/code
        format!("{} {}", formatted_amount, currency)
    }

    /// Generate localized message for transaction notification
    pub fn create_localized_message(
        notification: &TransactionNotification,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        // Handle different notification types
        match notification {
            TransactionNotification::Pending(tx) => {
                Self::create_transaction_message(tx, wallet_name, language, false)
            }
            TransactionNotification::Confirmed(tx) => {
                Self::create_transaction_message(tx, wallet_name, language, true)
            }
            TransactionNotification::BalanceAlert(alert) => {
                Self::create_balance_alert_message(alert, wallet_name, language)
            }
        }
    }

    /// Generate localized email subject for transaction notification
    pub fn create_localized_email_subject(
        notification: &TransactionNotification,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        match notification {
            TransactionNotification::Pending(tx) => {
                let (subject_text, emoji) = match tx.transaction_type {
                    EventType::Receive => match language {
                        Language::English => ("Receiving Bitcoin", "💸"),
                        Language::Norwegian => ("Mottar Bitcoin", "💸"),
                        Language::Spanish => ("Recibiendo Bitcoin", "💸"),
                        Language::Portuguese => ("Recebendo Bitcoin", "💸"),
                        Language::German => ("Bitcoin empfangen", "💸"),
                        Language::French => ("Reception de Bitcoin", "💸"),
                        Language::Japanese => ("ビットコイン受取中", "💸"),
                    },
                    EventType::Send => match language {
                        Language::English => ("Sending Bitcoin", "📤"),
                        Language::Norwegian => ("Sender Bitcoin", "📤"),
                        Language::Spanish => ("Enviando Bitcoin", "📤"),
                        Language::Portuguese => ("Enviando Bitcoin", "📤"),
                        Language::German => ("Bitcoin senden", "📤"),
                        Language::French => ("Envoi de Bitcoin", "📤"),
                        Language::Japanese => ("ビットコイン送信中", "📤"),
                    },
                };
                format!("{} {} - {}", emoji, subject_text, wallet_name)
            }
            TransactionNotification::Confirmed(tx) => {
                let (subject_text, emoji) = match tx.transaction_type {
                    EventType::Receive => match language {
                        Language::English => ("Bitcoin Received", "✅"),
                        Language::Norwegian => ("Bitcoin mottatt", "✅"),
                        Language::Spanish => ("Bitcoin Recibido", "✅"),
                        Language::Portuguese => ("Bitcoin Recebido", "✅"),
                        Language::German => ("Bitcoin erhalten", "✅"),
                        Language::French => ("Bitcoin Recu", "✅"),
                        Language::Japanese => ("ビットコイン受取完了", "✅"),
                    },
                    EventType::Send => match language {
                        Language::English => ("Bitcoin Sent", "✅"),
                        Language::Norwegian => ("Bitcoin sendt", "✅"),
                        Language::Spanish => ("Bitcoin Enviado", "✅"),
                        Language::Portuguese => ("Bitcoin Enviado", "✅"),
                        Language::German => ("Bitcoin gesendet", "✅"),
                        Language::French => ("Bitcoin Envoye", "✅"),
                        Language::Japanese => ("ビットコイン送信完了", "✅"),
                    },
                };
                format!("{} {} - {}", emoji, subject_text, wallet_name)
            }
            TransactionNotification::BalanceAlert(_) => {
                let subject_text = match language {
                    Language::English => "Balance Alert",
                    Language::Norwegian => "Saldovarsel",
                    Language::Spanish => "Alerta de Saldo",
                    Language::Portuguese => "Alerta de Saldo",
                    Language::German => "Kontostandwarnung",
                    Language::French => "Alerte de Solde",
                    Language::Japanese => "残高アラート",
                };
                format!("📊 {} - {}", subject_text, wallet_name)
            }
        }
    }

    /// Generate localized message for balance alert notification
    fn create_balance_alert_message(
        alert: &crate::metadata::BalanceAlertNotification,
        wallet_name: &str,
        language: &Language,
    ) -> String {
        // Check if this is a fiat threshold alert
        let threshold_display = if let (Some(ref currency), Some(fiat_amount), Some(rate)) = (
            &alert.threshold_currency,
            alert.threshold_fiat_amount,
            alert.exchange_rate_snapshot,
        ) {
            // Fiat threshold: show fiat amount with BTC equivalent
            let fiat_str = Self::format_fiat_amount(fiat_amount, currency, language);
            let btc_str = Self::format_btc_amount(alert.threshold_sats, language);
            format!(
                "{} (≈ {} BTC at {:.0} {}/BTC)",
                fiat_str, btc_str, rate, currency
            )
        } else {
            // BTC threshold: show only BTC
            format!(
                "{} BTC",
                Self::format_btc_amount(alert.threshold_sats, language)
            )
        };

        let current_display = if let (Some(ref currency), Some(rate)) =
            (&alert.threshold_currency, alert.exchange_rate_snapshot)
        {
            // Calculate current balance in fiat
            let current_btc = alert.current_balance_sats as f64 / 100_000_000.0;
            let current_fiat = current_btc * rate;
            let fiat_str = Self::format_fiat_amount(current_fiat, currency, language);
            let btc_str = Self::format_btc_amount(alert.current_balance_sats, language);
            format!("{} (≈ {} BTC)", fiat_str, btc_str)
        } else {
            format!(
                "{} BTC",
                Self::format_btc_amount(alert.current_balance_sats, language)
            )
        };

        match alert.alert_type {
            crate::metadata::BalanceAlertType::Equals => {
                if alert.threshold_sats == 0 {
                    // Special wallet drain alert
                    match language {
                        Language::English => format!("🚨 Wallet Drain Alert: {} balance is now 0 BTC", wallet_name),
                        Language::Norwegian => format!("🚨 Lommebok tømt: {} saldo er nå 0 BTC", wallet_name),
                        Language::Spanish => format!("🚨 Alerta de Vaciado: El saldo de {} es ahora 0 BTC", wallet_name),
                        Language::Portuguese => format!("🚨 Alerta de Esvaziamento: O saldo de {} agora e 0 BTC", wallet_name),
                        Language::German => format!("🚨 Wallet leer: {} Guthaben ist jetzt 0 BTC", wallet_name),
                        Language::French => format!("🚨 Portefeuille vide: Le solde de {} est maintenant 0 BTC", wallet_name),
                        Language::Japanese => format!("🚨 ウォレット残高ゼロ: {} の残高は 0 BTC になりました", wallet_name),
                    }
                } else {
                    match language {
                        Language::English => format!("📊 Balance Alert: {} balance is now {}", wallet_name, current_display),
                        Language::Norwegian => format!("📊 Saldovarsel: {} saldo er nå {}", wallet_name, current_display),
                        Language::Spanish => format!("📊 Alerta de Saldo: El saldo de {} es ahora {}", wallet_name, current_display),
                        Language::Portuguese => format!("📊 Alerta de Saldo: O saldo de {} agora e {}", wallet_name, current_display),
                        Language::German => format!("📊 Kontostandwarnung: {} Guthaben ist jetzt {}", wallet_name, current_display),
                        Language::French => format!("📊 Alerte de Solde: Le solde de {} est maintenant {}", wallet_name, current_display),
                        Language::Japanese => format!("📊 残高アラート: {} の残高は {} になりました", wallet_name, current_display),
                    }
                }
            }
            crate::metadata::BalanceAlertType::Above => match language {
                Language::English => format!("📊 Balance Alert: {} balance is now above {} (current: {})", wallet_name, threshold_display, current_display),
                Language::Norwegian => format!("📊 Saldovarsel: {} saldo er nå over {} (nåværende: {})", wallet_name, threshold_display, current_display),
                Language::Spanish => format!("📊 Alerta de Saldo: El saldo de {} esta ahora por encima de {} (actual: {})", wallet_name, threshold_display, current_display),
                Language::Portuguese => format!("📊 Alerta de Saldo: O saldo de {} agora esta acima de {} (atual: {})", wallet_name, threshold_display, current_display),
                Language::German => format!("📊 Kontostandwarnung: {} Guthaben ist jetzt uber {} (aktuell: {})", wallet_name, threshold_display, current_display),
                Language::French => format!("📊 Alerte de Solde: Le solde de {} est maintenant au-dessus de {} (actuel: {})", wallet_name, threshold_display, current_display),
                Language::Japanese => format!("📊 残高アラート: {} の残高が {} を超えました (現在: {})", wallet_name, threshold_display, current_display),
            },
            crate::metadata::BalanceAlertType::Below => match language {
                Language::English => format!("📊 Balance Alert: {} balance is now below {} (current: {})", wallet_name, threshold_display, current_display),
                Language::Norwegian => format!("📊 Saldovarsel: {} saldo er nå under {} (nåværende: {})", wallet_name, threshold_display, current_display),
                Language::Spanish => format!("📊 Alerta de Saldo: El saldo de {} esta ahora por debajo de {} (actual: {})", wallet_name, threshold_display, current_display),
                Language::Portuguese => format!("📊 Alerta de Saldo: O saldo de {} agora esta abaixo de {} (atual: {})", wallet_name, threshold_display, current_display),
                Language::German => format!("📊 Kontostandwarnung: {} Guthaben ist jetzt unter {} (aktuell: {})", wallet_name, threshold_display, current_display),
                Language::French => format!("📊 Alerte de Solde: Le solde de {} est maintenant en dessous de {} (actuel: {})", wallet_name, threshold_display, current_display),
                Language::Japanese => format!("📊 残高アラート: {} の残高が {} を下回りました (現在: {})", wallet_name, threshold_display, current_display),
            },
        }
    }

    /// Generate localized message for transaction notification
    fn create_transaction_message(
        transaction: &crate::metadata::Transaction,
        wallet_name: &str,
        language: &Language,
        is_confirmed: bool,
    ) -> String {
        let amount_btc = Self::format_btc_amount(transaction.amount_sats, language);

        // No balance display in notifications for privacy reasons
        match transaction.transaction_type {
            EventType::Send => {
                if is_confirmed {
                    if transaction.amount_sats > 0 {
                        match language {
                            Language::English => format!("✅ Sent: {} BTC from {}", amount_btc, wallet_name),
                            Language::Norwegian => format!("✅ Sendt: {} BTC fra {}", amount_btc, wallet_name),
                            Language::Spanish => format!("✅ Enviado: {} BTC desde {}", amount_btc, wallet_name),
                            Language::Portuguese => format!("✅ Enviado: {} BTC de {}", amount_btc, wallet_name),
                            Language::German => format!("✅ Gesendet: {} BTC von {}", amount_btc, wallet_name),
                            Language::French => format!("✅ Envoye: {} BTC de {}", amount_btc, wallet_name),
                            Language::Japanese => format!("✅ 送信完了: {} BTC を {} から", amount_btc, wallet_name),
                        }
                    } else {
                        match language {
                            Language::English => format!("✅ Sent from {}", wallet_name),
                            Language::Norwegian => format!("✅ Sendt fra {}", wallet_name),
                            Language::Spanish => format!("✅ Enviado desde {}", wallet_name),
                            Language::Portuguese => format!("✅ Enviado de {}", wallet_name),
                            Language::German => format!("✅ Gesendet von {}", wallet_name),
                            Language::French => format!("✅ Envoye de {}", wallet_name),
                            Language::Japanese => format!("✅ {} から送信完了", wallet_name),
                        }
                    }
                } else if transaction.transaction_status == "replaced" {
                    // RBF replacement notification
                    let replaced_by = transaction.replaced_by_txid.as_deref().unwrap_or("unknown");
                    let short_txid = &replaced_by[..8.min(replaced_by.len())];
                    match language {
                        Language::English => format!("🔄 Replaced: {} BTC from {} (replaced by {})", amount_btc, wallet_name, short_txid),
                        Language::Norwegian => format!("🔄 Erstattet: {} BTC fra {} (erstattet av {})", amount_btc, wallet_name, short_txid),
                        Language::Spanish => format!("🔄 Reemplazado: {} BTC desde {} (reemplazado por {})", amount_btc, wallet_name, short_txid),
                        Language::Portuguese => format!("🔄 Substituido: {} BTC de {} (substituido por {})", amount_btc, wallet_name, short_txid),
                        Language::German => format!("🔄 Ersetzt: {} BTC von {} (ersetzt durch {})", amount_btc, wallet_name, short_txid),
                        Language::French => format!("🔄 Remplace: {} BTC de {} (remplace par {})", amount_btc, wallet_name, short_txid),
                        Language::Japanese => format!("🔄 置換: {} BTC を {} から ({} に置換)", amount_btc, wallet_name, short_txid),
                    }
                } else {
                    match language {
                        Language::English => format!("📤 Sending: {} BTC from {}", amount_btc, wallet_name),
                        Language::Norwegian => format!("📤 Sender: {} BTC fra {}", amount_btc, wallet_name),
                        Language::Spanish => format!("📤 Enviando: {} BTC desde {}", amount_btc, wallet_name),
                        Language::Portuguese => format!("📤 Enviando: {} BTC de {}", amount_btc, wallet_name),
                        Language::German => format!("📤 Sende: {} BTC von {}", amount_btc, wallet_name),
                        Language::French => format!("📤 Envoi: {} BTC de {}", amount_btc, wallet_name),
                        Language::Japanese => format!("📤 送信中: {} BTC を {} から", amount_btc, wallet_name),
                    }
                }
            }
            EventType::Receive => {
                if transaction.transaction_status == "replaced" {
                    // RBF replacement notification for receive transaction
                    let replaced_by = transaction.replaced_by_txid.as_deref().unwrap_or("unknown");
                    let short_txid = &replaced_by[..8.min(replaced_by.len())];
                    match language {
                        Language::English => format!("🔄 Replaced: {} BTC to {} (replaced by {})", amount_btc, wallet_name, short_txid),
                        Language::Norwegian => format!("🔄 Erstattet: {} BTC til {} (erstattet av {})", amount_btc, wallet_name, short_txid),
                        Language::Spanish => format!("🔄 Reemplazado: {} BTC a {} (reemplazado por {})", amount_btc, wallet_name, short_txid),
                        Language::Portuguese => format!("🔄 Substituido: {} BTC para {} (substituido por {})", amount_btc, wallet_name, short_txid),
                        Language::German => format!("🔄 Ersetzt: {} BTC an {} (ersetzt durch {})", amount_btc, wallet_name, short_txid),
                        Language::French => format!("🔄 Remplace: {} BTC vers {} (remplace par {})", amount_btc, wallet_name, short_txid),
                        Language::Japanese => format!("🔄 置換: {} BTC を {} へ ({} に置換)", amount_btc, wallet_name, short_txid),
                    }
                } else if is_confirmed {
                    match language {
                        Language::English => format!("✅ Received: {} BTC to {}", amount_btc, wallet_name),
                        Language::Norwegian => format!("✅ Mottatt: {} BTC til {}", amount_btc, wallet_name),
                        Language::Spanish => format!("✅ Recibido: {} BTC en {}", amount_btc, wallet_name),
                        Language::Portuguese => format!("✅ Recebido: {} BTC em {}", amount_btc, wallet_name),
                        Language::German => format!("✅ Erhalten: {} BTC an {}", amount_btc, wallet_name),
                        Language::French => format!("✅ Recu: {} BTC sur {}", amount_btc, wallet_name),
                        Language::Japanese => format!("✅ 受取完了: {} BTC を {} へ", amount_btc, wallet_name),
                    }
                } else {
                    match language {
                        Language::English => format!("💸 Receiving: {} BTC to {} (unconfirmed)", amount_btc, wallet_name),
                        Language::Norwegian => format!("💸 Mottar: {} BTC til {} (ubekreftet)", amount_btc, wallet_name),
                        Language::Spanish => format!("💸 Recibiendo: {} BTC en {} (sin confirmar)", amount_btc, wallet_name),
                        Language::Portuguese => format!("💸 Recebendo: {} BTC em {} (nao confirmado)", amount_btc, wallet_name),
                        Language::German => format!("💸 Empfange: {} BTC an {} (unbestatigt)", amount_btc, wallet_name),
                        Language::French => format!("💸 Reception: {} BTC sur {} (non confirme)", amount_btc, wallet_name),
                        Language::Japanese => format!("💸 受取中: {} BTC を {} へ (未確認)", amount_btc, wallet_name),
                    }
                }
            }
        }
    }
}
