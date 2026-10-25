//! Opt-in pre-solve contact capture, with a separately enabled experimental
//! material response. Capture alone is read-only. Sources are physical surface patches;
//! independent patches must have different source IDs even on the same region.

use super::*;
use rapier2d::prelude::{ContactModificationContext, PhysicsHooks, SolverContactBatchContext};

pub mod response;

/// A logical material region, independent of collider partitioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegionId {
    pub entity: PhysicsId,
    pub part: u32,
}

/// World-issued incarnation. Retired tokens cannot resolve to a replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegionToken {
    pub id: RegionId,
    incarnation: u64,
}

/// One physical surface patch. Equivalent collider parts share this identity;
/// independently contacting patches use distinct values of `patch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContactSource {
    pub region: RegionToken,
    pub patch: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EpisodeId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpisodeTransition {
    /// A new touching episode with positive relative approach. This is a
    /// candidate impact, not permission to spend a separate budget per point.
    Impact,
    Support,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EpisodeEvent {
    pub id: EpisodeId,
    pub sources: [ContactSource; 2],
    pub transition: EpisodeTransition,
    pub closing_speed: f32,
}

/// Motion at the solver contact point before this substep's solve. Inverse
/// properties reflect locked axes; fixed/kinematic bodies contribute zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactBodyState {
    pub body: BodyId,
    pub kind: BodyKind,
    pub center_of_mass: Vec2,
    pub point_velocity: Vec2,
    pub angular_velocity: f32,
    pub inverse_mass: Vec2,
    pub inverse_inertia: f32,
    pub sleeping: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactRecord {
    pub sources: [ContactSource; 2],
    pub colliders: [ColliderId; 2],
    pub subshapes: [Option<u32>; 2],
    pub bodies: [ContactBodyState; 2],
    pub point: Vec2,
    /// Unit normal from source 0 to source 1. Positive closing speed means
    /// approach; tangent is (-normal.y, normal.x).
    pub normal: Vec2,
    pub separation: f32,
    pub closing_speed: f32,
    pub tangent_speed: f32,
    pub episode: Option<EpisodeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContactBatch {
    pub substep: usize,
    pub duration: f32,
    pub contacts: Vec<ContactRecord>,
    /// One transition per source pair, not per contact point or collider.
    pub events: Vec<EpisodeEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub max_regions: usize,
    pub max_bindings: usize,
    /// Counts pair, manifold and point visits, including unbound pairs.
    pub max_visits_per_batch: usize,
    pub max_contacts_per_batch: usize,
    pub max_episodes: usize,
    pub max_batches_per_step: usize,
    /// Combined contact records and episode transitions over all CCD substeps.
    pub max_records_per_step: usize,
    pub touch_tolerance: f32,
    pub min_approach_speed: f32,
    /// Absent-contact grace period. The last complete observation remains
    /// authoritative across rejected batches; those cannot rearm an episode.
    pub separation_grace_seconds: f32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            max_regions: 1024,
            max_bindings: 4096,
            max_visits_per_batch: 16384,
            max_contacts_per_batch: 2048,
            max_episodes: 1024,
            max_batches_per_step: 16,
            max_records_per_step: 8192,
            touch_tolerance: 1.0e-5,
            min_approach_speed: 1.0e-4,
            separation_grace_seconds: 1.0 / 60.0,
        }
    }
}

impl CaptureConfig {
    fn valid(self) -> bool {
        [
            self.max_regions,
            self.max_bindings,
            self.max_visits_per_batch,
            self.max_contacts_per_batch,
            self.max_episodes,
            self.max_batches_per_step,
            self.max_records_per_step,
        ]
        .into_iter()
        .all(|n| (1..=65536).contains(&n))
            && self.touch_tolerance.is_finite()
            && (0.0..=0.01).contains(&self.touch_tolerance)
            && self.min_approach_speed.is_finite()
            && (0.0..=1.0).contains(&self.min_approach_speed)
            && self.separation_grace_seconds.is_finite()
            && (0.0..=1.0).contains(&self.separation_grace_seconds)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureRejection {
    VisitLimit,
    ContactLimit,
    EpisodeLimit,
    StepLimit,
    NonFinite,
    IdentityExhausted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureDiagnostics {
    pub batches_seen: usize,
    pub rejected_batches: usize,
    pub last_rejection: Option<CaptureRejection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingError {
    InvalidConfig,
    AlreadyEnabled,
    Disabled,
    UnknownBody,
    RegionExists,
    StaleRegion,
    InvalidBinding,
    Capacity,
    IdentityExhausted,
    ResponseDisabled,
    MaterialExists,
    InvalidMaterial,
}

impl std::fmt::Display for LoadingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "loading capture: {self:?}")
    }
}
impl std::error::Error for LoadingError {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct RegionEntry {
    token: RegionToken,
    body: BodyId,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Episode {
    id: EpisodeId,
    absent_seconds: f64,
}

struct PreparedBatch {
    batch: ContactBatch,
    episodes: BTreeMap<[ContactSource; 2], Episode>,
    next_episode: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct LoadingState {
    config: CaptureConfig,
    next_region: u64,
    next_episode: u64,
    regions: BTreeMap<RegionId, RegionEntry>,
    bindings: BTreeMap<ColliderId, ContactSource>,
    episodes: BTreeMap<[ContactSource; 2], Episode>,
    #[serde(skip)]
    batches: Vec<ContactBatch>,
    #[serde(skip)]
    diagnostics: CaptureDiagnostics,
    #[serde(skip)]
    records: usize,
    // Stored separately in snapshot v3, keeping the capture-only v2 layout.
    #[serde(skip)]
    response: Option<response::ResponseState>,
    // Snapshot v4 appends this after the unchanged v2/v3 payloads.
    #[serde(skip)]
    pub(in crate::world) arrival_departures: Option<response::ball_arrival::DepartureTracker>,
    // Snapshot v5 has a separate tail for optional ball and flat evidence.
    #[serde(skip)]
    pub(in crate::world) flat_departures: Option<response::flat_arrival::DepartureTracker>,
    // Snapshots v6 (pair) and v7 (stack) retain even invalidated gravity history.
    #[serde(skip)]
    pub(in crate::world) gravity: Option<response::gravity::Continuation>,
}

impl LoadingState {
    fn new(config: CaptureConfig) -> Self {
        Self {
            config,
            next_region: 1,
            next_episode: 1,
            regions: BTreeMap::new(),
            bindings: BTreeMap::new(),
            episodes: BTreeMap::new(),
            batches: Vec::new(),
            diagnostics: CaptureDiagnostics::default(),
            records: 0,
            response: None,
            arrival_departures: None,
            flat_departures: None,
            gravity: None,
        }
    }

    pub(super) fn begin_step(&mut self) {
        self.batches.clear();
        self.diagnostics = CaptureDiagnostics::default();
        self.records = 0;
        if let Some(response) = &mut self.response {
            response.begin_step();
        }
    }

    pub(super) fn unbind(&mut self, collider: ColliderId) {
        self.invalidate_arrival_proofs();
        self.bindings.remove(&collider);
    }

    pub(in crate::world) fn invalidate_arrival_proofs(&mut self) {
        self.invalidate_gravity(response::gravity::Invalidation::ExternalEdit);
        if let Some(tracker) = &mut self.arrival_departures {
            tracker.clear();
        }
        if let Some(tracker) = &mut self.flat_departures {
            tracker.clear();
        }
    }

    pub(in crate::world) fn invalidate_gravity(&mut self, reason: response::gravity::Invalidation) {
        if let Some(state) = &mut self.gravity {
            state.invalidate(reason);
        }
    }

    pub(super) fn retire_entity(&mut self, entity: PhysicsId) {
        let tokens: Vec<_> = self
            .regions
            .values()
            .filter(|r| r.body.entity == entity)
            .map(|r| r.token)
            .collect();
        for token in tokens {
            self.retire(token);
        }
    }

    fn live(&self, token: RegionToken) -> bool {
        self.regions
            .get(&token.id)
            .is_some_and(|r| r.token == token)
    }

    fn retire(&mut self, token: RegionToken) -> bool {
        if !self.live(token) {
            return false;
        }
        self.regions.remove(&token.id);
        self.invalidate_arrival_proofs();
        if let Some(response) = &mut self.response {
            response.retire(token);
        }
        self.bindings.retain(|_, source| source.region != token);
        self.episodes
            .retain(|sources, _| sources.iter().all(|s| s.region != token));
        true
    }

    pub(super) fn valid_for(&self, world: &PhysicsWorld) -> bool {
        self.config.valid()
            && self.response.as_ref().is_none_or(|r| r.valid_for(self))
            && self
                .gravity
                .as_ref()
                .is_none_or(|g| self.response.is_some() && g.valid_for(self))
            && self
                .arrival_departures
                .as_ref()
                .is_none_or(|s| self.response.is_some() && s.valid_for(self))
            && self
                .flat_departures
                .as_ref()
                .is_none_or(|s| self.response.is_some() && s.valid_for(self))
            && (self
                .arrival_departures
                .as_ref()
                .is_none_or(|s| s.is_empty())
                || self.flat_departures.as_ref().is_none_or(|s| s.is_empty()))
            && self.next_region > 0
            && self.next_episode > 0
            && self.regions.len() <= self.config.max_regions
            && self.bindings.len() <= self.config.max_bindings
            && self.episodes.len() <= self.config.max_episodes
            && self.regions.iter().all(|(id, r)| {
                r.token.id == *id
                    && r.body.entity == id.entity
                    && r.token.incarnation > 0
                    && r.token.incarnation < self.next_region
                    && world.contains_body(r.body)
            })
            && self.bindings.iter().all(|(id, source)| {
                self.live(source.region)
                    && world.collider_indices.get(id).is_some_and(|i| {
                        world.colliders[*i].parent == self.regions[&source.region.id].body
                    })
            })
            && self.episodes.iter().all(|(sources, e)| {
                sources[0] < sources[1]
                    && sources.iter().all(|s| self.live(s.region))
                    && e.id.0 > 0
                    && e.id.0 < self.next_episode
                    && e.absent_seconds.is_finite()
                    && e.absent_seconds >= 0.0
            })
    }

    fn capture(&mut self, context: &mut SolverContactBatchContext<'_>, external_constraints: bool) {
        let substep = self.diagnostics.batches_seen;
        self.diagnostics.batches_seen = substep.saturating_add(1);
        let result = if substep >= self.config.max_batches_per_step {
            Err(CaptureRejection::StepLimit)
        } else {
            self.prepare(context, substep)
        };
        match result {
            Ok(PreparedBatch {
                batch,
                episodes,
                next_episode,
            }) => {
                self.records += batch.contacts.len() + batch.events.len();
                self.episodes = episodes;
                self.next_episode = next_episode;
                self.batches.push(batch);
                if let Some(mut response) = self.response.take() {
                    response.respond(context, self, external_constraints);
                    self.response = Some(response);
                }
            }
            Err(reason) => {
                self.diagnostics.rejected_batches =
                    self.diagnostics.rejected_batches.saturating_add(1);
                self.diagnostics.last_rejection = Some(reason);
                if let Some(response) = &mut self.response {
                    response.reject(substep, response::ResponseRejection::Capture(reason));
                }
            }
        }
    }

    // Stage a whole batch before committing any episode transition. Overflow
    // cannot admit a traversal-order-dependent prefix or discard earlier events.
    fn prepare(
        &self,
        context: &SolverContactBatchContext<'_>,
        substep: usize,
    ) -> Result<PreparedBatch, CaptureRejection> {
        let mut contacts = Vec::new();
        let mut visits = 0;
        let mut visit = || {
            visits += 1;
            if visits > self.config.max_visits_per_batch {
                Err(CaptureRejection::VisitLimit)
            } else {
                Ok(())
            }
        };
        for pair in context.contact_pairs() {
            visit()?;
            let raw = [
                &context.colliders[pair.collider1],
                &context.colliders[pair.collider2],
            ];
            let Some(ids) =
                decode_collider(raw[0].user_data).zip(decode_collider(raw[1].user_data))
            else {
                continue;
            };
            let Some((a, b)) = self.bindings.get(&ids.0).zip(self.bindings.get(&ids.1)) else {
                continue;
            };
            if a == b {
                continue;
            }
            let order = if a < b { [0, 1] } else { [1, 0] };
            let sources = if a < b { [*a, *b] } else { [*b, *a] };
            let colliders = [ids.0, ids.1];
            for manifold in &pair.manifolds {
                visit()?;
                let subshapes = [
                    raw[0].shape().as_compound().map(|_| manifold.subshape1),
                    raw[1].shape().as_compound().map(|_| manifold.subshape2),
                ];
                let normal =
                    from_rapier(manifold.data.normal) * if order[0] == 0 { 1.0 } else { -1.0 };
                for contact in &manifold.data.solver_contacts {
                    visit()?;
                    if contacts.len() == self.config.max_contacts_per_batch {
                        return Err(CaptureRejection::ContactLimit);
                    }
                    let bodies = [0, 1].map(|i| {
                        body_state(
                            &context.bodies()[raw[order[i]].parent().unwrap()],
                            contact.point,
                            self.regions[&sources[i].region.id].body,
                        )
                    });
                    let relative = bodies[0].point_velocity - bodies[1].point_velocity;
                    let record = ContactRecord {
                        sources,
                        colliders: order.map(|i| colliders[i]),
                        subshapes: order.map(|i| subshapes[i]),
                        bodies,
                        point: from_rapier(contact.point),
                        normal,
                        separation: contact.dist,
                        closing_speed: relative.dot(normal),
                        tangent_speed: relative.dot(Vec2::new(-normal.y, normal.x)),
                        episode: None,
                    };
                    if !finite_record(&record) {
                        return Err(CaptureRejection::NonFinite);
                    }
                    contacts.push(record);
                }
            }
        }
        contacts.sort_by(|a, b| {
            a.sources
                .cmp(&b.sources)
                .then(a.colliders.cmp(&b.colliders))
                .then(a.subshapes.cmp(&b.subshapes))
                .then(a.normal.x.total_cmp(&b.normal.x))
                .then(a.normal.y.total_cmp(&b.normal.y))
                .then(a.point.x.total_cmp(&b.point.x))
                .then(a.point.y.total_cmp(&b.point.y))
                .then(a.separation.total_cmp(&b.separation))
        });
        let mut touching = BTreeMap::<_, f32>::new();
        for record in &contacts {
            if record.separation <= self.config.touch_tolerance {
                touching
                    .entry(record.sources)
                    .and_modify(|speed| *speed = speed.max(record.closing_speed))
                    .or_insert(record.closing_speed);
            }
        }
        let mut episodes = self.episodes.clone();
        let mut events = Vec::new();
        episodes.retain(|sources, episode| {
            if touching.contains_key(sources) {
                episode.absent_seconds = 0.0;
                return true;
            }
            episode.absent_seconds += f64::from(context.dt);
            if episode.absent_seconds < f64::from(self.config.separation_grace_seconds) {
                return true;
            }
            events.push(EpisodeEvent {
                id: episode.id,
                sources: *sources,
                transition: EpisodeTransition::End,
                closing_speed: 0.0,
            });
            false
        });
        let mut next_episode = self.next_episode;
        for (sources, speed) in touching {
            if episodes.contains_key(&sources) {
                continue;
            }
            if episodes.len() == self.config.max_episodes {
                return Err(CaptureRejection::EpisodeLimit);
            }
            let id = EpisodeId(next_episode);
            next_episode = next_episode
                .checked_add(1)
                .ok_or(CaptureRejection::IdentityExhausted)?;
            episodes.insert(
                sources,
                Episode {
                    id,
                    absent_seconds: 0.0,
                },
            );
            events.push(EpisodeEvent {
                id,
                sources,
                closing_speed: speed,
                transition: if speed > self.config.min_approach_speed {
                    EpisodeTransition::Impact
                } else {
                    EpisodeTransition::Support
                },
            });
        }
        events.sort_by_key(|e| e.sources);
        if self.records + contacts.len() + events.len() > self.config.max_records_per_step {
            return Err(CaptureRejection::StepLimit);
        }
        for record in &mut contacts {
            record.episode = episodes.get(&record.sources).map(|e| e.id);
        }
        Ok(PreparedBatch {
            batch: ContactBatch {
                substep,
                duration: context.dt,
                contacts,
                events,
            },
            episodes,
            next_episode,
        })
    }
}

fn body_state(body: &RigidBody, point: Vector, id: BodyId) -> ContactBodyState {
    let dynamic = body.is_dynamic();
    ContactBodyState {
        body: id,
        kind: match body.body_type() {
            RigidBodyType::Dynamic => BodyKind::Dynamic,
            RigidBodyType::Fixed => BodyKind::Fixed,
            RigidBodyType::KinematicPositionBased => BodyKind::KinematicPosition,
            RigidBodyType::KinematicVelocityBased => BodyKind::KinematicVelocity,
        },
        center_of_mass: from_rapier(body.center_of_mass()),
        point_velocity: from_rapier(body.velocity_at_point(point)),
        angular_velocity: body.angvel(),
        inverse_mass: if dynamic {
            from_rapier(body.mass_properties().effective_inv_mass)
        } else {
            Vec2::ZERO
        },
        inverse_inertia: if dynamic {
            body.mass_properties().effective_world_inv_inertia
        } else {
            0.0
        },
        sleeping: body.is_sleeping(),
    }
}

fn finite_record(r: &ContactRecord) -> bool {
    finite_vec2(r.point)
        && finite_vec2(r.normal)
        && r.separation.is_finite()
        && r.closing_speed.is_finite()
        && r.tangent_speed.is_finite()
        && r.bodies.iter().all(|b| {
            finite_vec2(b.center_of_mass)
                && finite_vec2(b.point_velocity)
                && finite_vec2(b.inverse_mass)
                && b.inverse_inertia.is_finite()
                && b.angular_velocity.is_finite()
        })
}

/// Temporary controls supplied only by a fully admitted arrival operation.
pub(super) trait ArrivalControl: Sync {
    fn preview(&self) -> bool;
    fn filter(&self, context: &mut ContactModificationContext);
    fn observe(&self, context: &SolverContactBatchContext);
}

pub(super) struct Hooks<'a> {
    pub report_applied_contact_impulses: bool,
    pub arrival: Option<&'a dyn ArrivalControl>,
    pub flight: Option<&'a std::sync::atomic::AtomicBool>,
    pub drives: surface_drive::DriveHooks<'a>,
    pub loading: Option<Mutex<&'a mut LoadingState>>,
    pub external_constraints: bool,
}

impl PhysicsHooks for Hooks<'_> {
    fn report_applied_contact_impulses(&self) -> bool {
        self.report_applied_contact_impulses
    }

    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        if let Some(arrival) = self.arrival {
            arrival.filter(context);
        }
        if let Some(failed) = self.flight {
            if context
                .solver_contacts
                .iter()
                .any(|c| !c.dist.is_finite() || c.dist <= 0.0)
            {
                failed.store(true, std::sync::atomic::Ordering::Relaxed);
            } else {
                context.solver_contacts.clear();
            }
        }
        self.drives.modify_solver_contacts(context);
    }
    fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
        if self.arrival.is_some_and(ArrivalControl::preview) {
            context.dt = 0.0;
        }
        if let Some(state) = &self.loading {
            state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .capture(context, self.external_constraints);
        }
        if let Some(arrival) = self.arrival {
            arrival.observe(context);
        }
    }
}

impl PhysicsWorld {
    /// Enable once for this world's lifetime. Reconfiguration cannot reset
    /// incarnation counters and accidentally revive stale tokens.
    pub fn enable_loading_capture(&mut self, config: CaptureConfig) -> Result<(), LoadingError> {
        if !config.valid() {
            return Err(LoadingError::InvalidConfig);
        }
        if self.loading.is_some() {
            return Err(LoadingError::AlreadyEnabled);
        }
        self.loading = Some(LoadingState::new(config));
        Ok(())
    }

    pub fn create_loading_region(
        &mut self,
        id: RegionId,
        body: BodyId,
    ) -> Result<RegionToken, LoadingError> {
        if id.entity != body.entity || !self.contains_body(body) {
            return Err(LoadingError::UnknownBody);
        }
        let state = self.loading.as_mut().ok_or(LoadingError::Disabled)?;
        if state.regions.contains_key(&id) {
            return Err(LoadingError::RegionExists);
        }
        if state.regions.len() == state.config.max_regions {
            return Err(LoadingError::Capacity);
        }
        let next = state
            .next_region
            .checked_add(1)
            .ok_or(LoadingError::IdentityExhausted)?;
        let token = RegionToken {
            id,
            incarnation: state.next_region,
        };
        state.next_region = next;
        state.regions.insert(id, RegionEntry { token, body });
        state.invalidate_gravity(response::gravity::Invalidation::ExternalEdit);
        Ok(token)
    }

    /// One collider describes one host-declared physical patch in this slice.
    /// Compound subshape IDs remain in records, but independent subshapes must
    /// use separate colliders/sources until per-feature binding is implemented.
    pub fn bind_loading_collider(
        &mut self,
        collider: ColliderId,
        source: ContactSource,
    ) -> Result<(), LoadingError> {
        let parent = self
            .collider_indices
            .get(&collider)
            .map(|i| self.colliders[*i].parent)
            .ok_or(LoadingError::InvalidBinding)?;
        let state = self.loading.as_mut().ok_or(LoadingError::Disabled)?;
        if !state.live(source.region) {
            return Err(LoadingError::StaleRegion);
        }
        if state.regions[&source.region.id].body != parent {
            return Err(LoadingError::InvalidBinding);
        }
        if state
            .bindings
            .get(&collider)
            .is_some_and(|old| *old != source)
        {
            return Err(LoadingError::InvalidBinding);
        }
        if !state.bindings.contains_key(&collider)
            && state.bindings.len() == state.config.max_bindings
        {
            return Err(LoadingError::Capacity);
        }
        if state.bindings.insert(collider, source).is_none() {
            state.invalidate_arrival_proofs();
        }
        Ok(())
    }

    pub fn retire_loading_region(&mut self, token: RegionToken) -> bool {
        self.loading.as_mut().is_some_and(|s| s.retire(token))
    }
    pub fn loading_region_is_live(&self, token: RegionToken) -> bool {
        self.loading.as_ref().is_some_and(|s| s.live(token))
    }
    /// Last positive step's captures, or empty after a paused/invalid step.
    pub fn loading_batches(&self) -> &[ContactBatch] {
        self.loading.as_ref().map_or(&[], |s| &s.batches)
    }
    pub fn loading_diagnostics(&self) -> CaptureDiagnostics {
        self.loading
            .as_ref()
            .map_or(CaptureDiagnostics::default(), |s| s.diagnostics)
    }
}

#[cfg(test)]
mod tests;
