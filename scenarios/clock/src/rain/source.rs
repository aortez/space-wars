//! Seeded, evenly covered showers without a visible spatial stride or cadence.
use rand::{Rng, rngs::StdRng, seq::SliceRandom};

const BANDS: usize = 32;

pub(crate) struct RainSource {
    rng: StdRng,
    bands: [u8; BANDS],
    index: usize,
    next_tick: u64,
}

impl RainSource {
    pub fn new(rng: StdRng) -> Self {
        Self {
            rng,
            bands: std::array::from_fn(|i| i as u8),
            index: BANDS,
            next_tick: 2,
        }
    }

    pub fn emission_count(&mut self, tick: u64) -> usize {
        if tick < self.next_tick {
            return 0;
        }
        // About one drop per tick on average, as before. The caller allocates
        // its existing volume budget, independent of drop count and timing.
        self.next_tick = tick + self.rng.random_range(1..=3);
        self.rng.random_range(1..=3)
    }

    pub fn next_fraction(&mut self) -> f32 {
        if self.index == BANDS {
            self.bands.shuffle(&mut self.rng);
            self.index = 0;
        }
        let band = self.bands[self.index];
        self.index += 1;
        (f32::from(band) + self.rng.random_range(0.05..0.95)) / BANDS as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn coverage_is_complete_but_order_is_not_a_repeating_stride() {
        for seed in 0..32 {
            let mut source = RainSource::new(StdRng::seed_from_u64(seed));
            let mut previous = None;
            for _ in 0..8 {
                let fractions: [f32; BANDS] = std::array::from_fn(|_| source.next_fraction());
                let bands = fractions.map(|x| (x * BANDS as f32) as u8);
                let mut sorted = bands;
                sorted.sort();
                assert_eq!(sorted, std::array::from_fn(|i| i as u8));
                assert!(fractions.iter().all(|x| (0.0..1.0).contains(x)));
                assert_ne!(Some(bands), previous);
                let stride = (bands[1] + BANDS as u8 - bands[0]) % BANDS as u8;
                assert!(
                    bands
                        .windows(2)
                        .any(|p| (p[1] + BANDS as u8 - p[0]) % BANDS as u8 != stride)
                );
                previous = Some(bands);
            }
        }
    }

    #[test]
    fn timing_varies_and_replays_exactly_without_unbounded_bursts() {
        let mut a = RainSource::new(StdRng::seed_from_u64(7));
        let mut b = RainSource::new(StdRng::seed_from_u64(7));
        let mut counts = [0; 4];
        let mut gaps = [0; 4];
        let mut previous = 0;
        for tick in 0..1200 {
            let count = a.emission_count(tick);
            assert_eq!(count, b.emission_count(tick));
            counts[count] += 1;
            if count > 0 {
                gaps[(tick - previous) as usize] += 1;
                previous = tick;
            }
            for _ in 0..count {
                assert_eq!(a.next_fraction(), b.next_fraction());
            }
        }
        assert!(counts.iter().all(|n| *n > 0));
        assert!(gaps[1..].iter().all(|n| *n > 0));
    }
}
