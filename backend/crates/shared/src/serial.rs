//! Business serial numbers: `prefix + yyyyMMddHHmmss + 6 random digits`.

use chrono::{DateTime, FixedOffset, Offset, Utc};
use rand::Rng;

/// Number of random digits appended to every serial.
const RANDOM_DIGITS: usize = 6;

/// Offset used for the timestamp part (UTC+8), matching the original deployment's local time.
const SERIAL_UTC_OFFSET_SECS: i32 = 8 * 3600;

/// Generates a serial such as `DJ20260924173502588608`.
pub fn generate(prefix: &str, now: DateTime<Utc>) -> String {
    let offset = FixedOffset::east_opt(SERIAL_UTC_OFFSET_SECS).unwrap_or_else(|| Utc.fix());
    let mut rng = rand::rng();
    let digits: String = (0..RANDOM_DIGITS)
        .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
        .collect();
    format!(
        "{}{}{}",
        prefix.trim(),
        now.with_timezone(&offset).format("%Y%m%d%H%M%S"),
        digits
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn has_prefix_timestamp_and_digits() {
        let now = Utc.with_ymd_and_hms(2026, 9, 24, 9, 35, 2).unwrap();
        let serial = generate("DJ", now);
        assert!(serial.starts_with("DJ20260924173502"));
        assert_eq!(serial.len(), 2 + 14 + 6);
        assert!(serial[16..].chars().all(|c| c.is_ascii_digit()));
    }
}
