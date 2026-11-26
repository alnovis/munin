use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX)
}

/// Formats Unix milliseconds as `YYYY-MM-DD HH:MM UTC`.
pub fn format_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let in_day = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        in_day / 3600,
        in_day % 3600 / 60
    )
}

// Howard Hinnant's days-to-civil algorithm for the proleptic Gregorian calendar.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(1_700_000_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(format_utc(951_782_400_000), "2000-02-29 00:00 UTC");
        assert_eq!(format_utc(-60_000), "1969-12-31 23:59 UTC");
    }
}
