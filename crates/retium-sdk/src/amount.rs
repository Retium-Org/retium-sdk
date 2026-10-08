//! RTM amounts.
//!
//! One RTM is 10^18 atomic units. Everything signed and everything sent over
//! the wire is in atomic units, as a decimal string, never a float, which
//! could not represent these values exactly.

use crate::error::{Error, Result};

/// Atomic units in one RTM.
pub const ATOMIC_PER_RTM: u128 = 1_000_000_000_000_000_000;
/// Decimal places.
pub const DECIMALS: u32 = 18;

/// Parse human input ("1", "0.5", "1 234.75") into atomic units.
///
/// Refuses anything it cannot represent exactly: more than 18 decimal places,
/// a negative sign, letters, or an empty string.
pub fn parse_rtm(input: &str) -> Result<u128> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();
    if cleaned.is_empty() {
        return Err(Error::InvalidAmount(input.to_string()));
    }
    let (whole, fraction) = match cleaned.split_once('.') {
        Some((w, f)) => (w, f),
        None => (cleaned.as_str(), ""),
    };
    if whole.is_empty() && fraction.is_empty() {
        return Err(Error::InvalidAmount(input.to_string()));
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return Err(Error::InvalidAmount(input.to_string()));
    }
    if fraction.len() > DECIMALS as usize {
        return Err(Error::InvalidAmount(format!(
            "{input}: more than {DECIMALS} decimal places"
        )));
    }
    let whole: u128 = if whole.is_empty() {
        0
    } else {
        whole
            .parse()
            .map_err(|_| Error::InvalidAmount(input.to_string()))?
    };
    let mut padded = fraction.to_string();
    while padded.len() < DECIMALS as usize {
        padded.push('0');
    }
    let fraction: u128 = if padded.is_empty() {
        0
    } else {
        padded
            .parse()
            .map_err(|_| Error::InvalidAmount(input.to_string()))?
    };
    whole
        .checked_mul(ATOMIC_PER_RTM)
        .and_then(|w| w.checked_add(fraction))
        .ok_or_else(|| Error::InvalidAmount(format!("{input}: too large")))
}

/// Atomic units as an exact decimal string, trailing zeros trimmed.
pub fn format_rtm(atomic: u128) -> String {
    let whole = atomic / ATOMIC_PER_RTM;
    let fraction = atomic % ATOMIC_PER_RTM;
    if fraction == 0 {
        return whole.to_string();
    }
    let mut frac = format!("{:018}", fraction);
    while frac.ends_with('0') {
        frac.pop();
    }
    format!("{whole}.{frac}")
}

/// Parse an atomic-unit string as it comes back from the node.
pub fn parse_atomic(s: &str) -> Result<u128> {
    s.trim()
        .parse()
        .map_err(|_| Error::InvalidAmount(s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_and_fractional_amounts_parse() {
        assert_eq!(parse_rtm("1").unwrap(), ATOMIC_PER_RTM);
        assert_eq!(parse_rtm("0.5").unwrap(), ATOMIC_PER_RTM / 2);
        assert_eq!(parse_rtm(".5").unwrap(), ATOMIC_PER_RTM / 2);
        assert_eq!(parse_rtm("1 234").unwrap(), 1234 * ATOMIC_PER_RTM);
        assert_eq!(parse_rtm("0.000000000000000001").unwrap(), 1);
        assert_eq!(parse_rtm("0").unwrap(), 0);
    }

    #[test]
    fn nonsense_is_refused_rather_than_rounded() {
        for bad in [
            "",
            " ",
            "-1",
            "1.2.3",
            "1e18",
            "abc",
            "0.0000000000000000001",
        ] {
            assert!(parse_rtm(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn formatting_is_exact_and_round_trips() {
        assert_eq!(format_rtm(ATOMIC_PER_RTM), "1");
        assert_eq!(format_rtm(ATOMIC_PER_RTM / 2), "0.5");
        assert_eq!(format_rtm(1), "0.000000000000000001");
        assert_eq!(format_rtm(0), "0");
        for text in ["0", "1", "0.5", "12.25", "0.000000000000000001", "123456"] {
            let atomic = parse_rtm(text).unwrap();
            assert_eq!(
                format_rtm(atomic),
                text.trim_start_matches('.').to_string().replace(' ', ""),
                "round trip of {text}"
            );
        }
    }
}
