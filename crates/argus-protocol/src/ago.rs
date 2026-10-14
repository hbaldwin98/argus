//! How long ago something happened, as every client says it: `5m`, `2h`,
//! `3d`, `1w`, and `—` for a time nobody knows.

pub fn ago(then: Option<u64>, now: u64) -> String {
    let Some(then) = then else {
        return "—".into();
    };
    let secs = now.saturating_sub(then);
    match secs {
        0..=3599 => format!("{}m", (secs / 60).max(1)),
        3600..=86_399 => format!("{}h", secs / 3600),
        86_400..=604_799 => format!("{}d", secs / 86_400),
        _ => format!("{}w", secs / 604_800),
    }
}

/// Seconds since the epoch by this machine's clock.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_age_is_said_in_its_largest_whole_unit() {
        assert_eq!(ago(Some(100), 100), "1m", "never less than a minute");
        assert_eq!(ago(Some(0), 7_200), "2h");
        assert_eq!(ago(Some(0), 3 * 86_400), "3d");
        assert_eq!(ago(Some(0), 15 * 86_400), "2w");
        assert_eq!(ago(None, 100), "—");
    }
}
