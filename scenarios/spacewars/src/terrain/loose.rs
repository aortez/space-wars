//! Adapter between shared release mechanics and Spacewars' existing lifecycle.
use super::*;
use engine_rapier::terrain::{MaterialEdit, PreparedRelease, ReleaseError, TerrainBodyMut};

pub(crate) fn render_loose(
    frame: &mut RenderFrame,
    state: &SpacewarsState,
    view: Option<engine_common::RenderRect>,
) {
    for grain in state.terrain.loose.iter().flat_map(LooseTerrain::iter) {
        let Some(motion) = state.physics.world.motion(grain.body()) else {
            continue;
        };
        let radius = grain.radius();
        if view.is_some_and(|v| {
            motion.position.x + radius < v.min.x
                || motion.position.x - radius > v.max.x
                || motion.position.y + radius < v.min.y
                || motion.position.y - radius > v.max.y
        }) {
            continue;
        }
        let color = if grain.cell().material == ORE {
            RenderColor::rgb(0.53, 0.49, 0.25)
        } else {
            RenderColor::rgb(0.22, 0.36, 0.45)
        };
        let primitive = if let Some(vertices) = grain.shape().vertices(radius) {
            RenderPrimitive::Polygon(RenderPolygon {
                points: vertices
                    .into_iter()
                    .map(|p| render_point(motion.position + p.rotate_radians(motion.angle)))
                    .collect(),
                fill: Some(Fill::new(color)),
                stroke: None,
            })
        } else {
            RenderPrimitive::Circle(RenderCircle {
                center: render_point(motion.position),
                radius,
                fill: Some(Fill::new(color)),
                stroke: None,
            })
        };
        frame.push_primitive(PLANET_LAYER + 1, primitive);
    }
}

impl SpacewarsState {
    /// Enable before play; reconfiguration cannot orphan already released dirt.
    pub fn enable_loose_terrain(&mut self, config: LooseTerrainConfig) -> Result<(), TerrainError> {
        if self.terrain.loose.is_some() {
            return Err(TerrainError("loose terrain already enabled"));
        }
        self.terrain.loose = Some(LooseTerrain::new(config)?);
        Ok(())
    }

    pub fn loose_terrain(&self) -> Option<&LooseTerrain> {
        self.terrain.loose.as_ref()
    }
}

pub(super) enum ContactHit {
    Field(PendingEdit),
    Loose(PhysicsId, RadialImpulse),
}

pub(super) fn contact_hit(
    state: &SpacewarsState,
    body: PhysicsId,
    contact: ContactPoint,
    radius: u32,
    work: u8,
) -> Option<ContactHit> {
    let blast = RadialImpulse {
        center: contact.position,
        radius: radius as f32 * 2.0,
        speed: 35.0,
    };
    if let Some(field) = state.terrain.field(body) {
        Some(ContactHit::Field(PendingEdit {
            body,
            edit: impact_edit(field, state.terrain.surface, contact, radius, work)?,
            blast: state.terrain.loose.as_ref().map(|_| blast),
        }))
    } else if state
        .terrain
        .loose
        .as_ref()
        .is_some_and(|pool| pool.iter().any(|g| g.id() == body))
    {
        Some(ContactHit::Loose(body, blast))
    } else {
        None
    }
}

pub(super) fn queue_hit(terrain: &mut TerrainState, hit: ContactHit) {
    match hit {
        ContactHit::Field(edit) => terrain.pending.push(edit),
        ContactHit::Loose(body, blast) => terrain.pending_blasts.push((body, blast)),
    }
}

fn world_blast(
    state: &SpacewarsState,
    body: PhysicsId,
    blast: RadialImpulse,
) -> Option<RadialImpulse> {
    let motion = state.physics.world.motion(physics::primary_body(body))?;
    Some(RadialImpulse {
        center: motion.position + blast.center.rotate_radians(motion.angle),
        ..blast
    })
}

pub(super) fn commit_releases(
    state: &mut SpacewarsState,
    pending: Vec<PendingEdit>,
) -> Vec<PendingEdit> {
    if state.terrain.loose.is_none() {
        return pending;
    }
    let mut batches = BTreeMap::<PhysicsId, Vec<PendingEdit>>::new();
    for edit in pending {
        batches.entry(edit.body).or_default().push(edit);
    }
    let queued = std::mem::take(&mut state.terrain.pending_blasts);
    let mut blasts = queued
        .into_iter()
        .filter_map(|(id, blast)| world_blast(state, id, blast))
        .collect::<Vec<_>>();
    let mut ordinary = Vec::new();
    for (id, edits) in batches {
        if !edits.iter().any(|e| e.blast.is_some()) {
            ordinary.extend(edits);
            continue;
        }
        let Some(field) = state.terrain.field(id) else {
            continue;
        };
        // Preserve contact coordinates and damage/mining order, then split once.
        let plan = PreparedRelease::from_edits(
            field,
            edits.iter().map(|p| {
                if p.blast.is_some() {
                    MaterialEdit::Release(p.edit)
                } else {
                    MaterialEdit::Discard(p.edit)
                }
            }),
        )
        .expect("validated queued material edits");
        let discarded = plan.discarded_cells();
        let pending_blasts = edits
            .iter()
            .filter_map(|e| e.blast.and_then(|b| world_blast(state, id, b)))
            .collect::<Vec<_>>();
        let terrain = &mut state.terrain;
        let pool = terrain.loose.as_mut().unwrap();
        let previous = pool.len();
        let source = if let Some(fragment) = terrain.fragments.get_mut(&id.value()) {
            TerrainBodyMut {
                terrain: &mut fragment.terrain,
                geometry: &mut fragment.geometry,
                assembly: &mut fragment.assembly,
            }
        } else {
            let planet = terrain
                .planets
                .get_mut(&physics::planet_index(id).unwrap())
                .unwrap();
            TerrainBodyMut {
                terrain: &mut planet.field,
                geometry: &mut planet.geometry,
                assembly: &mut planet.assembly,
            }
        };
        let result = pool.commit(
            &mut state.physics.world,
            source,
            plan,
            &mut terrain.next_fragment,
        );
        let committed = match result {
            Ok(result) => result,
            Err(ReleaseError::Capacity) => {
                terrain.rejected_releases +=
                    edits.iter().filter(|e| e.blast.is_some()).count() as u64;
                // Mining retains its usual behavior even when impacts cannot fit.
                ordinary.extend(edits.into_iter().filter(|e| e.blast.is_none()));
                continue;
            }
            Err(e) => panic!("valid material release failed: {e}"),
        };
        terrain.removed_cells += discarded;
        register_release(state, id, previous, committed);
        blasts.extend(pending_blasts);
    }
    // Snapshot every source motion before applying any blast. All material and
    // actors then interact in the world's single ordinary solver step.
    let bodies = state
        .physics
        .terrain_fragments
        .iter()
        .map(|&id| physics::primary_body(PhysicsId::new(id)))
        .collect::<Vec<_>>();
    for blast in blasts {
        blast.apply(&mut state.physics.world, bodies.iter().copied());
    }
    ordinary
}

// The same registry, query-cache and support boundary handles both causes.
fn register_release(
    state: &mut SpacewarsState,
    id: PhysicsId,
    previous: usize,
    committed: engine_rapier::terrain::ReleaseCommit,
) {
    let terrain = &mut state.terrain;
    let pool = terrain.loose.as_ref().unwrap();
    state.physics.material_queries_dirty |=
        committed.rebuilt_chunks > 0 || committed.new_grains > 0 || !committed.fragments.is_empty();
    state
        .physics
        .terrain_fragments
        .extend(pool.iter().skip(previous).map(|g| g.id().value()));
    if let Some(fragment) = terrain.fragments.get_mut(&id.value()) {
        fragment.edited_chunks = committed.dirty_chunks;
        fragment.hash = fragment.terrain.hash();
        if fragment.geometry.shape_count() == 0 {
            state.physics.world.remove_entity(id);
            state.physics.terrain_fragments.remove(&id.value());
            terrain.fragments.remove(&id.value());
        }
    } else {
        let planet = terrain
            .planets
            .get_mut(&physics::planet_index(id).unwrap())
            .unwrap();
        planet.hash = planet.field.hash();
        planet.supported = planet.footing.iter().all(|c| {
            planet
                .field
                .cell(*c)
                .is_some_and(|c| c.material != MaterialId::VOID)
        });
    }
    for fragment in committed.fragments {
        let id = fragment.id().value();
        state.physics.terrain_fragments.insert(id);
        terrain.fragments.insert(id, fragment);
    }
}

pub(super) fn commit_slumping(state: &mut SpacewarsState) {
    if !state
        .terrain
        .loose
        .as_ref()
        .is_some_and(|pool| pool.config().slumping.is_some())
    {
        return;
    }
    // Sample the same point/spherical law as loose-grain gravity, at completed
    // body poses. The solver retains its own scratch; no force is applied here.
    let mut sources = Vec::new();
    if let Some(sun) = state.sun {
        sources.push(GravityParticipant::direct_source(
            GravityId::new(0),
            sun.position,
            sun.mass,
        ));
    }
    for (index, planet) in state.planets.iter().enumerate() {
        let position = state
            .physics
            .world
            .motion(state.physics.planet_body(index))
            .map_or(planet.position, |m| m.position);
        let id = GravityId::new(index as u64 + 1);
        sources.push(if state.terrain.planets.contains_key(&index) {
            GravityParticipant::spherical_source(id, position, planet.mass, planet.radius)
        } else {
            GravityParticipant::direct_source(id, position, planet.mass)
        });
    }
    sources.push(GravityParticipant::target(
        GravityId::new(u64::MAX),
        Vec2::ZERO,
        1.0,
    ));
    let mut gravity = GravitySolver::new();
    let capacity = 64usize.saturating_sub(state.terrain.fragments.len());
    let terrain = &mut state.terrain;
    let pool = terrain.loose.as_mut().unwrap();
    let previous = pool.len();
    let fields = terrain
        .planets
        .values_mut()
        .map(|p| TerrainBodyMut {
            terrain: &mut p.field,
            geometry: &mut p.geometry,
            assembly: &mut p.assembly,
        })
        .chain(terrain.fragments.values_mut().map(|f| TerrainBodyMut {
            terrain: &mut f.terrain,
            geometry: &mut f.geometry,
            assembly: &mut f.assembly,
        }));
    let committed = pool
        .slump(
            &mut state.physics.world,
            fields,
            |_, position| {
                sources.last_mut().unwrap().position = position;
                gravity
                    .solve(
                        &sources,
                        GravityConfig {
                            backend: GravityBackend::BarnesHut { theta: 0.7 },
                            softening: GRAVITY_SOFTENING,
                            interaction_scale: GRAVITY,
                        },
                    )
                    .expect("valid slumping gravity")
                    .iter()
                    .find(|o| o.id == GravityId::new(u64::MAX))
                    .unwrap()
                    .velocity_delta
            },
            &mut terrain.next_fragment,
            capacity,
            1.0 / 60.0,
        )
        .expect("valid Spacewars slumping");
    if let Some(committed) = committed {
        register_release(state, committed.body.entity, previous, committed.release);
    }
}

pub(super) fn commit_deposits(state: &mut SpacewarsState) {
    let terrain = &mut state.terrain;
    let Some(pool) = terrain.loose.as_mut() else {
        return;
    };
    let fields = terrain
        .planets
        .values_mut()
        .map(|planet| TerrainBodyMut {
            terrain: &mut planet.field,
            geometry: &mut planet.geometry,
            assembly: &mut planet.assembly,
        })
        .chain(
            terrain
                .fragments
                .values_mut()
                .map(|fragment| TerrainBodyMut {
                    terrain: &mut fragment.terrain,
                    geometry: &mut fragment.geometry,
                    assembly: &mut fragment.assembly,
                }),
        );
    // Spacewars' material boundary runs once per 60 Hz simulation tick. Pending
    // damage/blasts have already resolved, so a new impulse resets quiet time.
    let commits = pool
        .settle(&mut state.physics.world, fields, 1.0 / 60.0)
        .expect("valid Spacewars deposition");
    for commit in commits {
        state.physics.material_queries_dirty = true;
        for id in commit.retired_grains {
            state.physics.terrain_fragments.remove(&id.value());
        }
        if let Some(fragment) = terrain.fragments.get_mut(&commit.body.entity.value()) {
            fragment.hash = fragment.terrain.hash();
            fragment.edited_chunks = commit.dirty_chunks;
        } else if let Some(planet) =
            physics::planet_index(commit.body.entity).and_then(|i| terrain.planets.get_mut(&i))
        {
            planet.hash = planet.field.hash();
            planet.supported = planet.footing.iter().all(|c| {
                planet
                    .field
                    .cell(*c)
                    .is_some_and(|c| c.material != MaterialId::VOID)
            });
        }
    }
}

pub(super) fn observe(state: &SpacewarsState, payload: &mut Vec<u8>) {
    let Some(pool) = &state.terrain.loose else {
        return;
    };
    payload.extend(b"loose-v2");
    payload.extend(pool.settling_hash().to_le_bytes());
    payload.extend((pool.config().max_grains as u64).to_le_bytes());
    payload.extend([pool.config().shape as u8]);
    payload.extend(pool.config().friction.to_le_bytes());
    payload.extend(pool.config().restitution.to_le_bytes());
    payload.extend(state.terrain.rejected_releases.to_le_bytes());
    payload.extend((pool.len() as u64).to_le_bytes());
    for grain in pool.iter() {
        payload.extend(grain.id().value().to_le_bytes());
        payload.extend(grain.cell().material.0.to_le_bytes());
        payload.push(grain.cell().durability);
        payload.extend(grain.cell_size().to_le_bytes());
        let motion = state
            .physics
            .world
            .motion(grain.body())
            .expect("loose body");
        for v in [
            motion.position.x,
            motion.position.y,
            motion.angle,
            motion.linear_velocity.x,
            motion.linear_velocity.y,
            motion.angular_velocity,
        ] {
            payload.extend(v.to_le_bytes());
        }
    }
    let blasts = state
        .terrain
        .pending
        .iter()
        .filter_map(|p| p.blast.map(|b| (p.body, b)))
        .chain(state.terrain.pending_blasts.iter().copied())
        .collect::<Vec<_>>();
    payload.extend((blasts.len() as u64).to_le_bytes());
    for (id, blast) in blasts {
        payload.extend(id.value().to_le_bytes());
        for v in [blast.center.x, blast.center.y, blast.radius, blast.speed] {
            payload.extend(v.to_le_bytes());
        }
    }
}
