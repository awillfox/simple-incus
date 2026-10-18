//! Display formatting helpers (compile for ssr and hydrate).

/// Bytes as a short binary-unit string: `0 B`, `512 B`, `1.5 KiB`, `1.2 GiB`.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit - 1])
}

/// A byte rate such as `0 B/s` or `1.5 KiB/s`.
pub fn format_rate(bytes_per_s: f64) -> String {
    format!("{}/s", format_bytes(if bytes_per_s.is_finite() { bytes_per_s.max(0.0) as u64 } else { 0 }))
}

/// Seconds since the Unix epoch for an RFC 3339 timestamp such as
/// `2026-10-18T10:00:00.123456789Z` or `2026-10-18T17:00:00+07:00`.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let part = s.get(from..from + len)?;
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok()).flatten()
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, m, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hh, mm, ss) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Skip fractional seconds, then read the zone designator.
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    let offset = match b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if i + 6 == b.len() && b[i + 3] == b':' => {
            let secs = num(i + 1, 2)? * 3600 + num(i + 4, 2)? * 60;
            if *sign == b'+' { secs } else { -secs }
        }
        _ => return None,
    };
    // Days since 1970-01-01 (proleptic Gregorian, Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss - offset)
}

/// Compact age, e.g. `just now`, `5m ago`, `3h ago`, `12d ago`, `4mo ago`, `2y ago`.
pub fn relative_time(then: i64, now: i64) -> String {
    let secs = now - then;
    if secs < 60 {
        return "just now".into();
    }
    let (n, unit) = match secs {
        s if s < 3600 => (s / 60, "m"),
        s if s < 86_400 => (s / 3600, "h"),
        s if s < 30 * 86_400 => (s / 86_400, "d"),
        s if s < 365 * 86_400 => (s / (30 * 86_400), "mo"),
        s => (s / (365 * 86_400), "y"),
    };
    format!("{n}{unit} ago")
}

/// Current time in seconds since the Unix epoch.
pub fn now_secs() -> i64 {
    #[cfg(feature = "hydrate")]
    {
        (js_sys::Date::now() / 1000.0) as i64
    }
    #[cfg(not(feature = "hydrate"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

/// Current time in milliseconds since the Unix epoch.
pub fn now_ms() -> f64 {
    #[cfg(feature = "hydrate")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(feature = "hydrate"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

/// Relative age of an RFC 3339 timestamp, or `—` when it cannot be parsed.
pub fn created_label(created_at: &str) -> String {
    parse_rfc3339(created_at).map(|t| relative_time(t, now_secs())).unwrap_or_else(|| "—".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(1_288_490_189), "1.2 GiB");
        assert_eq!(format_bytes(5 * 1024u64.pow(4)), "5.0 TiB");
    }

    #[test]
    fn rates() {
        assert_eq!(format_rate(0.0), "0 B/s");
        assert_eq!(format_rate(1536.0), "1.5 KiB/s");
        assert_eq!(format_rate(-5.0), "0 B/s");
        assert_eq!(format_rate(f64::NAN), "0 B/s");
    }

    #[test]
    fn rfc3339() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-10-18T10:00:00.123456789Z"), Some(1_792_317_600));
        assert_eq!(parse_rfc3339("2026-10-18T17:00:00+07:00"), Some(1_792_317_600));
        assert_eq!(parse_rfc3339("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(parse_rfc3339("garbage"), None);
        assert_eq!(parse_rfc3339(""), None);
    }

    #[test]
    fn relative() {
        let now = 1_000_000_000;
        assert_eq!(relative_time(now - 5, now), "just now");
        assert_eq!(relative_time(now + 30, now), "just now");
        assert_eq!(relative_time(now - 300, now), "5m ago");
        assert_eq!(relative_time(now - 3 * 3600, now), "3h ago");
        assert_eq!(relative_time(now - 12 * 86400, now), "12d ago");
        assert_eq!(relative_time(now - 120 * 86400, now), "4mo ago");
        assert_eq!(relative_time(now - 800 * 86400, now), "2y ago");
    }
}
