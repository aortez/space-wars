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
