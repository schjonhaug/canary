//! Closed sets of domain values that are stored in `SQLite` and returned by the API.
//!
//! Each enum mirrors a `CHECK (... IN (...))` constraint in `backend/migrations/`.
//! A value has one string form, used in the database, in JSON and in logs. The
//! `string_enum!` macro generates the serde names, `as_str`, `Display`,
//! `FromStr` and the rusqlite conversions from a single table so they cannot
//! drift apart.
//!
//! Reading a string outside the set from the database is an error, not a
//! default: the CHECK constraints mean it can only happen if the schema and
//! the code disagree, and guessing would hide that.

use bdk_wallet::rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use bdk_wallet::rusqlite::ToSql;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A string that is not a member of the expected set of values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind}: {value:?}")]
pub struct UnknownValueError {
    kind: &'static str,
    value: String,
}

/// Defines a fieldless enum whose variants map one-to-one to fixed strings.
///
/// The generated type derives serde with an explicit rename per variant, and
/// implements `as_str`, `Display`, `FromStr`, `ToSql` and `FromSql` from the
/// same strings.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident as $kind:literal {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $text:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name {
            $(
                $(#[$variant_meta])*
                #[serde(rename = $text)]
                $variant,
            )+
        }

        impl $name {
            /// Every variant, in declaration order.
            #[cfg(test)]
            const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The string stored in the database and sent in JSON.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = UnknownValueError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(UnknownValueError {
                        kind: $kind,
                        value: value.to_owned(),
                    }),
                }
            }
        }

        impl ToSql for $name {
            fn to_sql(&self) -> bdk_wallet::rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                value
                    .as_str()?
                    .parse()
                    .map_err(|error| FromSqlError::Other(Box::new(error)))
            }
        }
    };
}

string_enum! {
    /// Lifecycle of a tracked transaction (`transactions.transaction_status`).
    pub enum TransactionStatus as "transaction status" {
        /// Seen in the mempool but not yet in a block.
        Pending => "pending",
        /// Included in a block.
        Confirmed => "confirmed",
        /// Evicted from the mempool by an RBF replacement (see `replaced_by_txid`).
        Replaced => "replaced",
    }
}

impl TransactionStatus {
    /// Status of a transaction that has not been replaced, given whether it is in a block.
    pub const fn from_confirmed(is_confirmed: bool) -> Self {
        if is_confirmed {
            Self::Confirmed
        } else {
            Self::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bdk_wallet::rusqlite::Connection;

    /// Every variant survives JSON, `Display`/`FromStr` and `SQLite` unchanged,
    /// and all three use the same string.
    fn assert_round_trips<T>(all: &[T], expected: &[&str])
    where
        T: Copy
            + fmt::Debug
            + PartialEq
            + fmt::Display
            + FromStr<Err = UnknownValueError>
            + Serialize
            + serde::de::DeserializeOwned
            + ToSql
            + FromSql,
    {
        let actual: Vec<String> = all.iter().map(ToString::to_string).collect();
        assert_eq!(actual, expected, "variant strings changed");

        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (value TEXT NOT NULL)", [])
            .unwrap();

        for (&value, &text) in all.iter().zip(expected) {
            let json = serde_json::to_string(&value).unwrap();
            assert_eq!(json, format!("\"{text}\""));
            assert_eq!(serde_json::from_str::<T>(&json).unwrap(), value);

            assert_eq!(text.parse::<T>().unwrap(), value);

            conn.execute("DELETE FROM t", []).unwrap();
            conn.execute("INSERT INTO t (value) VALUES (?1)", [&value])
                .unwrap();
            let stored: String = conn
                .query_row("SELECT value FROM t", [], |row| row.get(0))
                .unwrap();
            assert_eq!(stored, text);
            let loaded: T = conn
                .query_row("SELECT value FROM t", [], |row| row.get(0))
                .unwrap();
            assert_eq!(loaded, value);
        }

        assert!("not-a-member".parse::<T>().is_err());
        assert!(serde_json::from_str::<T>("\"not-a-member\"").is_err());
        let unknown = conn.query_row("SELECT 'not-a-member'", [], |row| row.get::<_, T>(0));
        assert!(unknown.is_err(), "unknown stored values must not be mapped");
    }

    #[test]
    fn transaction_status_round_trips() {
        assert_round_trips(
            TransactionStatus::ALL,
            &["pending", "confirmed", "replaced"],
        );
    }

    #[test]
    fn transaction_status_from_confirmed() {
        assert_eq!(
            TransactionStatus::from_confirmed(true),
            TransactionStatus::Confirmed
        );
        assert_eq!(
            TransactionStatus::from_confirmed(false),
            TransactionStatus::Pending
        );
    }

    #[test]
    fn unknown_value_error_names_the_set() {
        let error = "mined".parse::<TransactionStatus>().unwrap_err();
        assert_eq!(error.to_string(), "unknown transaction status: \"mined\"");
    }
}
