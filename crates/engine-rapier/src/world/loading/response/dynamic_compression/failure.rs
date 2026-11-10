//! Opt-in dissipative collapse of one fixed support at an internal-step boundary.
use super::*;

/// A single loss of load-bearing capacity, retained with the material's identity.
/// Motion is unchanged at this boundary. Stored elastic energy and plastic work
/// beyond the last capacity charge become dissipation, never a second impulse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionFailure {
    /// Absolute elapsed physical time of the committed internal-step endpoint.
    pub elapsed: f64,
    pub source: ContactSource,
    pub episode: EpisodeId,
    pub indentation: f64,
    pub velocity: f64,
    pub plastic_indentation: f64,
    pub released_elastic: f64,
    pub excess_work: f64,
}
impl CompressionFailure {
    pub fn dissipated(&self) -> f64 {
        self.released_elastic + self.excess_work
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct FailureHistory {
    pub source: ContactSource,
    pub event: Option<CompressionFailure>,
}
impl FailureHistory {
    pub fn valid(&self, h: &History) -> bool {
        if h.moving.is_some()
            || h.paired.is_some()
            || h.loads.is_some()
            || self.source.region.incarnation == 0
        {
            return false;
        }
        let Some(e) = &self.event else { return true };
        let c = h.curve();
        let s = &h.summary;
        let flight = s.elapsed - e.elapsed;
        let mass = f64::from(h.mass);
        let acceleration = f64::from(h.acceleration);
        e.source == self.source
            && e.episode.0 > 0
            && e.elapsed.is_finite()
            && e.elapsed > 0.0
            && e.elapsed <= s.elapsed
            && (0.0..=c.depth / 4.0).contains(&e.indentation)
            && e.indentation >= e.plastic_indentation
            && e.plastic_indentation == s.plastic_indentation
            && e.velocity.is_finite()
            && e.velocity.abs() <= 64.0
            && f64::from(e.velocity as f32) == e.velocity
            && (mass * (s.velocity - e.velocity - acceleration * flight)).abs()
                <= s.absolute_errors[1] + 1e-10 * s.scales[1]
            && (s.indentation
                - e.indentation
                - e.velocity * flight
                - 0.5 * acceleration * flight * flight)
                .abs()
                <= s.absolute_errors[2] + flight * s.absolute_errors[1] / mass + 1e-10 * c.depth
            && e.released_elastic == c.energy(e.indentation, e.plastic_indentation)
            && e.released_elastic > 0.0
            && (0.0..=c.y * c.depth / 4.0).contains(&e.excess_work)
            && s.damage_work == h.initial.remaining
            && s.elastic_energy == 0.0
    }
}
impl History {
    pub(super) fn collapsed(&self) -> bool {
        self.failure.as_ref().is_some_and(|f| f.event.is_some())
    }
    pub(super) fn released_elastic(&self) -> f64 {
        self.failure
            .as_ref()
            .and_then(|f| f.event.as_ref())
            .map_or(0.0, |e| e.released_elastic)
    }
    pub(super) fn failure_excess(&self) -> f64 {
        self.failure
            .as_ref()
            .and_then(|f| f.event.as_ref())
            .map_or(0.0, |e| e.excess_work)
    }
}
impl PhysicsWorld {
    /// Opt into irreversible collapse of a single fixed material support.
    ///
    /// Uses the dynamic-compression admission, time-step and numerical bounds,
    /// with constant normal gravity and an ideal rigid dynamic platen. Moving
    /// supports, paired materials and load changes are not admitted by this mode.
    /// The layer resists through the first internal interval exhausting capacity;
    /// collapse then dissipates its stored elastic energy and excess plastic work.
    /// Later intervals have zero reaction and charge: the platen follows gravity
    /// through the collapsed envelope. Mass, geometry and actor ownership remain.
    ///
    /// One failure and contact End are reported at that internal endpoint. Failure
    /// timing is quantized by the existing subdivision, at most 1/4096 second.
    /// The whole public call is atomic, including failures and output limits.
    /// Snapshot v21 retains the policy and failure even after invalidation/removal.
    /// Enrolled histories cannot switch to or from the legacy refusal policy.
    pub fn step_material_collapsing_support(
        &mut self,
        dt: f32,
    ) -> Result<DynamicCompressionStep, R> {
        step::advance(self, dt, true)
    }

    /// The committed failure, available after pause, restore or invalidation.
    /// Step output emits it once; this accessor does not consume the observation.
    pub fn material_compression_failure(&self) -> Option<CompressionFailure> {
        self.loading
            .as_ref()?
            .dynamic_compression
            .as_ref()?
            .history
            .failure
            .as_ref()?
            .event
            .clone()
    }
}

#[cfg(test)]
#[path = "tests/failure.rs"]
mod tests;
