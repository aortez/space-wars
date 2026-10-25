//! Transactional constant-gravity motion, material impacts and safe support for
//! admitted pair and three-plate stack scenes. Other arrival methods still refuse gravity.
use super::*;

mod api;
pub use api::*;
pub(super) mod phase;
pub(super) mod stack;

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
    // Keep these first two tags and payloads frozen for snapshot v6.
    Active(Box<Active>),
    Invalidated(Invalidation),
    // Stack keeps its corrections outside its own active/invalidated state.
    // Stack tags are accepted in snapshot v7 (legacy) or v9 (bounded masses).
    Stack(Box<stack::State>),
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
    fn new(endpoint: &Endpoint, acceleration: f64, settling: GravitySettling) -> Self {
        let (x, v, m) = (
            f64::from(endpoint.position[0]),
            f64::from(endpoint.velocity[0]),
            f64::from(endpoint.mass),
        );
        let energy = (0.5 * m * v * v - m * acceleration * x).max(0.0);
        Self {
            revision: mass_revision(&[endpoint.mass]),
            settling,
            reference: [x, v, m, acceleration],
            scales: [energy, libm::sqrt(2.0 * m * energy), x.abs().max(1.0)],
            errors: [0.0; 3],
        }
    }

    fn valid(&self) -> bool {
        let [x, v, m, a] = self.reference;
        let energy = (0.5 * m * v * v - m * a * x).max(0.0);
        let expected = [energy, libm::sqrt(2.0 * m * energy), x.abs().max(1.0)];
        matches!(self.revision, 1 | 2)
            // Reference values came from stored f32 backend state. Enforcing
            // that also prevents finite f64 inputs overflowing the work formula.
            && self.reference.iter().all(|n| {
                (*n as f32).is_finite() && f64::from(*n as f32) == *n
            })
            && x <= 1.0e-6
            && valid_masses(self.revision, &[m as f32])
            && a > 0.0
            && self.scales == expected
            && self.scales.iter().zip(self.errors).all(|(scale, error)| {
                scale.is_finite() && error.is_finite() && error.abs() <= 1.0e-5 * scale
            })
    }
}

// Revision 1 retains the mass-20 contract and its exact v6/v7 encoding.
// Revision 2 bounds both scale and conditioning; per-step numerical guards
// still refuse responses that cannot be represented within their fixed limits.
fn mass_revision(masses: &[f32]) -> u32 {
    if masses.iter().all(|m| *m == 20.0) {
        1
    } else {
        2
    }
}

fn valid_masses(revision: u32, masses: &[f32]) -> bool {
    if !(1..=2).contains(&masses.len()) {
        return false;
    }
    match revision {
        1 => masses.iter().all(|m| *m == 20.0),
        2 => {
            masses.iter().all(|m| (5.0..=80.0).contains(m))
                && (masses.len() == 1
                    || f64::from(masses[0].max(masses[1]))
                        <= 4.0 * f64::from(masses[0].min(masses[1])))
        }
        _ => false,
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

fn valid_integration(integration: &IntegrationParameters, plates: &[Plate]) -> bool {
    let n = integration.num_solver_iterations.checked_add(
        plates
            .iter()
            .map(|p| p.additional_iterations)
            .max()
            .unwrap_or(0),
    );
    integration.dt == 0.0
        && integration.length_unit.is_finite()
        && integration.length_unit > 0.0
        && integration.warmstart_coefficient.is_finite()
        && (0.0..=1.0).contains(&integration.warmstart_coefficient)
        && [
            integration.min_ccd_dt,
            integration.contact_softness.natural_frequency,
            integration.contact_softness.damping_ratio,
            integration.normalized_allowed_linear_error,
            integration.normalized_max_corrective_velocity,
            integration.normalized_prediction_distance,
        ]
        .iter()
        .all(|x| x.is_finite() && *x >= 0.0)
        && integration.num_solver_iterations > 0
        && n.is_some_and(|n| 16.0 * (n as f64 + 4.0) * f64::from(f32::EPSILON) < 0.01)
}

// Shared structural checks, independent of the admitted role positions.
fn valid_contract(
    loading: &LoadingState,
    plates: &[Plate],
    capture: &CaptureConfig,
    integration: &IntegrationParameters,
) -> bool {
    let Some(response) = &loading.response else {
        return false;
    };
    if !capture.valid()
        || *capture != loading.config
        || capture.touch_tolerance != 1.0e-5
        || loading.regions.len() != plates.len()
        || response.materials.len() != plates.len()
        || !valid_integration(integration, plates)
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
    for (index, plate) in plates.iter().enumerate() {
        let expected = usize::from(plate.slices) * usize::from(plate.copies);
        if ![1, 2, 4].contains(&plate.slices)
            || ![1, 2].contains(&plate.copies)
            || !plate.endpoint.valid()
            || plates[..index]
                .iter()
                .any(|p| p.body == plate.body || p.source.region == plate.source.region)
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
    count == loading.bindings.len()
}

impl OwnedEpisode {
    fn valid_for(
        &self,
        loading: &LoadingState,
        mut pair: [ContactSource; 2],
        endpoint: &Endpoint,
    ) -> bool {
        pair.sort();
        self.sources == pair
            && loading.episodes.get(&pair).is_some_and(|e| e.id == self.id)
            && self.energy.is_finite()
            && self.energy >= 0.0
            && (self.energy * (1.0 + 1.0e-5)).is_finite()
            && self.gap_upper.is_finite()
            && self.gap_upper >= -f64::from(endpoint.position[0])
            && self.settled_work.is_finite()
            && self.settled_work >= 0.0
            && self.settled_work <= self.energy * (1.0 + 1.0e-5)
            && (self.held || self.settled_work == 0.0)
            && (!self.held
                || (endpoint.velocity == [0.0, 0.0] && endpoint.position[0].abs() <= 1.0e-6))
            && (self.settled_work == 0.0
                || (self.turned
                    && self.gap_upper <= 2.0 * f64::from(loading.config.touch_tolerance)))
    }
}

impl Active {
    fn valid_for(&self, loading: &LoadingState, history: &History) -> bool {
        let [dynamic, fixed] = &self.plates;
        valid_contract(loading, &self.plates, &self.capture, &self.integration)
            && valid_masses(history.revision, &[dynamic.endpoint.mass])
            && f64::from(dynamic.endpoint.mass) == history.reference[2]
            && dynamic.endpoint.position[0] <= 1.0e-6
            && fixed.endpoint.position == [1.0, 0.0]
            && fixed.endpoint.velocity == [0.0, 0.0]
            && match &self.episode {
                None => loading.episodes.is_empty(),
                Some(episode) => {
                    loading.episodes.len() == 1
                        && episode.valid_for(
                            loading,
                            self.plates.each_ref().map(|p| p.source),
                            &dynamic.endpoint,
                        )
                }
            }
    }
}

impl Continuation {
    pub(in crate::world) fn snapshot_version(&self) -> u32 {
        use crate::world::{
            GRAVITY_SNAPSHOT_VERSION, MASS_GRAVITY_SNAPSHOT_VERSION,
            MASS_STACK_GRAVITY_SNAPSHOT_VERSION, STACK_GRAVITY_SNAPSHOT_VERSION,
        };
        match (self.history.revision == 2, self.is_stack()) {
            (false, false) => GRAVITY_SNAPSHOT_VERSION,
            (false, true) => STACK_GRAVITY_SNAPSHOT_VERSION,
            (true, false) => MASS_GRAVITY_SNAPSHOT_VERSION,
            (true, true) => MASS_STACK_GRAVITY_SNAPSHOT_VERSION,
        }
    }

    pub(in crate::world) fn is_stack(&self) -> bool {
        matches!(self.status, Status::Stack(_))
    }

    fn invalidation(&self) -> Option<Invalidation> {
        match &self.status {
            Status::Active(_) => None,
            Status::Invalidated(reason) => Some(*reason),
            Status::Stack(stack) => stack.invalidation(),
        }
    }

    pub(in crate::world) fn invalidate(&mut self, reason: Invalidation) {
        match &mut self.status {
            Status::Active(_) => {
                // Preserve the first cause and history; drop live identity claims.
                self.status = Status::Invalidated(reason);
            }
            Status::Invalidated(_) => {}
            Status::Stack(stack) => stack.invalidate(reason),
        }
    }

    pub(in crate::world) fn valid_for(&self, loading: &LoadingState) -> bool {
        self.history.valid()
            && match &self.status {
                Status::Invalidated(_) => true,
                Status::Active(active) => {
                    active.valid_for(loading, &self.history)
                        && active.episode.as_ref().is_none_or(|e| {
                            e.settled_work == 0.0
                                || self.history.settling == GravitySettling::BoundedNumerical
                        })
                }
                Status::Stack(stack) => stack.valid_for(loading, &self.history),
            }
    }
}

#[cfg(test)]
mod tests;
