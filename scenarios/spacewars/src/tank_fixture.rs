//! Shared, source-independent planetary tank acceptance script.
//! Timing belongs to callers; sampling, edits, replay and rendering are outside it.
use std::time::Duration;

use engine_common::{DirtMode, RenderFrame};
use engine_core::Vec2;
use engine_terrain::{Brush, EditMode, TerrainEdit};
use serde::Serialize;

use crate::{
    SpacewarsScenario, SpacewarsState,
    tanks::{TankCommand, TankId},
};

pub const FIXTURE_VERSION: &str = "planetary-tank-237-v1";
pub const WARMUP_STEPS: usize = 120;
pub const MEASURED_STEPS: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureCase {
    Drive,
    Cut,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TankSample {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub angle: f32,
    pub spin: f32,
    pub wheel_positions: [[f32; 2]; 2],
    pub wheel_velocities: [[f32; 2]; 2],
    pub supported: bool,
    pub relative_speed: Option<f32>,
    pub blocked: bool,
    pub aim: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FixtureSample {
    pub tick: usize,
    pub tanks: Vec<TankSample>,
    pub pilot_motion: [f32; 6],
    pub queries: usize,
    pub round_grains: usize,
    pub angular_grains: usize,
    /// Includes pairs/manifolds even when they yield no contacts.
    pub contact_traversal: usize,
    pub spatial_traversal: usize,
    pub spatial_exhaustions: usize,
    pub physics_steps: u64,
    pub gravity_bodies: usize,
    pub gravity_applications: usize,
    pub duplicate_gravity: usize,
    pub bodies: usize,
    pub colliders: usize,
    pub joints: usize,
    pub material_cells: u64,
    pub removed_cells: u64,
    pub world_motion_hash: String,
}

pub struct TankFixture {
    pub state: SpacewarsState,
    ids: Vec<TankId>,
    pilot: crate::surface_sortie::SurfacePilot,
    case: FixtureCase,
    tick: usize,
    initial_material: u64,
    initial_physics_steps: u64,
    cut_start: Option<(Vec2, Vec2)>,
    planned_edits: Vec<(usize, Vec2, u32)>,
    recorded_edits: Vec<(usize, Vec2, u32)>,
}

impl TankFixture {
    pub fn new(seed: u64, shape: DirtMode, case: FixtureCase) -> Self {
        Self::with_tanks(seed, shape, case, 2)
    }

    /// Zero-tank controls use the exact same local cut schedule obtained from a
    /// separate untimed two-tank rehearsal. Neither rehearsal nor warmup is timed.
    pub fn with_tanks(seed: u64, shape: DirtMode, case: FixtureCase, count: usize) -> Self {
        assert!(count == 0 || count == 2);
        let mut rehearsal = Self::build(seed, shape, case, 2);
        for _ in 0..MEASURED_STEPS {
            rehearsal.prepare_step();
            rehearsal.step();
            rehearsal.sample();
        }
        let mut fixture = Self::build(seed, shape, case, count);
        fixture.planned_edits = rehearsal.recorded_edits;
        fixture
    }

    fn build(seed: u64, shape: DirtMode, case: FixtureCase, count: usize) -> Self {
        let mut state = SpacewarsScenario::init_terrain_fixture(seed);
        // Keep ordinary ships visible but out of the isolated driving route.
        state.ships[0].position = Vec2::new(700.0, 700.0);
        state.ships[1].position = Vec2::new(800.0, 500.0);
        state
            .enable_loose_terrain(crate::surface_sortie::dirt_config(shape))
            .unwrap();
        let mut pilot = crate::surface_sortie::SurfacePilot::tank_comparison(&mut state);
        Self::host_step(&mut state, &mut pilot);
        let mut ids = Vec::new();
        if count == 2 {
            // Start on the real plateau: the original bearing put the route
            // footprint across a one-cell drop above the tank step limit.
            ids.push(
                state
                    .place_tank(Vec2::new(496.0, 565.0), -Vec2::Y)
                    .expect("top admission"),
            );
        }
        Self::host_step(&mut state, &mut pilot);
        if count == 2 {
            ids.push(
                state
                    .place_tank(Vec2::new(435.0, 500.0), Vec2::X)
                    .expect("side admission"),
            );
        }
        for &id in &ids {
            assert!(state.command_tank(
                id,
                TankCommand {
                    brake: true,
                    ..Default::default()
                }
            ));
        }
        for _ in 0..WARMUP_STEPS {
            Self::host_step(&mut state, &mut pilot);
        }
        let d = state.terrain_diagnostics();
        let initial_physics_steps = state.tank_step_audit().physics_steps;
        Self {
            state,
            ids,
            pilot,
            case,
            tick: 0,
            initial_physics_steps,
            initial_material: d.occupied_cells + d.loose_cells as u64 + d.removed_cells,
            cut_start: None,
            planned_edits: Vec::new(),
            recorded_edits: Vec::new(),
        }
    }

    fn host_step(state: &mut SpacewarsState, pilot: &mut crate::surface_sortie::SurfacePilot) {
        SpacewarsScenario::step_with_surface_pilots(
            state,
            &[],
            Duration::from_secs_f32(1.0 / 60.0),
            std::slice::from_mut(pilot),
            None,
            None,
            true,
        );
    }

    /// Script changes happen before the timed host step. Terrain cuts use actual
    /// local material coordinates, not a remembered circular support radius.
    pub fn prepare_step(&mut self) {
        let drive = self.tick < 120 || (240..420).contains(&self.tick);
        for (index, &id) in self.ids.iter().enumerate() {
            assert!(self.state.command_tank(
                id,
                TankCommand {
                    drive: if index == 0 && drive { 1.0 } else { 0.0 },
                    brake: index == 1 || !drive,
                    turret_angle: if index == 0 { 0.3 } else { -0.4 },
                }
            ));
        }
        if let Some((_, local, radius)) = self
            .planned_edits
            .iter()
            .find(|e| e.0 == self.tick)
            .copied()
        {
            if self.tick == 420 && !self.ids.is_empty() {
                let position = self
                    .state
                    .tank_snapshot(self.ids[0])
                    .unwrap()
                    .assembly
                    .chassis
                    .position;
                self.cut_start = Some((
                    position,
                    (position - self.state.planets[0].position).normalized(),
                ));
            }
            self.queue_local_cut(local, radius);
        }
        if self.tick == 240 && self.planned_edits.is_empty() {
            let s = self.state.tank_snapshot(self.ids[0]).unwrap();
            let support = s.support.expect("ground contact before gap");
            self.cut_at(
                support.position + Vec2::new(support.normal.y, -support.normal.x) * 7.0,
                3,
            );
        }
        if self.tick == 420 && self.case == FixtureCase::Cut && self.planned_edits.is_empty() {
            let s = self.state.tank_snapshot(self.ids[0]).unwrap();
            let position = s.assembly.chassis.position;
            let radial = (position - self.state.planets[0].position).normalized();
            self.cut_start = Some((position, radial));
            self.cut_at(position, 10);
        }
        if self.tick == 480 {
            // Away from the driving routes: exercise actual released dirt shapes
            // without letting falling traffic contaminate drive/brake measurements.
            self.state
                .queue_fixture_release(0, Vec2::new(-20.0, -50.0), 3)
                .unwrap();
        }
        // Prescribed translation is shared with the host; no assembly transport.
        self.state.planets[0].position += Vec2::X * (0.3 / 60.0);
    }

    fn cut_at(&mut self, point: Vec2, radius: u32) {
        let planet = self.state.planets[0];
        let local = (point - planet.position).rotate_radians(-planet.wrapper_angle);
        self.recorded_edits.push((self.tick, local, radius));
        self.queue_local_cut(local, radius);
    }

    fn queue_local_cut(&mut self, local: Vec2, radius: u32) {
        let center = self
            .state
            .planet_terrain(0)
            .unwrap()
            .local_to_cell(local)
            .expect("cut within material grid");
        self.state
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle { center, radius },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
    }

    pub fn step(&mut self) {
        Self::host_step(&mut self.state, &mut self.pilot);
    }

    /// Sampling scans diagnostics outside simulation timing.
    pub fn sample(&mut self) -> FixtureSample {
        let tanks = self
            .ids
            .iter()
            .map(|&id| {
                let s = self.state.tank_snapshot(id).unwrap();
                let c = s.assembly.chassis;
                TankSample {
                    position: [c.position.x, c.position.y],
                    velocity: [c.linear_velocity.x, c.linear_velocity.y],
                    angle: c.angle,
                    spin: c.angular_velocity,
                    wheel_positions: s.assembly.wheels.map(|w| [w.position.x, w.position.y]),
                    wheel_velocities: s
                        .assembly
                        .wheels
                        .map(|w| [w.linear_velocity.x, w.linear_velocity.y]),
                    supported: s.support.is_some(),
                    relative_speed: s.support.map(|support| {
                        (c.linear_velocity - support.velocity)
                            .dot(Vec2::new(support.normal.y, -support.normal.x))
                    }),
                    blocked: s.drive_blocked,
                    aim: s.command.turret_angle,
                }
            })
            .collect();
        let audit = self.state.tank_step_audit();
        let work = self.state.tank_work();
        let d = self.state.terrain_diagnostics();
        let pilot = self.pilot_snapshot().expect("live comparison pilot").motion;
        let sample = FixtureSample {
            pilot_motion: [
                pilot.position.x,
                pilot.position.y,
                pilot.linear_velocity.x,
                pilot.linear_velocity.y,
                pilot.angle,
                pilot.angular_velocity,
            ],
            tick: self.tick,
            tanks,
            queries: work.queries,
            round_grains: self
                .state
                .loose_terrain()
                .unwrap()
                .iter()
                .filter(|g| g.shape() == engine_rapier::terrain::GrainShape::Round)
                .count(),
            angular_grains: self
                .state
                .loose_terrain()
                .unwrap()
                .iter()
                .filter(|g| g.shape() == engine_rapier::terrain::GrainShape::Hexagon)
                .count(),
            contact_traversal: work.contact_traversal,
            spatial_traversal: work.spatial_traversal,
            spatial_exhaustions: work.spatial_exhaustions,
            physics_steps: audit.physics_steps,
            gravity_bodies: audit.gravity_bodies,
            gravity_applications: audit.gravity_applications,
            duplicate_gravity: audit.duplicate_gravity_applications,
            bodies: self.state.physics.world.body_count(),
            colliders: self.state.physics.world.collider_count(),
            joints: self.state.physics.world.joint_count(),
            material_cells: d.occupied_cells + d.loose_cells as u64 + d.removed_cells,
            removed_cells: d.removed_cells,
            world_motion_hash: d.motion_hash,
        };
        self.tick += 1;
        sample
    }

    pub fn pilot_snapshot(&self) -> Option<engine_rapier::spaceling::SpacelingSnapshot> {
        self.pilot.snapshot(&self.state.physics)
    }

    pub fn frame(&self) -> RenderFrame {
        // Ordinary world primitives and physical dimensions; only the inspection
        // camera is focused on this small material planet.
        let mut frame = crate::render_state_with_camera(
            &self.state,
            engine_common::Camera2::new(crate::render_point(self.state.planets[0].position), 220.0),
            crate::RenderOptions::player(),
        );
        self.pilot
            .render_comparison(&self.state.physics, &mut frame);
        frame
    }

    /// Actual lifecycle cleanup, retaining the monotonic IDs for stale-handle checks.
    pub fn cleanup(&mut self) -> bool {
        let before = (
            self.state.physics.world.body_count(),
            self.state.physics.world.collider_count(),
            self.state.physics.world.joint_count(),
        );
        self.state.clear_tanks();
        self.ids.iter().all(|&id| {
            self.state.tank_snapshot(id).is_none()
                && !self.state.command_tank(id, TankCommand::default())
        }) && self.state.tank_ids().count() == 0
            && self.state.physics.world.body_count() + self.ids.len() * 3 == before.0
            && self.state.physics.world.collider_count() + self.ids.len() * 3 == before.1
            && self.state.physics.world.joint_count() + self.ids.len() * 2 == before.2
    }

    pub fn assertions(
        &self,
        trace: &[FixtureSample],
    ) -> std::collections::BTreeMap<&'static str, bool> {
        assert_eq!(trace.len(), MEASURED_STEPS);
        let expected = self.state.loose_terrain().unwrap().config().shape;
        let shapes_exercised = trace.iter().any(|s| match expected {
            engine_rapier::terrain::GrainShape::Round => {
                s.round_grains > 0 && s.angular_grains == 0
            }
            engine_rapier::terrain::GrainShape::Hexagon => {
                s.angular_grains > 0 && s.round_grains == 0
            }
        });
        if self.ids.is_empty() {
            return std::collections::BTreeMap::from([
                ("released_shape_exercised", shapes_exercised),
                (
                    "one_physics_step",
                    trace[0].physics_steps == self.initial_physics_steps + 1
                        && trace
                            .windows(2)
                            .all(|w| w[1].physics_steps == w[0].physics_steps + 1),
                ),
                (
                    "no_duplicate_gravity",
                    trace.iter().all(|s| {
                        s.gravity_applications == 0
                            && s.gravity_bodies == 0
                            && s.duplicate_gravity == 0
                    }),
                ),
                (
                    "material_conserved",
                    trace
                        .iter()
                        .all(|s| s.material_cells == self.initial_material),
                ),
                (
                    "bounded_work",
                    trace
                        .iter()
                        .all(|s| s.queries == 0 && s.contact_traversal == 0),
                ),
            ]);
        }
        let mean = |range: std::ops::Range<usize>, index: usize| {
            let values: Vec<_> = trace[range]
                .iter()
                .filter_map(|s| s.tanks[index].relative_speed)
                .collect();
            (
                values.len(),
                values.iter().sum::<f32>() / values.len().max(1) as f32,
            )
        };
        let (drive_n, drive) = mean(60..120, 0);
        let (brake_n, brake) = mean(180..240, 0);
        let (_, independent) = mean(60..240, 1);
        let mut checks = std::collections::BTreeMap::from([
            ("released_shape_exercised", shapes_exercised),
            (
                "one_physics_step",
                trace[0].physics_steps == self.initial_physics_steps + 1
                    && trace
                        .windows(2)
                        .all(|w| w[1].physics_steps == w[0].physics_steps + 1),
            ),
            (
                "no_duplicate_gravity",
                trace.iter().all(|s| {
                    s.gravity_applications == 6 && s.gravity_bodies == 6 && s.duplicate_gravity == 0
                }),
            ),
            (
                "material_conserved",
                trace
                    .iter()
                    .all(|s| s.material_cells == self.initial_material),
            ),
            (
                "bounded_work",
                trace.iter().all(|s| {
                    s.queries <= 12 && s.contact_traversal <= 256 && s.spatial_traversal <= 16384
                }),
            ),
            (
                "independent_controls",
                independent.abs() < 0.8
                    && trace
                        .iter()
                        .all(|s| s.tanks[0].aim == 0.3 && s.tanks[1].aim == -0.4),
            ),
            (
                "supported_drive_and_brake",
                drive_n >= 50 && brake_n >= 50 && drive > 1.0 && brake.abs() < 0.8,
            ),
            (
                "unsafe_gap_refused",
                trace[240..420]
                    .iter()
                    .filter(|s| s.tanks[0].blocked)
                    .count()
                    > 30
                    && trace[240..420]
                        .iter()
                        .filter(|s| s.tanks[0].supported)
                        .count()
                        > 150,
            ),
        ]);
        if let Some((start, radial)) = self.cut_start {
            checks.insert(
                "support_loss_fall_or_fragment_ride",
                trace[420..430].iter().any(|s| !s.tanks[0].supported)
                    && trace[420..450].iter().any(|s| {
                        (Vec2::new(s.tanks[0].position[0], s.tanks[0].position[1]) - start)
                            .dot(radial)
                            < -1.0
                    }),
            );
        }
        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_script_measures_drive_brake_gap_cut_and_replay() {
        for shape in [DirtMode::Round, DirtMode::Angular] {
            for case in [FixtureCase::Drive, FixtureCase::Cut] {
                for count in [0, 2] {
                    let mut fixture = TankFixture::with_tanks(42, shape, case, count);
                    let mut replay = TankFixture::with_tanks(42, shape, case, count);
                    let mut trace = Vec::new();
                    for _ in 0..MEASURED_STEPS {
                        fixture.prepare_step();
                        fixture.step();
                        let pilot = fixture.pilot_snapshot().expect("live comparison pilot");
                        let center = fixture.state.planets[0].position;
                        assert!(
                            (pilot.motion.position - center).length() < 100.0,
                            "pilot outside comparison camera: {:?}",
                            pilot.motion.position
                        );
                        for id in &fixture.ids {
                            let tank = fixture.state.tank_snapshot(*id).unwrap();
                            assert!(
                                (pilot.motion.position - tank.assembly.chassis.position).length()
                                    > 12.0,
                                "comparison pilot entered driving course"
                            );
                        }
                        let ordinary = crate::render_state_with_camera(
                            &fixture.state,
                            fixture.frame().camera,
                            crate::RenderOptions::player(),
                        );
                        let count = |frame: RenderFrame| {
                            frame
                                .layers
                                .iter()
                                .map(|l| l.primitives.len())
                                .sum::<usize>()
                        };
                        assert!(
                            count(fixture.frame()) > count(ordinary),
                            "pilot drawing missing"
                        );
                        let sample = fixture.sample();
                        replay.prepare_step();
                        replay.step();
                        assert_eq!(sample, replay.sample());
                        trace.push(sample);
                    }
                    let checks = fixture.assertions(&trace);
                    eprintln!("{shape:?} {case:?}: {checks:?}");
                    assert!(checks.values().all(|&v| v), "{checks:?}");
                    assert!(fixture.cleanup());
                }
            }
        }
    }
}
