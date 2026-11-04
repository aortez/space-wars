use super::*;
use serde::Serialize;

/// Run totals, including zero-duration pointer actions. No net-population inference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PizzaBirthCounts {
    pub spawned: u64,
    pub pointer_created: u64,
    pub pointer_replaced: u64,
    pub exploded: u64,
    /// Generation 0, 1, 2, 3+. Diagnostic ancestry does not affect damage.
    pub exploded_by_generation: [u64; 4],
    /// Radii <0.005, <0.01, >=0.01 world units.
    pub exploded_by_size: [u64; 3],
    pub fragments_created: u64,
    /// Stops immediately after a fragment fills the budget. Remaining random
    /// candidates are not sampled, preserving the existing RNG/lifecycle order.
    pub fragment_capacity_stops: u64,
    /// Whole dead parents not sampled after the budget filled. Their unknown
    /// potential fragment count is deliberately not reported as refused births.
    pub fragment_parents_skipped: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PizzaPopulation {
    pub limit: usize,
    pub balls: usize,
    pub physics_balls: usize,
    pub ball_colliders: usize,
    pub births: PizzaBirthCounts,
}

impl PizzaState {
    /// Normal gameplay counts include every fragment and any held body. The one
    /// fixed boundary body and its four wall colliders are excluded.
    pub fn population(&self) -> PizzaPopulation {
        PizzaPopulation {
            limit: self
                .config
                .benchmark
                .map_or(self.config.live_ball_limit, |_| MAX_BENCHMARK_BALLS),
            balls: self.balls.len(),
            physics_balls: self
                .physics
                .as_ref()
                .map_or(0, |p| p.body_count().saturating_sub(1)),
            ball_colliders: self
                .physics
                .as_ref()
                .map_or(0, |p| p.collider_count().saturating_sub(4)),
            births: self.birth_counts,
        }
    }
}
