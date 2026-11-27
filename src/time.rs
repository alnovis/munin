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

/// Parses a UTC timestamp such as `2026-11-27T07:47:46.206Z` (as written in
/// Claude Code transcripts) into Unix milliseconds.
pub fn parse_utc_ms(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (d.next()??, d.next()??, d.next()??);
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut t = hms.splitn(3, ':').map(|p| p.parse::<i64>().ok());
    let (h, m, sec) = (t.next()??, t.next()??, t.next()??);
    let digits: String = frac.chars().take(3).collect();
    let ms = format!("{digits:0<3}").parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || h > 23 || m > 59 || sec > 60 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    Some(((days * 86_400 + h * 3600 + m * 60 + sec) * 1000) + ms)
}

// Howard Hinnant's days-from-civil, the inverse of civil_from_days.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
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

    #[test]
    fn parses_transcript_timestamps() {
        assert_eq!(parse_utc_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_utc_ms("2023-11-14T22:13:20Z"),
            Some(1_700_000_000_000)
        );
        assert_eq!(
            parse_utc_ms("2000-02-29T00:00:00.5Z"),
            Some(951_782_400_500)
        );
        let ms = parse_utc_ms("2026-11-27T07:47:46.206Z").unwrap();
        assert_eq!(format_utc(ms), "2026-11-27 07:47 UTC");
        assert_eq!(ms % 1000, 206);
        for bad in [
            "",
            "2026-11-27",
            "2026-11-27T07:47:46",
            "2026-13-01T00:00:00Z",
            "x-y-zTa:b:cZ",
        ] {
            assert_eq!(parse_utc_ms(bad), None, "{bad}");
        }
    }
}
