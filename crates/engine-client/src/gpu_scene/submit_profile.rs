//! Probe-only CPU wall clocks around GL calls. These are not GPU-active times:
//! a driver may defer work or wait inside any of the measured calls.
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Guard,
    Clear,
    Setup,
    VertexReserve,
    VertexUpload,
    IndexReserve,
    IndexUpload,
    Batches,
    ErrorCheck,
    MinimapResolve,
    MinimapComposite,
    FinalResolve,
}

pub(crate) const NAMES: [&str; 12] = [
    "guard_ms",
    "clear_ms",
    "setup_ms",
    "vertex_reserve_ms",
    "vertex_upload_ms",
    "index_reserve_ms",
    "index_upload_ms",
    "batches_ms",
    "error_check_ms",
    "minimap_resolve_ms",
    "minimap_composite_ms",
    "final_resolve_ms",
];

#[derive(Clone, Copy, Default)]
pub(crate) struct Profile(pub [Duration; 12]);

/// Even-sized Williams design: every mode occupies each position once, and
/// every unequal ordered pair occurs once within rows over `count` repeats.
/// Cross-row transitions and longer-lived driver history are not balanced.
pub(crate) fn balanced_order(count: usize, repeat: usize) -> Vec<usize> {
    assert!(count >= 2 && count.is_multiple_of(2));
    (0..count)
        .map(|i| {
            let base = if i % 2 == 0 {
                count - i / 2
            } else {
                i.div_ceil(2)
            };
            (base + repeat % count) % count
        })
        .collect()
}

impl Profile {
    pub fn accumulate(&mut self, other: Self) {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            *a += b;
        }
    }
}

/// Monomorphization removes both clocks and accumulation from normal drawing.
pub(crate) struct Timer<const ENABLED: bool>(Option<Instant>);

impl<const ENABLED: bool> Timer<ENABLED> {
    pub fn new() -> Self {
        Self(ENABLED.then(Instant::now))
    }

    pub fn mark(&mut self, profile: &mut Profile, stage: Stage) {
        if ENABLED {
            let now = Instant::now();
            profile.0[stage as usize] += now - self.0.unwrap();
            self.0 = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_balances_positions_and_within_row_predecessors() {
        for count in [2, 4, 6, 8, 12] {
            let mut positions = vec![vec![0; count]; count];
            let mut pairs = vec![vec![0; count]; count];
            for repeat in 0..count {
                let row = balanced_order(count, repeat);
                for (position, mode) in row.iter().enumerate() {
                    positions[*mode][position] += 1;
                }
                for pair in row.windows(2) {
                    pairs[pair[0]][pair[1]] += 1;
                }
                let mut sorted = row.clone();
                sorted.sort_unstable();
                assert_eq!(sorted, (0..count).collect::<Vec<_>>());
                assert_eq!(row, balanced_order(count, repeat + count));
            }
            assert!(positions.iter().flatten().all(|n| *n == 1));
            for (a, row) in pairs.iter().enumerate() {
                for (b, n) in row.iter().enumerate() {
                    assert_eq!(*n, usize::from(a != b));
                }
            }
        }
    }

    #[test]
    fn disabled_timer_does_not_record_or_read_time() {
        let mut timer = Timer::<false>::new();
        let mut profile = Profile::default();
        timer.mark(&mut profile, Stage::VertexUpload);
        assert!(timer.0.is_none());
        assert!(profile.0.iter().all(Duration::is_zero));
    }

    #[test]
    fn stages_accumulate_independently() {
        let mut a = Profile::default();
        let mut b = Profile::default();
        a.0[Stage::Setup as usize] = Duration::from_micros(4);
        b.0[Stage::Setup as usize] = Duration::from_micros(6);
        b.0[Stage::IndexUpload as usize] = Duration::from_micros(3);
        a.accumulate(b);
        assert_eq!(a.0[Stage::Setup as usize], Duration::from_micros(10));
        assert_eq!(a.0.iter().sum::<Duration>(), Duration::from_micros(13));
        assert_eq!(Stage::FinalResolve as usize + 1, NAMES.len());
    }
}
