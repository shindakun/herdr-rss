//! Unix seconds, relative ages, and dates for display.

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// `5m`, `3h`, `2d`, `6w`. Negative ages read as `0m`.
pub fn age(secs: i64) -> String {
    let s = secs.max(0);
    match s {
        _ if s < 3_600 => format!("{}m", s / 60),
        _ if s < 86_400 => format!("{}h", s / 3_600),
        _ if s < 30 * 86_400 => format!("{}d", s / 86_400),
        _ => format!("{}w", s / (7 * 86_400)),
    }
}

pub fn date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(age(90), "1m");
        assert_eq!(age(7_200), "2h");
        assert_eq!(age(3 * 86_400), "3d");
        assert_eq!(age(60 * 86_400), "8w");
        assert_eq!(age(-5), "0m");
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01 00:00 UTC");
    }
}
