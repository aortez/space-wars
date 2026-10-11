//! Bounded, opt-in triangle stream policies. Capacities describe requested
//! GL stores, not actual driver allocations or peak in-flight GPU memory.

pub(crate) const MAX_SLOTS: usize = 8;
pub(crate) const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Policy {
    #[default]
    SharedExact,
    PassExact,
    PassSplitExact,
    PassBucketed,
}

impl Policy {
    pub fn split_upload(self) -> bool {
        matches!(self, Self::PassSplitExact | Self::PassBucketed)
    }

    pub fn capacities(self, old: [usize; 2], used: [usize; 2]) -> Option<[usize; 2]> {
        if self != Self::PassBucketed {
            return Some(used);
        }
        let grow = |old: usize, used: usize| {
            if used <= old {
                Some(old)
            } else {
                used.max(4096).checked_next_power_of_two()
            }
        };
        Some([grow(old[0], used[0])?, grow(old[1], used[1])?])
    }
}

pub(crate) fn fits(slot: usize, pooled: usize, old: [usize; 2], new: [usize; 2]) -> bool {
    slot < MAX_SLOTS
        && pooled
            .saturating_sub(old[0].saturating_add(old[1]))
            .saturating_add(new[0])
            .saturating_add(new[1])
            <= MAX_BYTES
}

pub(crate) const NAMES: [&str; 5] = [
    "buffer_data_calls",
    "buffer_subdata_calls",
    "buffer_store_bytes",
    "buffer_size_changes",
    "stream_fallbacks",
];

#[derive(Clone, Copy, Default)]
pub(crate) struct Stats(pub [u64; 5]);

impl Stats {
    pub fn upload(old: [usize; 2], new: [usize; 2], split: bool, fallback: bool) -> Self {
        Self([
            2,
            if split { 2 } else { 0 },
            (new[0] + new[1]) as u64,
            old.into_iter().zip(new).filter(|(a, b)| a != b).count() as u64,
            u64::from(fallback),
        ])
    }

    pub fn accumulate(&mut self, other: Self) {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            *a += b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucketed_capacity_grows_at_boundaries_and_never_shrinks() {
        let p = Policy::PassBucketed;
        assert_eq!(p.capacities([0; 2], [0; 2]), Some([0; 2]));
        assert_eq!(p.capacities([0; 2], [1, 4096]), Some([4096; 2]));
        assert_eq!(p.capacities([4096; 2], [4097, 0]), Some([8192, 4096]));
        assert_eq!(p.capacities([8192, 4096], [64, 4096]), Some([8192, 4096]));
        assert_eq!(p.capacities([0; 2], [usize::MAX, 1]), None);
        assert_eq!(
            Policy::PassExact.capacities([8192; 2], [64, 8]),
            Some([64, 8])
        );
        assert_eq!(Policy::SharedExact, Policy::default());
    }

    #[test]
    fn stream_budget_counts_reserved_capacity_and_replacement() {
        assert!(fits(7, MAX_BYTES, [512; 2], [512; 2]));
        assert!(!fits(8, 0, [0; 2], [1; 2]));
        assert!(!fits(0, 0, [0; 2], [MAX_BYTES, 1]));
        assert!(!fits(0, MAX_BYTES, [512; 2], [1025, 0]));
        assert!(fits(0, MAX_BYTES, [1024; 2], [512; 2]));
        assert!(!fits(0, MAX_BYTES, [0; 2], [usize::MAX; 2]));
        let used = [1, MAX_BYTES / 2 + 1];
        assert!(fits(0, 0, [0; 2], used));
        let reserved = Policy::PassBucketed.capacities([0; 2], used).unwrap();
        assert!(!fits(0, 0, [0; 2], reserved));
    }

    #[test]
    fn split_exact_changes_only_the_upload_sequence() {
        for (old, used) in [
            ([0; 2], [8193, 72]),
            ([16384, 4096], [48, 8]),
            ([4096; 2], [0; 2]),
        ] {
            assert_eq!(Policy::PassSplitExact.capacities(old, used), Some(used));
            assert_eq!(Policy::PassExact.capacities(old, used), Some(used));
        }
        assert!(!Policy::SharedExact.split_upload());
        assert!(!Policy::PassExact.split_upload());
        assert!(Policy::PassSplitExact.split_upload());
        assert!(Policy::PassBucketed.split_upload());
        let exact = Stats::upload([8192, 4096], [48, 8], false, false);
        let split = Stats::upload([8192, 4096], [48, 8], true, false);
        assert_eq!(exact.0, [2, 0, 56, 2, 0]);
        assert_eq!(split.0, [2, 2, 56, 2, 0]);
    }

    #[test]
    fn stats_distinguish_discard_calls_from_storage_size_changes() {
        let mut s = Stats::upload([0; 2], [4096; 2], true, false);
        s.accumulate(Stats::upload([4096; 2], [4096; 2], true, false));
        assert_eq!(s.0, [4, 4, 16384, 2, 0]);
        assert_eq!(
            Stats::upload([64, 8], [32, 8], false, true).0,
            [2, 0, 40, 1, 1]
        );
        assert_eq!(s.0.len(), NAMES.len());
    }
}
