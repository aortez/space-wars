//! World selection belongs to the client; simulation randomness stays seeded.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

pub(crate) fn fresh_seed(current: u64) -> u64 {
    let candidate = RandomState::new().hash_one(current);
    if candidate == current {
        current.wrapping_add(1)
    } else {
        candidate
    }
}

pub(crate) fn length_label(seconds: u32) -> String {
    if seconds == 0 {
        "Unlimited".into()
    } else if seconds.is_multiple_of(60) {
        format!("{} min", seconds / 60)
    } else {
        format!("{seconds} s")
    }
}

pub(crate) fn parse_length(label: &str) -> Result<u32, String> {
    if label == "Unlimited" {
        return Ok(0);
    }
    let (number, multiplier) = if let Some(n) = label.strip_suffix(" min") {
        (n, 60)
    } else if let Some(n) = label.strip_suffix(" s") {
        (n, 1)
    } else {
        return Err("Match length must be minutes, seconds, or Unlimited.".into());
    };
    number
        .parse::<u32>()
        .ok()
        .and_then(|n| n.checked_mul(multiplier))
        .filter(|&n| (1..=3600).contains(&n))
        .ok_or_else(|| "Match length must be between one second and one hour.".into())
}
