//! Material planets in the ordinary Spacewars world. Gameplay queues local
//! edits; the next lifecycle boundary reconciles material, shapes, and services.

use std::collections::BTreeMap;

use engine_core::Vec2;
use engine_rapier::{
    terrain::{TerrainAssembly, TerrainFragment},
    world::{BodyKind, BodySpec, ContactPoint, PhysicsId},
};
use engine_terrain::{
    Brush, CellCoord, EditMode, Material, MaterialId, Terrain, TerrainEdit, TerrainError,
    TerrainGeometry, TerrainSurface,
};

use super::*;

mod diagnostics;
pub use diagnostics::TerrainDiagnostics;
mod motion_trace;
pub(super) use motion_trace::capture_motion;
pub use motion_trace::{
    TerrainMotionAnomaly, TerrainMotionBody, TerrainMotionFrame, TerrainMotionPeaks,
    TerrainMotionStage,
};

pub(super) const FRAGMENT_ID_BASE: u64 = 1 << 50;
const ROCK: MaterialId = MaterialId(1);
const ORE: MaterialId = MaterialId(2);
const CANNON_WORK: u8 = 160;
const CANNON_RADIUS: u32 = 6;
const MAX_CANNON_HITS: usize = 4;
const FIXTURE_ACTION: u32 = 100;

#[derive(Debug, Clone)]
pub(super) struct PlanetTerrain {
    pub field: Terrain,
    pub geometry: TerrainGeometry,
    pub assembly: TerrainAssembly,
    pub hash: u64,
    pub supported: bool,
    footing: [CellCoord; 3],
}

#[derive(Debug, Clone, Copy)]
struct PendingEdit {
    body: PhysicsId,
    edit: TerrainEdit,
}

#[derive(Debug, Clone)]
pub(super) struct TerrainState {
    pub surface: TerrainSurface,
    pub planets: BTreeMap<usize, PlanetTerrain>,
    pub fragments: BTreeMap<u64, TerrainFragment>,
    pending: Vec<PendingEdit>,
    next_fragment: u64,
    pub fixture: bool,
    pub legacy_services: bool,
    pub follow_ship: bool,
    previous_controls: [bool; 2],
    pub removed_cells: u64,
    pub cannon_hits: u64,
    pub budget_skips: u64,
    motion_trace: Option<motion_trace::MotionTrace>,
}

impl Default for TerrainState {
    fn default() -> Self {
        Self {
            surface: TerrainSurface::Blocks,
            planets: BTreeMap::new(),
            fragments: BTreeMap::new(),
            pending: Vec::new(),
            next_fragment: FRAGMENT_ID_BASE,
            fixture: false,
            legacy_services: true,
            follow_ship: false,
            previous_controls: [false; 2],
            removed_cells: 0,
            cannon_hits: 0,
            budget_skips: 0,
            motion_trace: None,
        }
    }
}

impl TerrainState {
    pub fn supported(&self, planet: usize) -> bool {
        self.legacy_services
            && self
                .planets
                .get(&planet)
                .is_none_or(|terrain| terrain.supported)
    }

    pub(super) fn field(&self, body: PhysicsId) -> Option<&Terrain> {
        if body.value() >= FRAGMENT_ID_BASE {
            self.fragments.get(&body.value()).map(|f| &f.terrain)
        } else {
            self.planets
                .get(&physics::planet_index(body)?)
                .map(|p| &p.field)
        }
    }

    fn field_mut(&mut self, body: PhysicsId) -> Option<&mut Terrain> {
        if body.value() >= FRAGMENT_ID_BASE {
            self.fragments
                .get_mut(&body.value())
                .map(|f| &mut f.terrain)
        } else {
            self.planets
                .get_mut(&physics::planet_index(body)?)
                .map(|p| &mut p.field)
        }
    }
}

impl SpacewarsState {
    /// Opt a planet into material terrain before play. Its stable physics ID,
    /// orbital identity, ownership, and external service sensor are retained.
    /// Its gravity becomes a bounded spherical field for every recipient.
    pub fn enable_planet_terrain(&mut self, index: usize) -> Result<(), TerrainError> {
        if self.terrain.planets.contains_key(&index) {
            return Ok(());
        }
        let planet = *self
            .planets
            .get(index)
            .ok_or(TerrainError("unknown planet"))?;
        let field = generate_field(planet.radius, self.seed ^ index as u64)?;
        let field = if self.terrain.surface == TerrainSurface::Interpolated {
            field
                .with_surface_distances(|p| planet.radius * BODY_BOUNDS_RADIUS_SCALE - p.length())?
        } else {
            field
        };
        let geometry = TerrainGeometry::with_surface(&field, self.terrain.surface);
        let radius = planet.radius * BODY_BOUNDS_RADIUS_SCALE;
        let footing =
            [-3.0, 0.0, 3.0].map(|y| field.local_to_cell(Vec2::new(radius - 1.5, y)).unwrap());
        self.physics
            .world
            .remove_entity(physics::planet_entity(index));
        let assembly = TerrainAssembly::insert(
            &mut self.physics.world,
            physics::planet_entity(index),
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position: planet.position,
                angle: planet.wrapper_angle,
                can_sleep: false,
                ..BodySpec::default()
            },
            &field,
            &geometry,
            engine_rapier::terrain::TerrainSpec {
                surface: self.terrain.surface,
                ..physics::terrain_spec()
            },
        )
        .expect("generated terrain has valid geometry and an available entity");
        self.physics.material_planets.insert(index);
        self.physics.material_queries_dirty = true;
        if self.terrain.legacy_services {
            self.physics.add_spaceport_sensor(index, &planet);
        } else {
            self.physics.disable_spaceport(index);
        }
        self.terrain.planets.insert(
            index,
            PlanetTerrain {
                hash: field.hash(),
                field,
                geometry,
                assembly,
                supported: true,
                footing,
            },
        );
        Ok(())
    }

    pub fn planet_terrain(&self, planet: usize) -> Option<&Terrain> {
        self.terrain
            .planets
            .get(&planet)
            .map(|terrain| &terrain.field)
    }

    pub fn terrain_fragments(&self) -> impl Iterator<Item = &TerrainFragment> {
        self.terrain.fragments.values()
    }

    pub fn terrain_fragment_motion(&self, id: PhysicsId) -> Option<BodyMotion> {
        let fragment = self.terrain.fragments.get(&id.value())?;
        self.physics.world.motion(fragment.assembly.body())
    }

    pub fn terrain_removed_cells(&self) -> u64 {
        self.terrain.removed_cells
    }
    pub fn terrain_cannon_hits(&self) -> u64 {
        self.terrain.cannon_hits
    }
    pub fn planet_base_supported(&self, planet: usize) -> bool {
        self.terrain.supported(planet)
    }
    pub fn is_terrain_fixture(&self) -> bool {
        self.terrain.fixture
    }

    pub fn queue_planet_edit(
        &mut self,
        planet: usize,
        edit: TerrainEdit,
    ) -> Result<(), TerrainError> {
        let field = self
            .planet_terrain(planet)
            .ok_or(TerrainError("planet has no material field"))?;
        let _ = field.brush_cells(edit.brush)?;
        self.terrain.pending.push(PendingEdit {
            body: physics::planet_entity(planet),
            edit,
        });
        Ok(())
    }

    pub(super) fn queue_material_edit(
        &mut self,
        body: PhysicsId,
        edit: TerrainEdit,
    ) -> Result<(), TerrainError> {
        let field = self
            .terrain
            .field(body)
            .ok_or(TerrainError("unknown material body"))?;
        let _ = field.brush_cells(edit.brush)?;
        self.terrain.pending.push(PendingEdit { body, edit });
        Ok(())
    }
}

fn generate_field(radius: f32, seed: u64) -> Result<Terrain, TerrainError> {
    if !radius.is_finite() || !(5.0..=150.0).contains(&radius) {
        return Err(TerrainError("terrain planet radius must be 5..150"));
    }
    let r = radius.ceil() as i32;
    let limit = (radius * BODY_BOUNDS_RADIUS_SCALE).powi(2);
    Terrain::generate(
        (r * 2 + 1) as u32,
        (r * 2 + 1) as u32,
        1.0,
        vec![
            Material {
                id: ROCK,
                hardness: 100,
            },
            Material {
                id: ORE,
                hardness: 180,
            },
        ],
        |coord| {
            let x = coord.x - r;
            let y = coord.y - r;
            if (x * x + y * y) as f32 > limit {
                return MaterialId::VOID;
            }
            let mut h = seed
                ^ (coord.x as u64 / 8).wrapping_mul(0x9e3779b97f4a7c15)
                ^ (coord.y as u64 / 8).wrapping_mul(0xbf58476d1ce4e5b9);
            h = (h ^ (h >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            if (h ^ (h >> 27)).is_multiple_of(5) {
                ORE
            } else {
                ROCK
            }
        },
    )
}

/// Complete every edit against its sampled field before any body is cropped or
/// split. Weapon contacts from this tick never mutate the active solver world.
pub(super) fn commit(state: &mut SpacewarsState) {
    let mut changed = BTreeMap::<PhysicsId, bool>::new();
    for pending in std::mem::take(&mut state.terrain.pending) {
        let Some(field) = state.terrain.field_mut(pending.body) else {
            continue;
        };
        let result = field
            .apply(pending.edit)
            .expect("validated queued terrain edit");
        if result.changed_cells == 0 {
            continue;
        }
        let removed = result
            .removed
            .iter()
            .map(|m| u64::from(m.cells))
            .sum::<u64>();
        state.terrain.removed_cells += removed;
        *changed.entry(pending.body).or_default() |= removed != 0;
    }
    for (id, removed) in changed {
        let body = physics::primary_body(id);
        let motion = state.physics.world.motion(body).expect("edited body");
        let center = state
            .physics
            .world
            .center_of_mass(body)
            .expect("edited body center");
        let detached = if removed {
            state
                .terrain
                .field_mut(id)
                .unwrap()
                .detach_disconnected()
                .expect("valid field")
        } else {
            Vec::new()
        };
        if let Some(fragment) = state.terrain.fragments.get_mut(&id.value()) {
            fragment.edited_chunks = fragment.geometry.refresh(&fragment.terrain);
            let rebuilt = fragment
                .assembly
                .synchronize(
                    &mut state.physics.world,
                    &fragment.terrain,
                    &fragment.geometry,
                )
                .expect("valid fragment cache");
            state.physics.material_queries_dirty |= rebuilt > 0;
            fragment.hash = fragment.terrain.hash();
            if fragment.geometry.shape_count() == 0 {
                state.physics.world.remove_entity(id);
                state.physics.terrain_fragments.remove(&id.value());
                state.terrain.fragments.remove(&id.value());
            }
        } else if let Some(planet) =
            physics::planet_index(id).and_then(|i| state.terrain.planets.get_mut(&i))
        {
            planet.geometry.refresh(&planet.field);
            let rebuilt = planet
                .assembly
                .synchronize(&mut state.physics.world, &planet.field, &planet.geometry)
                .expect("valid planet cache");
            state.physics.material_queries_dirty |= rebuilt > 0;
            planet.hash = planet.field.hash();
            planet.supported = planet.footing.iter().all(|coord| {
                planet
                    .field
                    .cell(*coord)
                    .is_some_and(|c| c.material != MaterialId::VOID)
            });
        }
        for piece in detached {
            state.physics.material_queries_dirty = true;
            let id = state.terrain.next_fragment;
            state.terrain.next_fragment = id.checked_add(1).expect("terrain identity exhausted");
            let fragment = TerrainFragment::insert(
                &mut state.physics.world,
                PhysicsId::new(id),
                piece,
                motion,
                center,
                engine_rapier::terrain::TerrainSpec {
                    surface: state.terrain.surface,
                    ..physics::terrain_spec()
                },
            )
            .expect("valid fragment");
            state.physics.terrain_fragments.insert(id);
            state.terrain.fragments.insert(id, fragment);
        }
    }
    reconcile_support(state);
}

fn reconcile_support(state: &mut SpacewarsState) {
    for (&index, terrain) in &state.terrain.planets {
        if terrain.supported {
            continue;
        }
        state.physics.disable_spaceport(index);
        state
            .spaceport_contacts
            .retain(|contact| contact.planet != index);
        for ship in &mut state.ships {
            if ship.spaceport_ejection.is_some_and(|e| e.planet == index) {
                ship.spaceport_ejection = None;
            }
        }
        let planet = &mut state.planets[index];
        planet.previous_docked_ship = None;
        planet.capturing_player_id = None;
        planet.taking_ownership_time = 0.0;
        planet.building_new_ship_time = 0.0;
        planet.dock_contest_time = 0.0;
    }
}

pub(super) fn queue_cannon_hits(state: &mut SpacewarsState) {
    if state.terrain.planets.is_empty() {
        return;
    }
    let shells = state
        .debris
        .iter()
        .enumerate()
        .filter(|(_, d)| d.kind == DebrisKind::Shell && !d.dead)
        .map(|(i, d)| (d.physics_id, i))
        .collect::<BTreeMap<_, _>>();
    let mut hits = BTreeMap::new();
    for event in state.physics.world.contact_events() {
        if event.impulse_magnitude <= 0.0 {
            continue;
        }
        for (surface, shell, local) in [
            (
                event.collider_a.entity,
                event.collider_b.entity,
                event.local_contact_a,
            ),
            (
                event.collider_b.entity,
                event.collider_a.entity,
                event.local_contact_b,
            ),
        ] {
            let Some(&shell_index) = shells.get(&shell.value()) else {
                continue;
            };
            let Some(field) = state.terrain.field(surface) else {
                continue;
            };
            let Some(edit) = local.and_then(|contact| {
                impact_edit(
                    field,
                    state.terrain.surface,
                    contact,
                    CANNON_RADIUS,
                    CANNON_WORK,
                )
            }) else {
                continue;
            };
            hits.entry(shell.value()).or_insert((
                shell_index,
                PendingEdit {
                    body: surface,
                    edit,
                },
            ));
        }
    }
    state.terrain.budget_skips += hits.len().saturating_sub(MAX_CANNON_HITS) as u64;
    for (ordinal, (_, (shell, edit))) in hits.into_iter().enumerate() {
        // A projectile is consumed by its first solid hit, including when this
        // tick's work budget is full. It cannot drill by resting on the ground.
        state.debris[shell].dead = true;
        if ordinal < MAX_CANNON_HITS {
            state.terrain.pending.push(edit);
            state.terrain.cannon_hits += 1;
        }
    }
}

pub(super) fn queue_asteroid_hit(
    state: &mut SpacewarsState,
    asteroid: u64,
    target: PhysicsId,
    radius: u32,
    work: u8,
) -> bool {
    let hit = state
        .physics
        .world
        .contact_events()
        .iter()
        .find_map(|event| {
            for (surface, debris, contact) in [
                (
                    event.collider_a.entity,
                    event.collider_b.entity,
                    event.local_contact_a,
                ),
                (
                    event.collider_b.entity,
                    event.collider_a.entity,
                    event.local_contact_b,
                ),
            ] {
                if debris.value() == asteroid
                    && surface == target
                    && let Some(field) = state.terrain.field(surface)
                    && let Some(edit) = contact
                        .and_then(|p| impact_edit(field, state.terrain.surface, p, radius, work))
                {
                    return Some(PendingEdit {
                        body: surface,
                        edit,
                    });
                }
            }
            None
        });
    if let Some(edit) = hit {
        state.terrain.pending.push(edit);
        true
    } else {
        false
    }
}

fn impact_edit(
    field: &Terrain,
    surface: TerrainSurface,
    contact: ContactPoint,
    radius: u32,
    work: u8,
) -> Option<TerrainEdit> {
    let inside = contact.position - contact.normal * 0.001;
    if surface != TerrainSurface::Blocks {
        return Some(TerrainEdit {
            brush: Brush::Circle {
                center: if surface == TerrainSurface::Interpolated {
                    field.contact_cell(contact.position, contact.normal, surface)?
                } else {
                    field.surface_cell(inside, surface)?
                },
                radius,
            },
            mode: EditMode::Damage(work),
        });
    }
    let sampled = field.local_to_cell(inside)?;
    let center = (-1..=1)
        .flat_map(|y| (-1..=1).map(move |x| CellCoord::new(sampled.x + x, sampled.y + y)))
        .filter(|coord| {
            field
                .cell(*coord)
                .is_some_and(|c| c.material != MaterialId::VOID)
        })
        .filter(|coord| {
            let offset = field.cell_center(*coord) - inside;
            offset.x.abs() <= 0.502 && offset.y.abs() <= 0.502
        })
        .min_by(|a, b| {
            (field.cell_center(*a) - inside)
                .length_squared()
                .total_cmp(&(field.cell_center(*b) - inside).length_squared())
                .then((a.y, a.x).cmp(&(b.y, b.x)))
        })?;
    Some(TerrainEdit {
        brush: Brush::Circle { center, radius },
        mode: EditMode::Damage(work),
    })
}

/// Fixture controls have their own action kind and held snapshots, preserving
/// button edges across checkpoints and requiring release before another cut.
pub fn fixture_controls(tunnel: bool, view: bool) -> Action {
    Action::scenario(FIXTURE_ACTION, vec![1, u8::from(tunnel), u8::from(view)])
}

pub(super) fn apply_fixture_controls(state: &mut SpacewarsState, actions: &[Action]) {
    if !state.terrain.fixture {
        return;
    }
    for action in actions {
        let Action::Scenario {
            kind: FIXTURE_ACTION,
            payload,
        } = action
        else {
            continue;
        };
        let [1, tunnel @ 0..=1, view @ 0..=1] = payload.as_slice() else {
            continue;
        };
        let controls = [*tunnel != 0, *view != 0];
        if controls[0] && !state.terrain.previous_controls[0] {
            let field = state.planet_terrain(0).expect("fixture planet");
            let planet = state.planets[0];
            let direction = state.ships[0]
                .direction
                .rotate_radians(-planet.wrapper_angle);
            let reach = planet.radius + 25.0;
            let start = field.local_to_cell(-direction * reach).unwrap();
            let end = field.local_to_cell(direction * reach).unwrap();
            state
                .queue_planet_edit(
                    0,
                    TerrainEdit {
                        brush: Brush::Capsule {
                            start,
                            end,
                            radius: 14,
                        },
                        mode: EditMode::Remove,
                    },
                )
                .expect("bounded fixture tunnel");
        }
        if controls[1] && !state.terrain.previous_controls[1] {
            state.terrain.follow_ship = !state.terrain.follow_ship;
        }
        state.terrain.previous_controls = controls;
    }
}

impl SpacewarsScenario {
    pub fn init_terrain_fixture(seed: u64) -> SpacewarsState {
        let config = SpacewarsConfig {
            universe_radius: 500,
            use_planets: false,
            asteroid_probability_per_sec: 0.0,
            use_starfield: false,
            player_view_heights: [220.0; 2],
            ..SpacewarsConfig::default()
        };
        let mut state = Self::init(config, seed);
        let center = Vec2::new(500.0, 500.0);
        state.planets = vec![PlanetState {
            position: center,
            radius: 60.0,
            mass: body_mass(60.0),
            color: Color::rgb(0.22, 0.36, 0.45),
            owner_id: Some(0),
            capturing_player_id: None,
            previous_docked_ship: None,
            dock_contest_time: 0.0,
            taking_ownership_time: 0.0,
            building_new_ship_time: 0.0,
            orbit_radius: 0.0,
            orbit_angle: 0.0,
            orbit_omega: 0.0,
            wrapper_angle: 0.0,
            wrapper_omega: 0.035,
        }];
        state.rover_builds = vec![RoverBuildState {
            owner_id: Some(0),
            progress: 1.0,
        }];
        state.ships[0].position = center + Vec2::new(0.0, 110.0);
        state.ships[0].rotation_radians = core::f32::consts::PI;
        state.ships[0].direction = direction_from_rotation(core::f32::consts::PI);
        state.ships[1].position = center + Vec2::new(350.0, 0.0);
        state.physics = physics::SpacewarsPhysics::new(500.0, &state.ships, None, &state.planets);
        state.terrain.fixture = true;
        state.enable_planet_terrain(0).expect("fixture terrain");
        refresh_player_planet_counts(&mut state);
        state
    }
}

pub(super) fn render_wireframe(
    frame: &mut RenderFrame,
    field: &Terrain,
    geometry: &TerrainGeometry,
    position: Vec2,
    angle: f32,
) {
    for chunk in geometry.chunks() {
        let rectangles = chunk.rectangles.iter().map(|r| {
            let center = r.local_center(field);
            let half = r.half_extents(field);
            [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(x, y)| center + Vec2::new(x * half.x, y * half.y))
                .to_vec()
        });
        for vertices in rectangles.chain(chunk.polygons.iter().map(|p| p.vertices.clone())) {
            frame.push_primitive(
                PLANET_LAYER + 1,
                RenderPrimitive::Polygon(RenderPolygon {
                    points: vertices
                        .into_iter()
                        .map(|p| render_point(position + p.rotate_radians(angle)))
                        .collect(),
                    fill: None,
                    stroke: Some(Stroke::new(RenderColor::rgb(0.04, 0.08, 0.1), 0.5)),
                }),
            );
        }
    }
}

pub(super) fn render_body(
    frame: &mut RenderFrame,
    field: &Terrain,
    geometry: &TerrainGeometry,
    position: Vec2,
    angle: f32,
) {
    for chunk in geometry.chunks() {
        for rect in &chunk.rectangles {
            let color = if rect.material == ORE {
                RenderColor::rgb(0.53, 0.49, 0.25)
            } else {
                RenderColor::rgb(0.22, 0.36, 0.45)
            };
            let center = rect.local_center(field);
            let half = rect.half_extents(field);
            let points = [
                (-half.x, -half.y),
                (half.x, -half.y),
                (half.x, half.y),
                (-half.x, half.y),
            ]
            .map(|(x, y)| render_point(position + (center + Vec2::new(x, y)).rotate_radians(angle)))
            .to_vec();
            frame.push_primitive(
                PLANET_LAYER,
                RenderPrimitive::Polygon(RenderPolygon {
                    points,
                    fill: Some(Fill::new(color)),
                    stroke: Some(Stroke::new(color, 0.75)),
                }),
            );
        }
        for polygon in &chunk.polygons {
            let color = if polygon.material == ORE {
                RenderColor::rgb(0.53, 0.49, 0.25)
            } else {
                RenderColor::rgb(0.22, 0.36, 0.45)
            };
            frame.push_primitive(
                PLANET_LAYER,
                RenderPrimitive::Polygon(RenderPolygon {
                    points: polygon
                        .vertices
                        .iter()
                        .map(|&p| render_point(position + p.rotate_radians(angle)))
                        .collect(),
                    fill: Some(Fill::new(color)),
                    stroke: Some(Stroke::new(color, 0.75)),
                }),
            );
        }
    }
}

impl SpacewarsScenario {
    pub fn render_terrain_fixture(state: &SpacewarsState) -> RenderFrame {
        let camera = if state.terrain.follow_ship {
            player_camera(state, 0)
        } else {
            Camera2::new(render_point(state.planets[0].position), 290.0)
        };
        let mut frame = render_state_with_camera(state, camera, RenderOptions::player());
        let center = Vec2::new(camera.center.x, camera.center.y);
        for (bottom, top) in [(0.34, 0.5), (-0.5, -0.36)] {
            let color = RenderColor::rgba(0.015, 0.025, 0.04, 0.94);
            frame.push_primitive(
                20,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    [(-4.0, bottom), (4.0, bottom), (4.0, top), (-4.0, top)]
                        .map(|(x, y)| render_point(center + Vec2::new(x, y) * camera.height))
                        .to_vec(),
                    color,
                )),
            );
        }
        let text = |frame: &mut RenderFrame, y: f32, message: String, size, color| {
            frame.push_primitive(
                21,
                RenderPrimitive::Text(RenderText {
                    position: render_point(center + Vec2::new(0.0, y * camera.height)),
                    text: message,
                    color,
                    size,
                    anchor: TextAnchor::Center,
                }),
            );
        };
        text(
            &mut frame,
            0.45,
            "SPACEWARS TERRAIN".into(),
            22.0,
            RenderColor::WHITE,
        );
        text(
            &mut frame,
            0.395,
            "Cannon: Y / K   Laser: B / Space   Turn: stick   Thrust: ZR   Brake: ZL".into(),
            13.0,
            RenderColor::WHITE,
        );
        text(
            &mut frame,
            -0.395,
            "X / T: test tunnel   L / V: view   + / Esc: pause & restart".into(),
            13.0,
            RenderColor::WHITE,
        );
        text(
            &mut frame,
            -0.455,
            format!(
                "Hull {:.0}%   Cannon hits {}   Removed {} cells   Fragments {}   Base: {}",
                100.0 * state.ships[0].life / state.ships[0].life_max,
                state.terrain.cannon_hits,
                state.terrain.removed_cells,
                state.terrain.fragments.len(),
                if state.terrain.supported(0) {
                    "SUPPORTED"
                } else {
                    "OFFLINE"
                }
            ),
            13.0,
            RenderColor::rgb(0.35, 0.9, 0.85),
        );
        frame
    }
}

pub(super) fn observation(state: &SpacewarsState) -> Observation {
    if state.terrain.planets.is_empty() {
        return Observation {
            payload: Vec::new(),
        };
    }
    let terrain = &state.terrain;
    let mut payload = vec![1, u8::from(terrain.fixture), u8::from(terrain.follow_ship)];
    payload.extend(terrain.previous_controls.map(u8::from));
    for value in [
        state.tick,
        terrain.next_fragment,
        terrain.removed_cells,
        terrain.cannon_hits,
        terrain.budget_skips,
    ] {
        payload.extend(value.to_le_bytes());
    }
    payload.extend((terrain.planets.len() as u32).to_le_bytes());
    for (&id, planet) in &terrain.planets {
        for value in [id as u64, planet.hash, planet.field.revision()] {
            payload.extend(value.to_le_bytes());
        }
        payload.push(u8::from(planet.supported));
    }
    payload.extend((terrain.fragments.len() as u32).to_le_bytes());
    for (&id, fragment) in &terrain.fragments {
        for value in [id, fragment.hash, fragment.terrain.revision()] {
            payload.extend(value.to_le_bytes());
        }
        let motion = state
            .physics
            .world
            .motion(fragment.assembly.body())
            .expect("fragment motion");
        for value in [
            motion.position.x,
            motion.position.y,
            motion.angle,
            motion.linear_velocity.x,
            motion.linear_velocity.y,
            motion.angular_velocity,
        ] {
            payload.extend(value.to_le_bytes());
        }
    }
    payload.extend((terrain.pending.len() as u32).to_le_bytes());
    for pending in &terrain.pending {
        payload.extend(pending.body.value().to_le_bytes());
        let (tag, start, end, radius) = match pending.edit.brush {
            Brush::Circle { center, radius } => (0, center, center, radius),
            Brush::Capsule { start, end, radius } => (1, start, end, radius),
        };
        payload.push(tag);
        for value in [start.x, start.y, end.x, end.y] {
            payload.extend(value.to_le_bytes());
        }
        payload.extend(radius.to_le_bytes());
        payload.extend(match pending.edit.mode {
            EditMode::Remove => [0, 0],
            EditMode::Damage(work) => [1, work],
        });
    }
    if terrain.surface == TerrainSurface::Contour {
        payload.extend(b"contour-v1");
    } else if terrain.surface == TerrainSurface::Interpolated {
        payload.extend(b"interpolated-v1");
    }
    Observation { payload }
}

#[cfg(test)]
mod tests;
