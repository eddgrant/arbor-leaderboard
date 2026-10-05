//! Money is held as integer pence to avoid floating point drift.

/// Parses Arbor's money strings such as "£13.40", "-£1.20", "£-1.20" or "£2" into pence.
pub fn parse_pence(s: &str) -> Option<i64> {
    let s = s.trim();
    let negative = s.contains('-');
    let digits: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if digits.is_empty() {
        return None;
    }
    let (pounds, pence) = match digits.split_once('.') {
        Some((p, f)) => {
            let f = format!("{f:0<2}");
            (p.parse::<i64>().ok()?, f[..2].parse::<i64>().ok()?)
        }
        None => (digits.parse::<i64>().ok()?, 0),
    };
    let total = pounds * 100 + pence;
    Some(if negative { -total } else { total })
}

#[cfg(test)]
mod tests {
    use super::parse_pence;

    #[test]
    fn parses_common_formats() {
        assert_eq!(parse_pence("£13.40"), Some(1340));
        assert_eq!(parse_pence("Balance: £11.00"), Some(1100));
        assert_eq!(parse_pence("-£1.20"), Some(-120));
        assert_eq!(parse_pence("£-1.20"), Some(-120));
        assert_eq!(parse_pence("£2"), Some(200));
        assert_eq!(parse_pence("£0.5"), Some(50));
        assert_eq!(parse_pence("n/a"), None);
    }
}
