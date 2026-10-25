//! Transactional constant-gravity motion, material impacts and safe support for
//! the admitted two-plate scene. Other arrival methods still refuse gravity.
use super::*;

mod api;
pub use api::*;
pub(super) mod phase;

/// Policy for a return that has not proved departure from its original episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GravitySettling {
    /// Allow safe rest, but refuse unresolved approaching returns.
    Refuse,
    /// Permit one bounded numerical settlement with retained containment evidence.
    /// This does not select material damping or charge damage.
    BoundedNumerical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::world) enum Invalidation {
    ExternalEdit,
    OtherStep,
}

/// None in LoadingState means never enrolled, never an invalidated history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(in crate::world) struct Continuation {
    history: History,
    status: Status,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Status {
    Active(Box<Active>),
    Invalidated(Invalidation),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct History {
    revision: u32,
    settling: GravitySettling,
    // Initial touching-center coordinate, velocity, mass and acceleration.
    reference: [f64; 4],
    // Energy, momentum and distance. Frozen through every episode transition.
    scales: [f64; 3],
    // Signed observed flight defects, not damage or deferred force/time.
    errors: [f64; 3],
}

impl History {
    fn valid(&self) -> bool {
        let [x, v, m, a] = self.reference;
        let energy = (0.5 * m * v * v - m * a * x).max(0.0);
        let expected = [energy, libm::sqrt(2.0 * m * energy), x.abs().max(1.0)];
        self.revision == 1
            // Reference values came from stored f32 backend state. Enforcing
            // that also prevents finite f64 inputs overflowing the work formula.
            && self.reference.iter().all(|n| {
                (*n as f32).is_finite() && f64::from(*n as f32) == *n
            })
            && x <= 1.0e-6
            && m == 20.0
            && a > 0.0
            && self.scales == expected
            && self.scales.iter().zip(self.errors).all(|(scale, error)| {
                scale.is_finite() && error.is_finite() && error.abs() <= 1.0e-5 * scale
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Active {
    // Dynamic then fixed: role order is independent of canonical source order.
    plates: [Plate; 2],
    capture: CaptureConfig,
    // dt is normalized to zero; it is transient backend bookkeeping.
    integration: IntegrationParameters,
    episode: Option<OwnedEpisode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Plate {
    body: BodyId,
    source: ContactSource,
    slices: u8,
    copies: u8,
    // Sorted, populated prefix; fixed size bounds persisted topology.
    colliders: [Option<ColliderId>; 8],
    material: MaterialIdentity,
    endpoint: Endpoint,
    additional_iterations: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MaterialIdentity {
    parameters: [f32; 5],
    brittle: bool,
    volume: f32,
    thickness: f32,
    depth: f32,
}

impl MaterialIdentity {
    fn matches(&self, region: &StoredRegion) -> bool {
        self.parameters == region.parameters
            && self.brittle == region.brittle
            && self.volume == region.volume
            && self.thickness == region.thickness
            && self.depth == region.depth
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Endpoint {
    position: [f32; 2],
    velocity: [f32; 2],
    angle: f32,
    angular_velocity: f32,
    mass: f32,
    local_com: [f32; 2],
    inertia: f32,
}

impl Endpoint {
    fn valid(&self) -> bool {
        self.position
            .iter()
            .chain(&self.velocity)
            .chain(&self.local_com)
            .all(|x| x.is_finite())
            && self.position[1] == 0.0
            && self.velocity[1] == 0.0
            && self.angle == 0.0
            && self.angular_velocity == 0.0
            && self.mass.is_finite()
            && self.mass >= 0.0
            && self.inertia.is_finite()
            && self.inertia >= 0.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OwnedEpisode {
    sources: [ContactSource; 2],
    id: EpisodeId,
    energy: f64,
    gap_upper: f64,
    turned: bool,
    held: bool,
    settled_work: f64,
}

impl Active {
    fn valid_for(&self, loading: &LoadingState) -> bool {
        let Some(response) = &loading.response else {
            return false;
        };
        let [dynamic, fixed] = &self.plates;
        let integration = &self.integration;
        let n = integration.num_solver_iterations.checked_add(
            self.plates
                .iter()
                .map(|p| p.additional_iterations)
                .max()
                .unwrap_or(0),
        );
        if !self.capture.valid()
            || self.capture != loading.config
            || self.capture.touch_tolerance != 1.0e-5
            || dynamic.body == fixed.body
            || dynamic.source.region == fixed.source.region
            || loading.regions.len() != 2
            || response.materials.len() != 2
            || dynamic.endpoint.mass != 20.0
            || dynamic.endpoint.position[0] > 1.0e-6
            || fixed.endpoint.position != [1.0, 0.0]
            || fixed.endpoint.velocity != [0.0, 0.0]
            || integration.dt != 0.0
            || !integration.length_unit.is_finite()
            || integration.length_unit <= 0.0
            || !integration.warmstart_coefficient.is_finite()
            || !(0.0..=1.0).contains(&integration.warmstart_coefficient)
            || [
                integration.min_ccd_dt,
                integration.contact_softness.natural_frequency,
                integration.contact_softness.damping_ratio,
                integration.normalized_allowed_linear_error,
                integration.normalized_max_corrective_velocity,
                integration.normalized_prediction_distance,
            ]
            .iter()
            .any(|x| !x.is_finite() || *x < 0.0)
            || integration.num_solver_iterations == 0
            || n.is_none_or(|n| 16.0 * (n as f64 + 4.0) * f64::from(f32::EPSILON) >= 0.01)
            || loading
                .arrival_departures
                .as_ref()
                .is_some_and(|s| !s.is_empty())
            || loading
                .flat_departures
                .as_ref()
                .is_some_and(|s| !s.is_empty())
        {
            return false;
        }
        let mut count = 0;
        for plate in &self.plates {
            let expected = usize::from(plate.slices) * usize::from(plate.copies);
            if ![1, 2, 4].contains(&plate.slices)
                || ![1, 2].contains(&plate.copies)
                || !plate.endpoint.valid()
                || !loading.live(plate.source.region)
                || loading.regions[&plate.source.region.id].body != plate.body
                || !response
                    .materials
                    .get(&plate.source.region)
                    .is_some_and(|r| plate.material.matches(r))
            {
                return false;
            }
            let mut previous = None;
            for (i, id) in plate.colliders.iter().enumerate() {
                if i >= expected {
                    if id.is_some() {
                        return false;
                    }
                    continue;
                }
                let Some(id) = id else { return false };
                if previous.is_some_and(|p| p >= *id)
                    || id.entity != plate.body.entity
                    || loading.bindings.get(id) != Some(&plate.source)
                {
                    return false;
                }
                previous = Some(*id);
                count += 1;
            }
        }
        if count != loading.bindings.len() {
            return false;
        }
        let Some(episode) = &self.episode else {
            return loading.episodes.is_empty();
        };
        let mut pair = self.plates.each_ref().map(|p| p.source);
        pair.sort();
        loading.episodes.len() == 1
            && episode.sources == pair
            && loading
                .episodes
                .get(&pair)
                .is_some_and(|e| e.id == episode.id)
            && episode.energy.is_finite()
            && episode.energy >= 0.0
            && (episode.energy * (1.0 + 1.0e-5)).is_finite()
            && episode.gap_upper.is_finite()
            && episode.gap_upper >= -f64::from(dynamic.endpoint.position[0])
            && episode.settled_work.is_finite()
            && episode.settled_work >= 0.0
            && episode.settled_work <= episode.energy * (1.0 + 1.0e-5)
            && (episode.held || episode.settled_work == 0.0)
            && (!episode.held
                || (dynamic.endpoint.velocity == [0.0, 0.0]
                    && dynamic.endpoint.position[0].abs() <= 1.0e-6))
            && (episode.settled_work == 0.0
                || (episode.turned
                    && episode.gap_upper <= 2.0 * f64::from(self.capture.touch_tolerance)))
    }
}

impl Continuation {
    pub(in crate::world) fn invalidate(&mut self, reason: Invalidation) {
        if matches!(self.status, Status::Active(_)) {
            // Preserve the first cause and history; drop all live identity claims.
            self.status = Status::Invalidated(reason);
        }
    }

    pub(in crate::world) fn valid_for(&self, loading: &LoadingState) -> bool {
        self.history.valid()
            && match &self.status {
                Status::Invalidated(_) => true,
                Status::Active(active) => {
                    active.valid_for(loading)
                        && active.episode.as_ref().is_none_or(|e| {
                            e.settled_work == 0.0
                                || self.history.settling == GravitySettling::BoundedNumerical
                        })
                }
            }
    }
}

#[cfg(test)]
mod tests;
