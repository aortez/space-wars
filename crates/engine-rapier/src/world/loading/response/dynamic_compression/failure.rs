//! Opt-in dissipative collapse of one material support at an internal-step boundary.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Policy {
    Refuse,
    Support,
    Pair,
}

/// A single loss of load-bearing capacity, retained with the material's identity.
/// Motion is unchanged at this boundary. Stored elastic energy and plastic work
/// beyond the last capacity charge become dissipation, never a second impulse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionFailure {
    /// Absolute elapsed physical time of the committed internal-step endpoint.
    pub elapsed: f64,
    pub source: ContactSource,
    pub episode: EpisodeId,
    /// This layer's displacement; for one material it equals relative displacement.
    pub indentation: f64,
    /// Relative normal velocity; the support is stationary in the fixed mode.
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
    // Required v22 extension; the fixed-support v21 layout is unchanged.
    #[serde(skip)]
    pub motion: Option<moving::Motion>,
}
impl FailureHistory {
    pub fn valid(&self, h: &History) -> bool {
        if h.paired.is_some() || h.loads.is_some() || self.source.region.incarnation == 0 {
            return false;
        }
        let Some(e) = &self.event else {
            return self.motion.is_none();
        };
        let c = h.curve();
        let s = &h.summary;
        let flight = s.elapsed - e.elapsed;
        let mass = h.effective_mass();
        let acceleration = f64::from(h.acceleration);
        let motion_valid = match (&h.moving, &self.motion) {
            (None, None) => true,
            (Some(m), Some(boundary)) => {
                m.valid_failure(boundary, e.indentation, e.velocity, flight, c.depth)
            }
            _ => false,
        };
        motion_valid
            && e.source == self.source
            && e.episode.0 > 0
            && e.elapsed.is_finite()
            && e.elapsed > 0.0
            && e.elapsed <= s.elapsed
            && (0.0..=c.depth / 4.0).contains(&e.indentation)
            && e.indentation >= e.plastic_indentation
            && e.plastic_indentation == s.plastic_indentation
            && e.velocity.is_finite()
            && e.velocity.abs() <= 64.0
            && (h.moving.is_some() || f64::from(e.velocity as f32) == e.velocity)
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
    pub(super) fn failure_policy(&self) -> Policy {
        if self.failure.is_some() {
            Policy::Support
        } else if self.paired.as_ref().is_some_and(|p| p.failure_enabled()) {
            Policy::Pair
        } else {
            Policy::Refuse
        }
    }
    pub(super) fn collapsed(&self) -> bool {
        self.failure.as_ref().is_some_and(|f| f.event.is_some())
            || self.paired.as_ref().is_some_and(|p| p.collapsed())
    }
    pub(super) fn released_elastic(&self) -> f64 {
        self.failure
            .as_ref()
            .and_then(|f| f.event.as_ref())
            .map_or_else(
                || self.paired.as_ref().map_or(0.0, |p| p.released_elastic()),
                |e| e.released_elastic,
            )
    }
    pub(super) fn failure_excess(&self) -> f64 {
        self.failure
            .as_ref()
            .and_then(|f| f.event.as_ref())
            .map_or_else(
                || self.paired.as_ref().map_or(0.0, |p| p.failure_excess()),
                |e| e.excess_work,
            )
    }
    pub(super) fn committed_failures(&self) -> Vec<CompressionFailure> {
        if let Some(event) = self.failure.as_ref().and_then(|f| f.event.as_ref()) {
            vec![event.clone()]
        } else {
            self.paired
                .as_ref()
                .map_or_else(Vec::new, |p| p.committed_failures())
        }
    }
}
impl PhysicsWorld {
    /// Opt into irreversible collapse of a single material support.
    ///
    /// Uses the dynamic-compression admission, time-step and numerical bounds,
    /// with an ideal rigid dynamic platen. A fixed support admits constant normal
    /// gravity; a moving support admits two free bodies and zero gravity. Paired
    /// materials and load changes are not admitted by this mode.
    /// The layer resists through the first internal interval exhausting capacity;
    /// collapse then dissipates its stored elastic energy and excess plastic work.
    /// Later intervals have zero reaction and charge: a fixed-support platen
    /// follows gravity and two free bodies coast. Mass, geometry and actor
    /// ownership remain. A moving support retains both bodies' boundary motion
    /// through `material_moving_support_failure()`.
    ///
    /// One failure and contact End are reported at that internal endpoint. Failure
    /// timing is quantized by the existing subdivision, at most 1/4096 second.
    /// The whole public call is atomic, including failures and output limits.
    /// Snapshots v21 (fixed) and v22 (moving) retain the policy and failure even
    /// after invalidation/removal.
    /// Enrolled histories cannot switch to or from the legacy refusal policy.
    pub fn step_material_collapsing_support(
        &mut self,
        dt: f32,
    ) -> Result<DynamicCompressionStep, R> {
        step::advance(self, dt, Policy::Support)
    }

    /// The committed single-support collapse (fixed or moving), available after
    /// pause, restore or invalidation. Step output emits it once; reading does
    /// not consume it. For paired collapse this returns `None`; use
    /// [`Self::material_paired_compression_failure()`] for the contact boundary
    /// and both layers' dispositions.
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

#[cfg(test)]
#[path = "tests/moving_failure.rs"]
mod moving_tests;
