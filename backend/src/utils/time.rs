use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

pub fn unix_timestamp(time: SystemTime) -> Result<u64, SystemTimeError> {
    Ok(time.duration_since(UNIX_EPOCH)?.as_secs())
}

pub fn current_unix_timestamp() -> Result<u64, SystemTimeError> {
    unix_timestamp(SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::{current_unix_timestamp, unix_timestamp};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn unix_timestamp_returns_seconds_since_epoch() {
        let time = UNIX_EPOCH + Duration::from_secs(42);
        assert_eq!(unix_timestamp(time).unwrap(), 42);
    }

    #[test]
    fn unix_timestamp_rejects_pre_epoch_times() {
        let time = UNIX_EPOCH - Duration::from_secs(1);
        assert!(unix_timestamp(time).is_err());
    }

    #[test]
    fn current_unix_timestamp_returns_current_time() {
        assert!(current_unix_timestamp().is_ok());
    }
}
