//! Explicit deterministic RNG plumbing for simulation code.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

pub type SpacewarsRng = StdRng;

pub fn seeded_rng(seed: u64) -> SpacewarsRng {
    StdRng::seed_from_u64(seed)
}

pub fn random_unit_f32(rng: &mut impl Rng) -> f32 {
    rng.random()
}

pub fn random_range_f32(rng: &mut impl Rng, min: f32, max: f32) -> f32 {
    rng.random_range(min..max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_replays_the_same_sequence() {
        let mut a = seeded_rng(0x5eed);
        let mut b = seeded_rng(0x5eed);

        let a_values = [
            random_unit_f32(&mut a),
            random_range_f32(&mut a, -10.0, 10.0),
            random_unit_f32(&mut a),
        ];
        let b_values = [
            random_unit_f32(&mut b),
            random_range_f32(&mut b, -10.0, 10.0),
            random_unit_f32(&mut b),
        ];

        assert_eq!(a_values, b_values);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = seeded_rng(1);
        let mut b = seeded_rng(2);

        assert_ne!(random_unit_f32(&mut a), random_unit_f32(&mut b));
    }
}
