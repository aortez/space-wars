//! Experimental, opt-in material response for complete frictionless contact
//! batches. Unsupported batches use ordinary physics with explicit refusal;
//! neither capacity nor material impulses are partially committed.
use super::*;
use engine_material::{ContactGeometry, Material, Outcome, Region, Section, shared_impact};
use rapier2d::prelude::{ContactImpulse, SolverFlags};

mod geometry;
use geometry::Footprint;

pub const MAX_RESPONSE_BATCHES: usize = 16;
const MAX_RESPONSE_POINTS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseRejection {
    Capture(CaptureRejection),
    OutputLimit,
    Limit,
    ExternalWork,
    UnsupportedBody,
    UnboundContact,
    MissingMaterial,
    UnsupportedContact,
    AmbiguousPatch,
    UnownedMotion,
    Budget(shared_impact::BudgetError),
    Material,
    Backend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialImpact {
    pub sources: [ContactSource; 2],
    pub episode: EpisodeId,
    pub material_ids: [u64; 2],
    pub geometry: ContactGeometry,
    pub before: [f64; 2],
    pub after: [f64; 2],
    pub outcome: Outcome,
    pub normal_impulse: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppliedResponse {
    pub law: &'static str,
    pub allocation: &'static str,
    /// New impact episodes owning this batch's incoming motion. Existing support
    /// episodes may transmit that work without creating another incoming budget.
    pub roots: Vec<EpisodeId>,
    pub budget: shared_impact::Budget,
    pub coefficients: Vec<f32>,
    /// Signed prescribed work for the full applied impulse, including rebound.
    pub prescribed_work: f64,
    pub impacts: Vec<MaterialImpact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResponseBatch {
    pub substep: usize,
    pub result: Result<AppliedResponse, ResponseRejection>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResponseDiagnostics {
    pub applied_batches: usize,
    pub refused_batches: usize,
    pub last_rejection: Option<ResponseRejection>,
}

/// Numeric parameters only: material names are presentation metadata, not an
/// authoritative identity. Avoid leaking owned snapshot labels into static str.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRegion {
    parameters: [f32; 5],
    brittle: bool,
    volume: f32,
    remaining: f64,
    thickness: f32,
    depth: f32,
}

impl StoredRegion {
    fn region(&self) -> Result<Region, engine_material::MaterialError> {
        let [density, stiffness, strength, toughness, damping] = self.parameters;
        Region::with_remaining(
            Material {
                name: "registered",
                density,
                stiffness,
                strength,
                toughness,
                damping,
                mode: if self.brittle {
                    engine_material::FailureMode::Brittle
                } else {
                    engine_material::FailureMode::Ductile
                },
            },
            self.volume,
            self.remaining,
        )
    }
    fn section(&self) -> Section {
        Section::new(self.thickness, self.depth)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ResponseState {
    law: String,
    allocation: String,
    materials: BTreeMap<RegionToken, StoredRegion>,
    #[serde(skip)]
    batches: Vec<ResponseBatch>,
    #[serde(skip)]
    diagnostics: ResponseDiagnostics,
}

struct Patch {
    sources: [ContactSource; 2],
    bodies: [BodyId; 2],
    episode: EpisodeId,
    point: Vec2,
    normal: Vec2,
    footprint: Footprint,
    // Canonical collider order makes duplicate impulse application deterministic.
    targets: Vec<Target>,
}

#[derive(Clone)]
struct Target {
    ids: [ColliderId; 2],
    point: Vec2,
    request: ContactImpulse,
}

struct PreparedResponse {
    output: AppliedResponse,
    materials: BTreeMap<RegionToken, Region>,
    impulses: Vec<ContactImpulse>,
}

impl ResponseState {
    pub(super) fn begin_step(&mut self) {
        self.batches.clear();
        self.diagnostics = ResponseDiagnostics::default();
    }
    pub(super) fn retire(&mut self, token: RegionToken) {
        self.materials.remove(&token);
    }
    pub(super) fn valid_for(&self, loading: &LoadingState) -> bool {
        self.law == engine_material::LAW_VERSION
            && self.allocation == shared_impact::PULSE_MODEL_VERSION
            && self.materials.len() <= loading.config.max_regions
            && self.materials.iter().all(|(token, r)| {
                loading.live(*token) && r.region().is_ok() && r.section().validate().is_ok()
            })
    }
    pub(super) fn reject(&mut self, substep: usize, reason: ResponseRejection) {
        self.diagnostics.refused_batches = self.diagnostics.refused_batches.saturating_add(1);
        self.diagnostics.last_rejection = Some(reason);
        if self.batches.len() < MAX_RESPONSE_BATCHES {
            self.batches.push(ResponseBatch {
                substep,
                result: Err(reason),
            });
        } else {
            self.diagnostics.last_rejection = Some(ResponseRejection::OutputLimit);
        }
    }
    pub(super) fn respond(
        &mut self,
        context: &mut SolverContactBatchContext,
        loading: &LoadingState,
        external_constraints: bool,
    ) {
        let batch = loading.batches.last().unwrap();
        if !batch
            .events
            .iter()
            .any(|e| e.transition == EpisodeTransition::Impact)
        {
            // Capture cannot create a source episode for an unbound collision.
            // Still expose touching unbound arrivals instead of silently
            // presenting the missing data as a valid zero-damage observation.
            if let Err(reason) = unbound_arrival(context, loading) {
                self.reject(batch.substep, reason);
            }
            return;
        }
        if self.batches.len() == MAX_RESPONSE_BATCHES {
            self.reject(batch.substep, ResponseRejection::OutputLimit);
            return;
        }
        let prepared = self.prepare(context, loading, external_constraints);
        match prepared {
            Ok(prepared) => self.apply(context, batch.substep, prepared),
            Err(reason) => self.reject(batch.substep, reason),
        }
    }

    fn apply(
        &mut self,
        context: &mut SolverContactBatchContext,
        substep: usize,
        prepared: PreparedResponse,
    ) {
        if !context.apply_contact_impulses(&prepared.impulses) {
            self.reject(substep, ResponseRejection::Backend);
            return;
        }
        // Everything below is infallible after backend acceptance.
        for (token, region) in prepared.materials {
            self.materials.get_mut(&token).unwrap().remaining = region.remaining_precise();
        }
        self.batches.push(ResponseBatch {
            substep,
            result: Ok(prepared.output),
        });
        self.diagnostics.applied_batches += 1;
    }

    fn prepare(
        &self,
        context: &SolverContactBatchContext,
        loading: &LoadingState,
        external_constraints: bool,
    ) -> Result<PreparedResponse, ResponseRejection> {
        use ResponseRejection as R;
        if external_constraints {
            return Err(R::ExternalWork);
        }
        if context.bodies().len() > shared_impact::MAX_BODIES {
            return Err(R::Limit);
        }
        for (_, body) in context.bodies().iter() {
            if body.user_force() != Vector::ZERO
                || body.user_torque() != 0.0
                || body.linear_damping() != 0.0
                || body.angular_damping() != 0.0
            {
                return Err(R::ExternalWork);
            }
            if body.is_sleeping()
                || body.is_ccd_enabled()
                || body.dominance_group() != 0
                || body.body_type() == RigidBodyType::KinematicPositionBased
                || (body.is_kinematic() && body.angvel() != 0.0)
            {
                return Err(R::UnsupportedBody);
            }
        }
        let mut patches = BTreeMap::<[ContactSource; 2], Patch>::new();
        let mut bodies = BTreeMap::<BodyId, RigidBodyHandle>::new();
        let mut points = 0;
        let mut visits = 0;
        let mut visit = || {
            visits += 1;
            if visits > loading.config.max_visits_per_batch {
                Err(R::Limit)
            } else {
                Ok(())
            }
        };
        for pair in context.contact_pairs() {
            visit()?;
            let raw = [pair.collider1, pair.collider2];
            for (mi, manifold) in pair.manifolds.iter().enumerate() {
                visit()?;
                for (ci, contact) in manifold.data.solver_contacts.iter().enumerate() {
                    visit()?;
                    points += 1;
                    if points > MAX_RESPONSE_POINTS {
                        return Err(R::Limit);
                    }
                    if contact.friction != 0.0
                        || contact.tangent_velocity != Vector::ZERO
                        || !contact.dist.is_finite()
                        || contact.dist.abs() > 1.0e-5
                        || !manifold
                            .data
                            .solver_flags
                            .contains(SolverFlags::COMPUTE_IMPULSES)
                    {
                        return Err(R::UnsupportedContact);
                    }
                    let ids = raw.map(|h| decode_collider(context.colliders[h].user_data));
                    let [Some(a), Some(b)] = ids else {
                        return Err(R::UnboundContact);
                    };
                    let (Some(a_source), Some(b_source)) =
                        (loading.bindings.get(&a), loading.bindings.get(&b))
                    else {
                        return Err(R::UnboundContact);
                    };
                    if a_source == b_source {
                        return Err(R::AmbiguousPatch);
                    }
                    let reverse = a_source > b_source;
                    let order = if reverse { [1, 0] } else { [0, 1] };
                    let sources = order.map(|i| [*a_source, *b_source][i]);
                    let ids = order.map(|i| [a, b][i]);
                    let ordered = order.map(|i| &context.colliders[raw[i]]);
                    let body_ids = sources.map(|s| loading.regions[&s.region.id].body);
                    for i in 0..2 {
                        if !self.materials.contains_key(&sources[i].region) {
                            return Err(R::MissingMaterial);
                        }
                        let handle = ordered[i].parent().ok_or(R::UnsupportedBody)?;
                        if context.bodies()[handle].user_data != encode_body(body_ids[i]) {
                            return Err(R::UnsupportedBody);
                        }
                        bodies.insert(body_ids[i], handle);
                    }
                    let point = from_rapier(contact.point);
                    let normal =
                        from_rapier(manifold.data.normal) * if reverse { -1.0 } else { 1.0 };
                    let footprint = Footprint::new(ordered, context.bodies(), normal)?;
                    let episode = loading
                        .episodes
                        .get(&sources)
                        .ok_or(R::UnsupportedContact)?
                        .id;
                    if !patches.contains_key(&sources)
                        && patches.len() == shared_impact::MAX_CONTACTS
                    {
                        return Err(R::Limit);
                    }
                    let patch = patches.entry(sources).or_insert_with(|| Patch {
                        sources,
                        bodies: body_ids,
                        episode,
                        point,
                        normal,
                        footprint,
                        targets: Vec::new(),
                    });
                    if (!footprint.is_flat() && patch.point != point)
                        || patch.normal != normal
                        || patch.footprint != footprint
                        || patch.bodies != body_ids
                    {
                        return Err(R::AmbiguousPatch);
                    }
                    patch.targets.push(Target {
                        ids,
                        point,
                        request: ContactImpulse {
                            colliders: raw,
                            manifold: mi,
                            contact: ci,
                            impulse: 0.0,
                        },
                    });
                }
            }
        }
        let patches: Vec<_> = patches.into_values().collect();
        let ordered_bodies: Vec<_> = bodies.into_iter().collect();
        let motions: Vec<_> = ordered_bodies
            .iter()
            .map(|(_, h)| {
                let b = &context.bodies()[*h];
                shared_impact::BodyMotion {
                    center: from_rapier(b.center_of_mass()),
                    linear_velocity: from_rapier(b.linvel()),
                    angular_velocity: b.angvel(),
                    inverse_mass: if b.is_dynamic() {
                        from_rapier(b.mass_properties().effective_inv_mass)
                    } else {
                        Vec2::ZERO
                    },
                    inverse_inertia: if b.is_dynamic() {
                        b.mass_properties().effective_world_inv_inertia
                    } else {
                        0.0
                    },
                }
            })
            .collect();
        let mut materials = BTreeMap::new();
        let mut geometries = Vec::new();
        let mut contacts = Vec::new();
        for (i, patch) in patches.iter().enumerate() {
            for source in patch.sources {
                materials.entry(source.region).or_insert(
                    self.materials[&source.region]
                        .region()
                        .map_err(|_| R::Material)?,
                );
            }
            let [a, b] = patch.sources.map(|s| &self.materials[&s.region]);
            let (point, width) = patch.footprint.measure(patch)?;
            let geometry = ContactGeometry::new(width, a.section(), b.section());
            geometry.validate().map_err(|_| R::Material)?;
            let ka = geometry.a.contact_stiffness(
                materials[&patch.sources[0].region].material().stiffness,
                geometry.area(),
            );
            let kb = geometry.b.contact_stiffness(
                materials[&patch.sources[1].region].material().stiffness,
                geometry.area(),
            );
            contacts.push(shared_impact::Contact {
                id: i as u64,
                bodies: patch
                    .bodies
                    .map(|b| ordered_bodies.iter().position(|(id, _)| *id == b).unwrap()),
                point,
                normal: patch.normal,
                stiffness: 1.0 / (1.0 / ka + 1.0 / kb),
            });
            geometries.push(geometry);
        }
        let budget = shared_impact::allocate_pulse(&motions, &contacts).map_err(R::Budget)?;
        let starts: Vec<_> = loading
            .batches
            .last()
            .unwrap()
            .events
            .iter()
            .filter(|e| e.transition == EpisodeTransition::Impact)
            .collect();
        for (index, component) in budget.components.iter().enumerate() {
            if component.available > 0.0
                && !budget.contacts.iter().any(|c| {
                    c.component == index
                        && starts
                            .iter()
                            .any(|e| e.sources == patches[c.id as usize].sources)
                })
            {
                return Err(R::UnownedMotion);
            }
        }
        let mut recoverable = vec![0.0_f64; budget.components.len()];
        let mut impacts = Vec::new();
        for c in &budget.contacts {
            let Some(input) = c.material_input().map_err(R::Budget)? else {
                continue;
            };
            let index = c.id as usize;
            let patch = &patches[index];
            let [a, b] = patch.sources.map(|s| s.region);
            let before = [
                materials[&a].remaining_precise(),
                materials[&b].remaining_precise(),
            ];
            let material_ids = [
                materials[&a].material().identity(),
                materials[&b].material().identity(),
            ];
            let mut outcome =
                engine_material::impact(&materials[&a], &materials[&b], geometries[index], input)
                    .map_err(|_| R::Material)?;
            outcome
                .ledger
                .resolve_excess(0.0)
                .map_err(|_| R::Material)?;
            materials
                .get_mut(&a)
                .unwrap()
                .absorb(outcome.consumed[0])
                .map_err(|_| R::Material)?;
            materials
                .get_mut(&b)
                .unwrap()
                .absorb(outcome.consumed[1])
                .map_err(|_| R::Material)?;
            recoverable[c.component] += outcome
                .ledger
                .recoverable
                .iter()
                .map(|v| f64::from(*v))
                .sum::<f64>();
            impacts.push(MaterialImpact {
                sources: patch.sources,
                episode: patch.episode,
                material_ids,
                geometry: geometries[index],
                before,
                after: [
                    materials[&a].remaining_precise(),
                    materials[&b].remaining_precise(),
                ],
                outcome,
                normal_impulse: 0.0,
            });
        }
        let mut coefficients = Vec::new();
        for (component, recovered) in budget.components.iter().zip(recoverable) {
            if recovered > component.available * (1.0 + 1.0e-6) {
                return Err(R::Material);
            }
            coefficients.push(if component.available > 0.0 {
                libm::sqrt((recovered / component.available).min(1.0)) as f32
            } else {
                0.0
            });
        }
        let mut impulses = Vec::new();
        for c in &budget.contacts {
            let patch = &patches[c.id as usize];
            let impulse =
                ((1.0 + f64::from(coefficients[c.component])) * c.compression_impulse) as f32;
            if let Some(record) = impacts.iter_mut().find(|r| r.sources == patch.sources) {
                record.normal_impulse = impulse;
            }
            let mut targets = patch.targets.clone();
            targets.sort_by_key(|t| (t.ids, t.request.manifold, t.request.contact));
            let count = targets.len() as f32;
            impulses.extend(targets.into_iter().map(|mut t| {
                t.request.impulse = impulse / count;
                t.request
            }));
        }
        let roots = loading
            .batches
            .last()
            .unwrap()
            .events
            .iter()
            .filter(|e| e.transition == EpisodeTransition::Impact)
            .map(|e| e.id)
            .collect();
        Ok(PreparedResponse {
            output: AppliedResponse {
                law: engine_material::LAW_VERSION,
                allocation: shared_impact::PULSE_MODEL_VERSION,
                roots,
                prescribed_work: budget
                    .components
                    .iter()
                    .zip(&coefficients)
                    .map(|(b, e)| (1.0 + f64::from(*e)) * b.prescribed_work)
                    .sum(),
                budget,
                coefficients,
                impacts,
            },
            materials,
            impulses,
        })
    }
}

fn unbound_arrival(
    context: &SolverContactBatchContext,
    loading: &LoadingState,
) -> Result<(), ResponseRejection> {
    let mut visits = 0;
    let mut visit = || {
        visits += 1;
        if visits > loading.config.max_visits_per_batch {
            Err(ResponseRejection::Limit)
        } else {
            Ok(())
        }
    };
    for pair in context.contact_pairs() {
        visit()?;
        let raw = [pair.collider1, pair.collider2];
        if raw.iter().all(|h| {
            decode_collider(context.colliders[*h].user_data)
                .is_some_and(|id| loading.bindings.contains_key(&id))
        }) {
            continue;
        }
        for manifold in &pair.manifolds {
            visit()?;
            for contact in &manifold.data.solver_contacts {
                visit()?;
                let velocities = raw.map(|h| {
                    context.colliders[h].parent().map_or(Vector::ZERO, |b| {
                        context.bodies()[b].velocity_at_point(contact.point)
                    })
                });
                if contact.dist.abs() <= loading.config.touch_tolerance
                    && (velocities[0] - velocities[1]).dot(manifold.data.normal)
                        > loading.config.min_approach_speed
                {
                    return Err(ResponseRejection::UnboundContact);
                }
            }
        }
    }
    Ok(())
}

impl LoadingState {
    pub(in crate::world) fn response_enabled(&self) -> bool {
        self.response.is_some()
    }
    pub(in crate::world) fn serialize_response(
        &self,
        bytes: &mut Vec<u8>,
    ) -> Result<(), PhysicsWorldError> {
        if let Some(response) = &self.response {
            bincode::serialize_into(bytes, response)
                .map_err(|e| PhysicsWorldError(e.to_string()))?;
        }
        Ok(())
    }
    pub(in crate::world) fn deserialize_response(
        &mut self,
        reader: &mut std::io::Cursor<&[u8]>,
    ) -> Result<(), PhysicsWorldError> {
        self.response =
            Some(bincode::deserialize_from(reader).map_err(|e| PhysicsWorldError(e.to_string()))?);
        Ok(())
    }
}

impl PhysicsWorld {
    /// Opt into experimental pulse allocation and blocking failure dissipation.
    /// Requires capture. Refused batches use ordinary physics and report no
    /// material charges; callers must inspect diagnostics before relying on damage.
    pub fn enable_material_response(&mut self) -> Result<(), LoadingError> {
        let loading = self.loading.as_mut().ok_or(LoadingError::Disabled)?;
        if loading.response.is_some() {
            return Err(LoadingError::AlreadyEnabled);
        }
        loading.response = Some(ResponseState {
            law: engine_material::LAW_VERSION.into(),
            allocation: shared_impact::PULSE_MODEL_VERSION.into(),
            materials: BTreeMap::new(),
            batches: Vec::new(),
            diagnostics: ResponseDiagnostics::default(),
        });
        Ok(())
    }

    /// Register persistent capacity and host-specified material thickness/depth.
    /// Depth belongs to the region, never a duplicate collider's partition size.
    pub fn register_loading_material(
        &mut self,
        token: RegionToken,
        region: Region,
        section: Section,
    ) -> Result<(), LoadingError> {
        section
            .validate()
            .map_err(|_| LoadingError::InvalidMaterial)?;
        let loading = self.loading.as_mut().ok_or(LoadingError::Disabled)?;
        if !loading.live(token) {
            return Err(LoadingError::StaleRegion);
        }
        let response = loading
            .response
            .as_mut()
            .ok_or(LoadingError::ResponseDisabled)?;
        if response.materials.contains_key(&token) {
            return Err(LoadingError::MaterialExists);
        }
        let m = region.material();
        response.materials.insert(
            token,
            StoredRegion {
                parameters: [m.density, m.stiffness, m.strength, m.toughness, m.damping],
                brittle: m.mode == engine_material::FailureMode::Brittle,
                volume: region.volume(),
                remaining: region.remaining_precise(),
                thickness: section.thickness,
                depth: section.depth,
            },
        );
        Ok(())
    }

    pub fn loading_material_remaining(&self, token: RegionToken) -> Option<f64> {
        self.loading
            .as_ref()?
            .response
            .as_ref()?
            .materials
            .get(&token)
            .map(|r| r.remaining)
    }
    pub fn material_response_batches(&self) -> &[ResponseBatch] {
        self.loading
            .as_ref()
            .and_then(|l| l.response.as_ref())
            .map_or(&[], |r| r.batches.as_slice())
    }
    pub fn material_response_diagnostics(&self) -> ResponseDiagnostics {
        self.loading
            .as_ref()
            .and_then(|l| l.response.as_ref())
            .map_or(ResponseDiagnostics::default(), |r| r.diagnostics)
    }
}

#[cfg(test)]
mod tests;
