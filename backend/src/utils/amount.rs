//! Conversions between satoshi amounts, BTC and fiat cents.
//!
//! Bitcoin's supply cap (21 million BTC, 2.1 × 10^15 sats) fits in `i64` and is
//! below 2^53, so every valid amount converts losslessly between `u64`, `i64`
//! and `f64`. The casts are confined to this module so call sites stay explicit
//! about units.

/// Number of satoshis in one bitcoin.
pub const SATS_PER_BTC: i64 = 100_000_000;

/// Converts an unsigned satoshi amount to the signed representation stored in
/// the database.
///
/// Values above `i64::MAX` cannot be valid Bitcoin amounts. They saturate rather
/// than panic, because some inputs come straight from an Electrum server.
pub fn sats_to_i64(sats: u64) -> i64 {
    i64::try_from(sats).unwrap_or(i64::MAX)
}

/// Converts a satoshi amount to `f64`, exactly for every valid amount.
#[expect(
    clippy::cast_precision_loss,
    reason = "valid amounts are below 2^53 and convert exactly"
)]
pub fn sats_to_f64(sats: i64) -> f64 {
    sats as f64
}

/// Converts a satoshi amount to BTC.
pub fn sats_to_btc(sats: i64) -> f64 {
    sats_to_f64(sats) / sats_to_f64(SATS_PER_BTC)
}

/// Converts a BTC amount to satoshis, truncating sub-satoshi fractions toward
/// zero.
#[expect(
    clippy::cast_possible_truncation,
    reason = "float-to-int `as` saturates; sub-satoshi fractions are dropped on purpose"
)]
pub fn btc_to_sats(btc: f64) -> i64 {
    (btc * sats_to_f64(SATS_PER_BTC)) as i64
}

/// Converts a fiat amount to cents, truncating fractions of a cent toward zero.
#[expect(
    clippy::cast_possible_truncation,
    reason = "float-to-int `as` saturates; fractions of a cent are dropped on purpose"
)]
pub fn fiat_to_cents(amount: f64) -> i64 {
    (amount * 100.0) as i64
}

/// Converts an amount in cents (or another minor currency unit) to `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "minor-unit prices are far below 2^53 and convert exactly"
)]
pub fn cents_to_f64(cents: i64) -> f64 {
    cents as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_MONEY_SATS: i64 = 21_000_000 * SATS_PER_BTC;

    #[test]
    #[expect(clippy::float_cmp, reason = "the conversion is exact by design")]
    fn max_money_round_trips_through_f64() {
        assert_eq!(btc_to_sats(sats_to_btc(MAX_MONEY_SATS)), MAX_MONEY_SATS);
        assert_eq!(sats_to_f64(MAX_MONEY_SATS), 2_100_000_000_000_000.0);
    }

    #[test]
    fn one_btc_converts_both_ways() {
        assert!((sats_to_btc(SATS_PER_BTC) - 1.0).abs() < f64::EPSILON);
        assert_eq!(btc_to_sats(1.0), SATS_PER_BTC);
    }

    #[test]
    fn fractions_truncate_toward_zero() {
        assert_eq!(btc_to_sats(0.000_000_019), 1);
        assert_eq!(fiat_to_cents(12.349), 1234);
        assert_eq!(fiat_to_cents(-1.239), -123);
    }

    #[test]
    fn oversized_unsigned_amounts_saturate() {
        assert_eq!(sats_to_i64(50 * 100_000_000), 5_000_000_000);
        assert_eq!(sats_to_i64(u64::MAX), i64::MAX);
    }

    #[test]
    fn cents_convert_exactly() {
        assert!((cents_to_f64(999) - 999.0).abs() < f64::EPSILON);
    }
}
