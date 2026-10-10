//! ISO-8601 UTC timestamps without a date crate.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn now_iso() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_millis(d.as_millis() as i64)
}

pub fn format_millis(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, sod) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        sod / 3600,
        sod / 60 % 60,
        sod % 60,
        ms.rem_euclid(1000)
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats() {
        assert_eq!(super::format_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            super::format_millis(1_781_686_800_123),
            "2026-06-17T09:00:00.123Z"
        );
        assert_eq!(
            super::format_millis(951_782_400_000),
            "2000-02-29T00:00:00.000Z"
        );
    }
}
