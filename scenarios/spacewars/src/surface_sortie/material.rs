//! Material-ground integration for the existing Expedition actor and claim loop.
use super::*;
use engine_rapier::world::{RayCastOptions, RayHit};
use engine_terrain::{Brush, CellCoord, EditMode, MaterialId, TerrainEdit};

const MINING_ACTION: u32 = 0x5354_0001;
const CUT_RADII: [u32; 3] = [0, 1, 3];
const MINING_RANGE: f32 = 8.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SurfaceMiningAction {
    pub aim: Vec2,
    pub held: bool,
    pub cycle: bool,
}

impl SurfaceMiningAction {
    pub fn encode(self, player: PlayerId) -> Action {
        let mut bytes = self.aim.x.to_le_bytes().to_vec();
        bytes.extend(self.aim.y.to_le_bytes());
        bytes.extend([self.held as u8, self.cycle as u8, player.index() as u8]);
        Action::scenario(MINING_ACTION, bytes)
    }

    pub fn decode(action: &Action) -> Option<(usize, Self)> {
        let Action::Scenario {
            kind: MINING_ACTION,
            payload,
        } = action
        else {
            return None;
        };
        if payload.len() != 11 || payload[8] > 1 || payload[9] > 1 {
            return None;
        }
        let player = PlayerId::from_index(payload[10] as usize)?.index();
        let aim = Vec2::new(
            f32::from_le_bytes(payload[..4].try_into().ok()?),
            f32::from_le_bytes(payload[4..8].try_into().ok()?),
        );
        (aim.x.is_finite() && aim.y.is_finite() && aim.length_squared().is_finite()).then_some((
            player,
            Self {
                aim: if aim.length_squared() > 0.01 {
                    aim.normalized()
                } else {
                    Vec2::ZERO
                },
                held: payload[8] != 0,
                cycle: payload[9] != 0,
            },
        ))
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct MiningSeat {
    input: SurfaceMiningAction,
    tool: usize,
    armed: bool,
    previous_cycle: bool,
    cooldown: u8,
    beam: Option<(Vec2, Vec2)>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SurfaceMining {
    seats: [MiningSeat; SPACEWARS_PLAYER_COUNT],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurfaceMiningObservation {
    pub radius: u32,
    pub armed: bool,
    pub cooldown_ticks: u8,
    pub aim: Vec2,
    pub held: bool,
    pub cycle_held: bool,
    pub beam: Option<(Vec2, Vec2)>,
    pub removed_cells: u64,
    pub fragments: usize,
}

impl SurfaceSortieScenario {
    /// The controlled material-planet acceptance scene of the Expedition loop.
    /// Historical terrain stress fixtures and ordinary Spacewars stay available
    /// to their existing runners with their original world parameters.
    pub fn init_material(seed: u64, players: usize) -> SurfaceSortieState {
        Self::init_material_surface(seed, players, engine_terrain::TerrainSurface::Blocks)
    }

    pub(super) fn init_material_surface(
        seed: u64,
        players: usize,
        surface: engine_terrain::TerrainSurface,
    ) -> SurfaceSortieState {
        assert!((1..=SPACEWARS_PLAYER_COUNT).contains(&players));
        let mut state = Self::init(SurfaceMotionPreset::Stationary, seed);
        state.outposts.clear();
        state.world.terrain.surface = surface;
        state.world.terrain.legacy_services = false;
        state
            .world
            .enable_planet_terrain(0)
            .expect("controlled material planet");
        state.enable_planet_claims();
        for player in 0..players {
            if player == 1 {
                let planet = state.world.planets[0];
                let up = -Vec2::Y;
                let center =
                    planet.position + up * (planet.radius * BODY_BOUNDS_RADIUS_SCALE + 6.0);
                let ship = &mut state.world.ships[player];
                ship.position = center - SHIP_PIVOT;
                ship.rotation_radians = rotation_for_direction(up);
                ship.direction = up;
                ship.velocity =
                    Vec2::new(-(center - planet.position).y, (center - planet.position).x)
                        * planet.wrapper_omega;
                state
                    .pilots
                    .push(SurfacePilot::new(PlayerId::PLAYER_2, 0, true));
            }
            state.world.ships[player].life = state.world.ships[player].life_max;
            state.pilots[player].flight_enabled = true;
        }
        state
            .world
            .physics
            .enable_surface_sortie(&(0..players).collect::<Vec<_>>(), &state.world.ships);
        state.enable_recovery();
        state.mining = Some(SurfaceMining::default());
        state
    }
}

impl SurfaceSortieState {
    pub fn has_material_ground(&self) -> bool {
        self.mining.is_some()
    }

    pub fn terrain_diagnostics(&self) -> TerrainDiagnostics {
        self.world.terrain_diagnostics()
    }

    pub fn mining_observation(&self, player: usize) -> Option<SurfaceMiningObservation> {
        let seat = self.mining.as_ref()?.seats.get(player)?;
        Some(SurfaceMiningObservation {
            radius: CUT_RADII[seat.tool],
            armed: seat.armed,
            cooldown_ticks: seat.cooldown,
            aim: seat.input.aim,
            held: seat.input.held,
            cycle_held: seat.input.cycle,
            beam: seat.beam,
            removed_cells: self.world.terrain.removed_cells,
            fragments: self.world.terrain.fragments.len(),
        })
    }

    pub(super) fn material_footings(&self) -> [[Option<CellCoord>; 2]; SPACEWARS_PLAYER_COUNT] {
        std::array::from_fn(|player| {
            let Some(pilot) = self.pilots.get(player) else {
                return [None; 2];
            };
            let Some(terrain) = self.world.terrain.planets.get(&pilot.planet) else {
                return [None; 2];
            };
            let Some(body) = self
                .world
                .physics
                .world
                .motion(self.world.physics.ship_body(pilot.vehicle.0))
            else {
                return [None; 2];
            };
            let frame = motion::SurfaceFrame::read(&self.world.physics, pilot.planet);
            let up = (body.position - frame.position).normalized();
            let contacts = if pilot.landing.phase == LandingPhase::Landed
                && self.world.ships[pilot.vehicle.0].form == ShipForm::Ship
            {
                self.world.physics.parked_landing_support_contacts(
                    pilot.vehicle.0,
                    pilot.planet,
                    up,
                )
            } else {
                self.world
                    .physics
                    .landing_support_contacts(pilot.vehicle.0, pilot.planet, up)
            };
            contacts.map(|contact| {
                contact.and_then(|contact| {
                    terrain.geometry.contact_cell(
                        &terrain.field,
                        (contact.position - frame.position).rotate_radians(-frame.angle),
                        contact.normal.rotate_radians(-frame.angle),
                    )
                })
            })
        })
    }

    pub(super) fn reconcile_material_support(
        &mut self,
        footings: [[Option<CellCoord>; 2]; SPACEWARS_PLAYER_COUNT],
    ) {
        if self.world.terrain.planets.is_empty() {
            return;
        }
        self.invalidate_flag_footings();
        for (player, pilot) in self.pilots.iter_mut().enumerate() {
            let previous = pilot.landing;
            // dt=0 only revalidates a previous landing; it cannot earn settling time.
            pilot.landing.update(
                &self.world.physics,
                pilot.vehicle.0,
                pilot.planet,
                &self.world.planets[pilot.planet],
                &self.world.ships[pilot.vehicle.0],
                0.0,
            );
            // Structural edits can replace handles in an otherwise unchanged
            // footing. Preserve only already-earned support whose two actual
            // contact cells survive in this same body's unchanged local frame.
            // Transfers still wait for the completed query index; after the
            // solve, fresh foot contacts must independently pass every gate.
            if self.world.physics.material_queries_dirty
                && self
                    .world
                    .terrain
                    .planets
                    .get(&pilot.planet)
                    .is_some_and(|terrain| {
                        footings[player].iter().all(|cell| {
                            cell.is_some_and(|cell| {
                                terrain
                                    .field
                                    .cell(cell)
                                    .is_some_and(|cell| cell.material != MaterialId::VOID)
                            })
                        })
                    })
            {
                pilot.landing = previous;
            }
        }
    }

    /// A short ray beside the real hull. An absent floor is an unavailable hatch,
    /// never a projection onto an excavated nominal circle.
    pub(super) fn material_access(&self, player: usize) -> Option<RayHit> {
        let pilot = &self.pilots[player];
        let ship = &self.world.ships[pilot.vehicle.0];
        let body = self
            .world
            .physics
            .world
            .motion(self.world.physics.ship_body(pilot.vehicle.0))?;
        self.material_access_at(
            pilot.planet,
            ship.form,
            body.position,
            body.angle,
            Some(pilot_physics_id(pilot.owner)),
        )
    }

    pub(super) fn material_access_at(
        &self,
        planet: usize,
        form: ShipForm,
        position: Vec2,
        angle: f32,
        exclude_actor: Option<PhysicsId>,
    ) -> Option<RayHit> {
        let spec = Self::spec();
        let clear = self.world.physics.world.capsule_clearance_test(
            spec.half_segment,
            spec.radius + 0.04,
            spec.collision_groups,
            exclude_actor,
        );
        let mut first = None;
        for hit in self.material_access_candidates_at(planet, form, position, angle) {
            first.get_or_insert(hit);
            if clear(
                hit.point + hit.normal * (spec.half_height() + 0.12),
                rotation_for_direction(hit.normal),
            ) {
                return Some(hit);
            }
        }
        // Keep the nearby floor observable when every capsule pose is blocked;
        // the authoritative transfer gate must still reject the actual exit.
        first
    }

    pub(super) fn material_access_candidates_at(
        &self,
        planet: usize,
        form: ShipForm,
        position: Vec2,
        angle: f32,
    ) -> impl Iterator<Item = RayHit> + '_ {
        let local = if form == ShipForm::Ship {
            Vec2::new(8.0, -5.0)
        } else {
            Vec2::new(2.8, -0.65)
        };
        let hatch = position + local.rotate_radians(angle);
        let surface = motion::SurfaceFrame::read(&self.world.physics, planet);
        let up = (position - surface.position).normalized();
        let right = Vec2::new(up.y, -up.x);
        // The hatch can straddle a cell edge. Search one cell to either side
        // for nearby footing without extending its reach down a deep shaft.
        [0.0, -1.0, 1.0].into_iter().filter_map(move |offset| {
            let hit = self.world.physics.material_ground_ray(
                planet,
                hatch + right * offset + up * 2.0,
                -up,
                5.0,
            )?;
            (hit.normal.dot(up) >= Self::spec().min_support_alignment).then_some(hit)
        })
    }

    pub(super) fn read_mining_actions(&mut self, actions: &[Action]) {
        let count = self.player_count();
        let Some(mining) = &mut self.mining else {
            return;
        };
        for (player, input) in actions.iter().filter_map(SurfaceMiningAction::decode) {
            if player < count {
                mining.seats[player].input = input;
            }
        }
    }

    pub(super) fn update_mining(&mut self) {
        for player in 0..self.player_count() {
            let snapshot = self.spaceling_snapshot(player);
            let pilot = &self.pilots[player];
            let Some(mining) = &mut self.mining else {
                return;
            };
            let seat = &mut mining.seats[player];
            seat.beam = None;
            seat.cooldown = seat.cooldown.saturating_sub(1);
            if snapshot.is_none() || !pilot.controls_armed {
                seat.armed = false;
            } else if !seat.armed {
                seat.armed = !seat.input.held && !seat.input.cycle;
            } else if seat.input.cycle && !seat.previous_cycle {
                seat.tool = (seat.tool + 1) % CUT_RADII.len();
            }
            seat.previous_cycle = seat.input.cycle;
            let Some(snapshot) = snapshot.filter(|_| seat.armed) else {
                continue;
            };
            let direction = if seat.input.aim == Vec2::ZERO {
                Vec2::new(snapshot.up.y, -snapshot.up.x) * pilot.facing
            } else {
                seat.input.aim
            };
            let origin = snapshot.motion.position + snapshot.up * 0.2;
            let hit = self.world.physics.world.cast_ray(
                origin,
                direction,
                RayCastOptions {
                    max_distance: MINING_RANGE,
                    exclude_entity: Some(pilot_physics_id(pilot.owner)),
                    collision_groups: physics::spaceling_collision_groups(),
                    ..RayCastOptions::default()
                },
            );
            seat.beam = Some((
                origin,
                hit.map_or(origin + direction * MINING_RANGE, |h| h.point),
            ));
            if !seat.input.held || seat.cooldown > 0 {
                continue;
            }
            seat.cooldown = 8;
            let Some(hit) = hit else {
                continue;
            };
            // The first collider occludes the beam, including ships and pilots.
            let Some(field) = self.world.terrain.field(hit.collider.entity) else {
                continue;
            };
            let body = self
                .world
                .physics
                .world
                .motion(physics::primary_body(hit.collider.entity))
                .expect("material body");
            let local = (hit.point - body.position).rotate_radians(-body.angle);
            let Some(center) = field.contact_cell(
                local,
                hit.normal.rotate_radians(-body.angle),
                self.world.terrain.surface,
            ) else {
                continue;
            };
            self.world
                .queue_material_edit(
                    hit.collider.entity,
                    TerrainEdit {
                        brush: Brush::Circle {
                            center,
                            radius: CUT_RADII[seat.tool],
                        },
                        mode: EditMode::Damage(100),
                    },
                )
                .expect("bounded mining edit");
        }
    }
}

#[cfg(test)]
mod tests;
