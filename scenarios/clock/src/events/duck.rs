//! A bounded, event-local obstacle course. The round body is physical; the
//! upright pixel character and side doors are presentation, not articulated rigs.

pub(crate) mod arena;
mod controller;
mod drain;
mod flow;
pub(crate) mod planner;
#[cfg(test)]
mod platform_tests;
mod responsive;
#[cfg(test)]
mod responsive_tests;
mod shared;
pub(crate) use shared::PlayerArena;
#[cfg(test)]
mod takeover_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod water_tests;

use controller::{Command, Controller, CourseContext, Gait, Movement, Observation};
use engine_common::{ClockDuckNavigationState, ClockDuckOutcome, ClockDuckState};
use engine_core::Vec2;
use engine_rapier::buoyancy::{BuoyancyConfig, BuoyancyReport, BuoyantBody, BuoyantMaterial};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld, PhysicsWorldConfig,
};
use engine_water::{WaterWorld, immersion::HullShape};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::EventPhase;
use crate::floor::responsive::ResponsiveFloor;
use crate::layout::Layout;

pub const DUCK_TICKS: u64 = 35 * 60;
const EXIT_DELAY_TICKS: u64 = 20 * 60;
const OPENING_TICKS: u64 = 36;
const RESET_TICKS: u64 = 30;
const DT: f32 = 1.0 / 60.0;
const DUCK_ENTITY: PhysicsId = PhysicsId::new(1);
const DUCK_BODY: BodyId = BodyId::new(DUCK_ENTITY, BodyRole::PRIMARY);
const DUCK_COLLIDER: ColliderId = ColliderId::new(DUCK_ENTITY, ColliderRole::PRIMARY, 0);
const DUCK_DENSITY: f32 = 0.45;
const PLAYER_EXIT_DELAY_TICKS: u64 = 44;
// One third of the dry actuator's acceleration. Water drag sets the eventual
// relative speed; enough authority to paddle against ordinary course runoff.
const PADDLE_ACCELERATION: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Obstacle {
    pub start: f32,
    pub end: f32,
    /// Zero is the pit; positive heights are solid hurdles.
    pub height: f32,
}

pub(crate) struct DuckEvent {
    pub phase: EventPhase,
    pub phase_tick: u64,
    pub tick: u64,
    pub layout: Layout,
    pub width: f32,
    pub radius: f32,
    /// Fixed entrance-side course mirroring, not the runner's current facing.
    pub direction: f32,
    pub obstacles: [Obstacle; 3],
    pub course: Option<planner::Course>,
    // Rain/Meltdown advance the authoritative actuator while active. This small
    // snapshot drives contacts/rendering; after the wet event, it settles dry.
    responsive_floor: Option<ResponsiveFloor>,
    // A joined Falling keeps its original two banks and side walls.
    pub(crate) drain_floor: Option<crate::floor::DrainGeometry>,
    spawn_motion: Option<(Vec2, Vec2)>,
    world: Option<PhysicsWorld>,
    // A physical event can lease this world across entry, dismissal and exit.
    // It owns its bodies; the visit must remove only the character on reset.
    arena_claimed: bool,
    // An existing floor must not disappear while the new entrance door opens.
    // Joining a fading event restores its opacity smoothly, not in one frame.
    entry_arena_opacity: Option<f32>,
    buoyant: Option<BuoyantBody>,
    water_report: BuoyancyReport,
    seed: u64,
    movement: Movement,
    controller: Controller,
    jumps: u32,
    outcome: Option<ClockDuckOutcome>,
    player: Option<PlayerControl>,
}

struct PlayerControl {
    session_id: u64,
    seat: u8,
    move_milli: i16,
    jump_held: bool,
    jump_pending: bool,
    facing: f32,
    exit_at_tick: u64,
}

impl PlayerControl {
    fn new(session_id: u64, seat: u8, facing: f32, exit_at_tick: u64) -> Self {
        Self {
            session_id,
            seat,
            move_milli: 0,
            jump_held: false,
            jump_pending: false,
            facing,
            exit_at_tick,
        }
    }
}

impl DuckEvent {
    pub fn new_player(
        layout: Layout,
        seed: u64,
        pattern: Option<engine_common::ClockDuckCoursePattern>,
        session_id: u64,
        seat: u8,
    ) -> Self {
        Self::with_player(Self::new_course(layout, seed, pattern), session_id, seat)
    }

    fn with_player(mut scene: Self, session_id: u64, seat: u8) -> Self {
        scene.player = Some(PlayerControl::new(
            session_id,
            seat,
            1.0,
            OPENING_TICKS + PLAYER_EXIT_DELAY_TICKS,
        ));
        scene
    }

    /// Change the command source, never the actor, course or physics clock.
    /// A resetting visit has already removed its actor and cannot be taken over.
    pub fn take_control(&mut self, session_id: u64, seat: u8) -> bool {
        if self.player.is_some() || self.phase == EventPhase::Resetting {
            return false;
        }
        // Keep an open exit open. Otherwise give the player a short delay before
        // its normal opening animation, without changing the current phase.
        let exit_at_tick = (self.tick + PLAYER_EXIT_DELAY_TICKS).clamp(
            OPENING_TICKS + PLAYER_EXIT_DELAY_TICKS,
            OPENING_TICKS + EXIT_DELAY_TICKS,
        );
        self.player = Some(PlayerControl::new(
            session_id,
            seat,
            self.controller.direction,
            exit_at_tick,
        ));
        if self.world.is_some() {
            self.insert_entrance_wall();
        }
        true
    }

    pub fn player_session(&self) -> Option<(u64, u8)> {
        self.player.as_ref().map(|p| (p.session_id, p.seat))
    }

    pub fn adopt_course(&mut self, geometry: &arena::CourseGeometry) {
        assert_eq!(self.tick, 0);
        assert!(self.player.is_some());
        self.layout = geometry.layout;
        self.width = geometry.width;
        self.radius = geometry.radius;
        self.direction = geometry.direction;
        self.course = Some(geometry.course.clone());
        self.movement = Movement::new(self.width, self.radius);
    }

    pub fn set_player_input(&mut self, move_milli: i16, jump: bool) {
        if self.phase == EventPhase::Resetting {
            return;
        }
        let Some(player) = &mut self.player else {
            return;
        };
        player.move_milli = move_milli.clamp(-1000, 1000);
        // No buffered landing jump or opening-phase jump. A new press must
        // happen while running; holding the button never becomes auto-hop.
        player.jump_pending |= jump
            && !player.jump_held
            && matches!(self.phase, EventPhase::Running | EventPhase::Exiting);
        player.jump_held = jump;
    }

    pub fn dismiss_player(&mut self) {
        if self.player.is_some() && self.phase != EventPhase::Resetting {
            self.reset(ClockDuckOutcome::Dismissed);
        }
    }

    pub fn player_diagnostics(&self) -> Option<engine_common::ClockPlayerDuckState> {
        let player = self.player.as_ref()?;
        Some(engine_common::ClockPlayerDuckState {
            session_id: player.session_id,
            player: player.seat,
            phase: self.phase.as_str().into(),
            phase_tick: self.phase_tick,
            move_milli: player.move_milli,
            jump_held: player.jump_held,
            facing_right: player.facing * self.direction > 0.0,
            floor_open_milli: self
                .responsive_floor
                .as_ref()
                .map(|f| (f.opening * 1000.0).round() as u32),
            submerged_milli: (self.water_report.submerged_fraction * 1000.0).round() as u32,
            velocity_milli: self
                .world
                .as_ref()
                .and_then(|w| w.motion(DUCK_BODY))
                .map(|m| {
                    [
                        (m.linear_velocity.x * 1000.0).round() as i32,
                        (m.linear_velocity.y * 1000.0).round() as i32,
                    ]
                }),
            duck: self.diagnostics(),
        })
    }

    #[cfg(test)]
    pub fn new_platforms(layout: Layout, seed: u64) -> Self {
        Self::new_course(
            layout,
            seed,
            Some(engine_common::ClockDuckCoursePattern::Platforms),
        )
    }

    pub fn new_course(
        layout: Layout,
        seed: u64,
        pattern: Option<engine_common::ClockDuckCoursePattern>,
    ) -> Self {
        let mut event = Self::new(layout, seed);
        event.fit_character();
        event.course = Some(planner::Course::varied(
            event.width,
            event.radius,
            seed,
            pattern,
        ));
        event
    }

    fn fit_character(&mut self) {
        // Preserve room for the full jump arc below the clock on very wide
        // displays. Standard device layouts keep the existing duck size.
        self.radius = self
            .radius
            .min((self.layout.face_origin.y - self.layout.floor_y - 4.0) / 10.5);
        self.movement = Movement::new(self.width, self.radius);
    }

    pub fn new(layout: Layout, seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let width = layout.bounds_max.x - layout.bounds_min.x;
        let radius = (width / 100.0).min(8.0);
        let mut hurdle = |fraction: f32| {
            let start = width * (fraction + rng.random_range(-0.015..0.015));
            Obstacle {
                start,
                end: start + width * 0.018,
                height: radius * rng.random_range(0.85..1.25),
            }
        };
        let first = hurdle(0.27);
        let last = hurdle(0.73);
        let pit_start = width * rng.random_range(0.465..0.48);
        Self {
            phase: EventPhase::Opening,
            phase_tick: 0,
            tick: 0,
            layout,
            width,
            radius,
            direction: if rng.random_bool(0.5) { 1.0 } else { -1.0 },
            obstacles: [
                first,
                Obstacle {
                    start: pit_start,
                    end: pit_start + width * rng.random_range(0.045..0.05),
                    height: 0.0,
                },
                last,
            ],
            world: None,
            arena_claimed: false,
            entry_arena_opacity: None,
            buoyant: None,
            water_report: BuoyancyReport::default(),
            course: None,
            responsive_floor: None,
            drain_floor: None,
            spawn_motion: None,
            seed,
            movement: Movement::new(width, radius),
            controller: Controller::new(),
            jumps: 0,
            outcome: None,
            player: None,
        }
    }

    fn ensure_world(&mut self) {
        if self.world.is_some() {
            return;
        }
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -self.movement.gravity),
            length_unit: self.radius,
            max_ccd_substeps: 2,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        // One dynamic body; all course surfaces are fixed, bounded geometry.
        let count = self
            .course
            .as_ref()
            .map_or(5, |course| course.surfaces.len() + 1);
        let count = if self.responsive_floor.is_some() {
            4
        } else {
            count + usize::from(self.player.is_some())
        };
        world.reserve(count, count, 0);
        let pit = self.obstacles[1];
        let floor = self.layout.floor_y;
        let geometry = if let Some(responsive) = &self.responsive_floor {
            responsive.insert_panels(&mut world);
            vec![]
        } else if let Some(course) = &self.course {
            course
                .surfaces
                .iter()
                .map(|surface| {
                    (
                        surface.start,
                        surface.end,
                        self.layout.bounds_min.y,
                        floor + surface.height,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            vec![
                (
                    -self.radius * 8.0,
                    pit.start,
                    self.layout.bounds_min.y,
                    floor,
                ),
                (
                    pit.end,
                    self.width + self.radius * 8.0,
                    self.layout.bounds_min.y,
                    floor,
                ),
                (
                    self.obstacles[0].start,
                    self.obstacles[0].end,
                    floor,
                    floor + self.obstacles[0].height,
                ),
                (
                    self.obstacles[2].start,
                    self.obstacles[2].end,
                    floor,
                    floor + self.obstacles[2].height,
                ),
            ]
        };
        for (index, (start, end, bottom, top)) in geometry.into_iter().enumerate() {
            let entity =
                PhysicsId::new(if self.course.is_some() { 1000 } else { 100 } + index as u64);
            let mut collider = ColliderSpec::cuboid(
                ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                (end - start) * 0.5,
                (top - bottom) * 0.5,
            );
            collider.friction = 0.0;
            collider.restitution = 0.0;
            assert!(world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position:
                        self.physics_position(Vec2::new((start + end) * 0.5, (bottom + top) * 0.5)),
                    ..BodySpec::default()
                },
                &[collider],
            ));
        }
        self.world = Some(world);
        if self.player.is_some() {
            self.insert_entrance_wall();
        }
    }

    fn insert_entrance_wall(&mut self) {
        // A physical screen-edge wall replaces the bot's turnaround rule.
        // The far end remains open so the player can walk through the exit.
        let entity = PhysicsId::new(2000);
        let height = self.layout.bounds_max.y - self.layout.bounds_min.y;
        let position = self.physics_position(Vec2::new(
            -self.radius,
            self.layout.bounds_min.y + height * 0.5,
        ));
        let mut wall = ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            self.radius,
            height * 0.5,
        );
        wall.friction = 0.0;
        assert!(self.world.as_mut().unwrap().insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position,
                ..BodySpec::default()
            },
            &[wall],
        ));
    }

    fn spawn(&mut self) {
        self.ensure_world();
        let floor = self.layout.floor_y;
        let (position, linear_velocity) = self.spawn_motion.take().unwrap_or_else(|| {
            (
                self.physics_position(Vec2::new(self.radius * 6.0, floor + self.radius * 1.05)),
                Vec2::ZERO,
            )
        });
        let world = self.world.as_mut().unwrap();
        let spec = BodySpec {
            position,
            linear_velocity,
            can_sleep: false,
            ccd_enabled: true,
            ..BodySpec::default()
        };
        // Both command sources use the same hull and mass. Taking over never
        // replaces a collider or changes density; dry bots do no water queries.
        self.buoyant = Some(
            BuoyantBody::insert_with_material(
                world,
                DUCK_ENTITY,
                spec,
                HullShape::Circle {
                    radius: self.radius,
                },
                BuoyantMaterial {
                    density: DUCK_DENSITY,
                    friction: 0.0,
                    restitution: 0.0,
                },
            )
            .expect("one bounded duck hull"),
        );
    }

    // Physics always uses screen/world coordinates, regardless of who supplies
    // commands. Planning and movement observations stay entrance-relative.
    fn physics_position(&self, p: Vec2) -> Vec2 {
        self.render_position(p)
    }

    pub fn position(&self) -> Option<Vec2> {
        self.world.as_ref()?.motion(DUCK_BODY).map(|motion| {
            Vec2::new(
                motion.position.x * self.direction + self.width * 0.5,
                motion.position.y,
            )
        })
    }

    pub fn grounded(&self) -> bool {
        self.world.as_ref().is_some_and(|world| {
            world
                .surface_contacts(DUCK_COLLIDER)
                .any(|contact| contact.normal.y > 0.7 && contact.separation <= self.radius * 0.05)
        })
    }

    fn supported_surface(&self) -> Option<usize> {
        let world = self.world.as_ref()?;
        let course = self.course.as_ref()?;
        world.surface_contacts(DUCK_COLLIDER).find_map(|contact| {
            if contact.normal.y > 0.7 && contact.separation <= self.radius * 0.05 {
                let index = contact.collider.entity.value().checked_sub(1000)? as usize;
                (index < course.surfaces.len()).then_some(index)
            } else {
                None
            }
        })
    }

    fn run(&mut self, water: Option<&WaterWorld>) {
        let grounded = self.grounded();
        let support = self.supported_surface();
        let exit_visible = self.exit_visible();
        let Some(world) = &mut self.world else { return };
        let motion = world.motion(DUCK_BODY).expect("live duck body");
        let screen = self.direction;
        let support_velocity = if grounded {
            world
                .surface_contacts(DUCK_COLLIDER)
                .find(|c| c.normal.y > 0.7 && c.separation <= self.radius * 0.05)
                .map_or(Vec2::ZERO, |c| c.velocity)
        } else {
            Vec2::ZERO
        };
        let observed = Observation {
            position: Vec2::new(
                motion.position.x * screen + self.width * 0.5,
                motion.position.y,
            ),
            velocity: Vec2::new(
                (motion.linear_velocity.x - support_velocity.x) * screen,
                motion.linear_velocity.y - support_velocity.y,
            ),
            grounded,
            blocked: world.surface_contacts(DUCK_COLLIDER).any(|contact| {
                (contact.normal.x.abs() > 0.3 || contact.normal.y < -0.3)
                    && contact.separation <= self.radius * 0.05
            }),
            support,
        };
        let command = if let Some(player) = &mut self.player {
            // Translate screen input into the controller's entrance-relative convention.
            let axis = f32::from(player.move_milli) / 1000.0 * self.direction;
            if axis != 0.0 {
                player.facing = axis.signum();
            }
            Command {
                gait: Gait::Pace(axis.abs()),
                direction: axis.signum(),
                jump: std::mem::take(&mut player.jump_pending),
            }
        } else if let Some(course) = &self.course {
            self.controller.decide_course(
                observed,
                CourseContext {
                    course,
                    obstacles: &self.obstacles,
                    width: self.width,
                    radius: self.radius,
                    floor: self.layout.floor_y,
                },
                exit_visible,
            )
        } else {
            self.controller.decide(
                observed,
                &self.obstacles,
                self.width,
                self.radius,
                exit_visible,
            )
        };
        world.clear_forces();
        self.water_report = match (&self.buoyant, water) {
            (Some(hull), Some(water)) => hull
                .apply_forces(world, water, BuoyancyConfig::default(), f64::from(DT))
                .expect("player hull and non-overlapping course water"),
            _ => BuoyancyReport::default(),
        };
        let mut delta = self.movement.velocity_delta(observed, &command);
        if self.water_report.submerged_fraction > 0.05 && !grounded {
            // In water, input supplies a bounded paddling acceleration instead
            // of cancelling flow with a zero-velocity target. Neutral drifts.
            let axis = self
                .player
                .as_ref()
                .map_or(0.0, |p| f32::from(p.move_milli) / 1000.0)
                * screen;
            delta.x = axis * self.movement.run_speed * DT * PADDLE_ACCELERATION;
        }
        delta.x *= screen;
        if command.jump && grounded {
            self.jumps += 1;
        }
        world.apply_velocity_delta(DUCK_BODY, delta, true);
        world.step(DT);
    }

    fn enter(&mut self, phase: EventPhase) {
        self.phase = phase;
        self.phase_tick = 0;
    }

    fn reset(&mut self, outcome: ClockDuckOutcome) {
        self.outcome = Some(outcome);
        if self.arena_claimed {
            if let Some(world) = &mut self.world {
                world.remove_entity(DUCK_ENTITY);
            }
        } else {
            self.world = None;
        }
        self.buoyant = None;
        self.water_report = BuoyancyReport::default();
        if let Some(player) = &mut self.player {
            player.move_milli = 0;
            player.jump_held = false;
            player.jump_pending = false;
        }
        self.enter(EventPhase::Resetting);
    }

    pub fn step(&mut self) -> bool {
        self.step_with_water(None)
    }

    pub fn step_with_water(&mut self, water: Option<&WaterWorld>) -> bool {
        self.step_with_environment(water, water.is_some())
    }

    pub fn step_with_environment(
        &mut self,
        water: Option<&WaterWorld>,
        panels_advanced: bool,
    ) -> bool {
        self.advance_panels(panels_advanced);
        // Before the door opens and after the character leaves, an event's
        // bodies still advance exactly once. Running owns its step in run().
        if self.arena_claimed && !matches!(self.phase, EventPhase::Running | EventPhase::Exiting) {
            self.world.as_mut().expect("leased arena").step(DT);
        }
        self.tick += 1;
        self.phase_tick += 1;
        match self.phase {
            EventPhase::Opening if self.phase_tick >= OPENING_TICKS => {
                if self.responsive_floor.is_some() {
                    self.prepare_responsive_spawn(water);
                }
                self.spawn();
                self.enter(EventPhase::Running);
            }
            EventPhase::Running | EventPhase::Exiting => {
                if self.phase == EventPhase::Running && self.exit_visible() {
                    self.enter(EventPhase::Exiting);
                }
                self.run(water);
                let position = self.position().expect("running duck");
                if !position.x.is_finite()
                    || !position.y.is_finite()
                    || position.y
                        < if self.responsive_floor.is_some() || self.drain_floor.is_some() {
                            self.layout.bounds_min.y - self.radius
                        } else {
                            self.layout.floor_y - self.radius * 5.0
                        }
                {
                    self.reset(ClockDuckOutcome::Fell);
                } else if self.exit_visible()
                    && position.x
                        > self.width
                            + self.radius
                                * if self.drain_floor.is_some() {
                                    -2.0
                                } else {
                                    2.0
                                }
                {
                    self.reset(ClockDuckOutcome::Exited);
                } else if self.player.is_none() && self.tick >= DUCK_TICKS - RESET_TICKS {
                    self.reset(ClockDuckOutcome::TimedOut);
                }
            }
            _ => {}
        }
        // Keep the catalog's fixed envelope even after an early recovery. The
        // course fades out, physics is already dropped, and live time continues.
        if self.player.is_some() {
            self.player_finished()
        } else {
            self.tick >= DUCK_TICKS
        }
    }

    pub fn course_opacity(&self) -> f32 {
        match self.phase {
            EventPhase::Opening => self.phase_tick as f32 / OPENING_TICKS as f32,
            EventPhase::Resetting => (1.0 - self.phase_tick as f32 / RESET_TICKS as f32).max(0.0),
            _ => 1.0,
        }
    }

    pub fn entrance_visible(&self) -> bool {
        // Finish closing behind the spawned duck, then keep this door hidden
        // for the rest of the visit, including return trips to the entrance wall.
        self.phase == EventPhase::Opening
            || (self.phase == EventPhase::Running && self.door_openness().0 > 0.0)
    }

    pub fn exit_visible(&self) -> bool {
        self.tick
            >= self
                .player
                .as_ref()
                .map_or(OPENING_TICKS + EXIT_DELAY_TICKS, |p| p.exit_at_tick)
    }

    /// Character facing is independent of the entrance-side course transform.
    pub fn facing(&self) -> f32 {
        self.player
            .as_ref()
            .map_or(self.controller.direction, |p| p.facing)
    }

    pub fn debug_arc(&self) -> Option<[Vec2; 25]> {
        if self.player.is_some() {
            return None;
        }
        let plan = self.controller.navigator.plan?;
        let capabilities = self.controller.capabilities()?;
        Some(std::array::from_fn(|i| {
            plan.sample(capabilities, i as f32 / 24.0)
                + Vec2::new(0.0, self.layout.floor_y + self.radius)
        }))
    }

    pub fn door_openness(&self) -> (f32, f32) {
        match self.phase {
            EventPhase::Opening => (self.phase_tick as f32 / OPENING_TICKS as f32, 0.0),
            EventPhase::Running => (
                (1.0 - self.phase_tick.saturating_sub(20) as f32 / 24.0).max(0.0),
                0.0,
            ),
            EventPhase::Exiting => (0.0, (self.phase_tick as f32 / 24.0).min(1.0)),
            EventPhase::Resetting => (
                0.0,
                if self.outcome == Some(ClockDuckOutcome::Exited) {
                    (1.0 - self.phase_tick as f32 / RESET_TICKS as f32).max(0.0)
                } else {
                    0.0
                },
            ),
            _ => (0.0, 0.0),
        }
    }

    /// Convert entrance-relative course/planner coordinates to the shared world.
    pub fn render_position(&self, position: Vec2) -> Vec2 {
        Vec2::new((position.x - self.width * 0.5) * self.direction, position.y)
    }

    pub fn physics_counts(&self) -> (usize, usize) {
        self.world
            .as_ref()
            .map_or((0, 0), |world| (world.body_count(), world.collider_count()))
    }

    pub fn diagnostics(&self) -> ClockDuckState {
        let (entrance, exit) = self.door_openness();
        ClockDuckState {
            left_to_right: self.direction > 0.0,
            position_milli: self.position().map(|p| {
                let p = self.render_position(p);
                [(p.x * 1000.0).round() as i32, (p.y * 1000.0).round() as i32]
            }),
            grounded: self.grounded(),
            jumps: self.jumps,
            cleared_obstacles: self.controller.cleared,
            obstacle_count: if self.responsive_floor.is_some() {
                0
            } else {
                self.course
                    .as_ref()
                    .map_or(self.obstacles.len(), |course| course.surfaces.len() - 1)
            },
            entrance_open_milli: (entrance * 1000.0).round() as u32,
            exit_open_milli: (exit * 1000.0).round() as u32,
            outcome: self.outcome,
            navigation: self.player.is_none().then(|| ClockDuckNavigationState {
                jump_profile: self.controller.profile,
                course_seed: self.seed,
                behavior: self.controller.behavior,
                facing_right: self.controller.direction * self.direction > 0.0,
                wall_tags: if self.direction > 0.0 {
                    self.controller.wall_tags
                } else {
                    [self.controller.wall_tags[1], self.controller.wall_tags[0]]
                },
                calibrated_jumps: self.controller.heights.count,
                speed_samples: self.controller.speeds.count,
                jump_height_milli: self
                    .controller
                    .heights
                    .median()
                    .map(|h| (h * 1000.0).round() as u32),
                flight_ticks: self
                    .controller
                    .flight_ticks
                    .median()
                    .map(|t| t.round() as u32),
                run_speed_milli: self
                    .controller
                    .speeds
                    .median()
                    .map(|s| (s * 1000.0).round() as u32),
                target_obstacle: self.controller.target_obstacle,
                spawned_ticks: self.tick.saturating_sub(OPENING_TICKS),
                exit_visible: self.exit_visible(),
                body_radius_milli: (self.radius * 1000.0).round() as u32,
                planning: self.course.as_ref().map(|course| {
                    use engine_common::{
                        ClockDuckPlanState, ClockDuckPlanningState, ClockDuckRejection,
                    };
                    let navigator = &self.controller.navigator;
                    let point = |position: Vec2| {
                        let position =
                            self.render_position(position + Vec2::new(0.0, self.layout.floor_y));
                        [
                            (position.x * 1000.0).round() as i32,
                            (position.y * 1000.0).round() as i32,
                        ]
                    };
                    ClockDuckPlanningState {
                        pattern: course.pattern,
                        surface_count: course.surfaces.len(),
                        support: self.supported_surface(),
                        plan: navigator.plan.map(|plan| ClockDuckPlanState {
                            source: plan.source,
                            target: plan.target,
                            takeoff_milli: point(plan.takeoff),
                            landing_milli: point(plan.landing),
                            flight_ticks: (plan.flight / DT).round() as u32,
                            cruise_speed_milli: (plan.cruise * 1000.0).round() as u32,
                            running_takeoff: plan.running,
                            next_target: plan.next_target,
                        }),
                        confirmed_landings: navigator.confirmed,
                        undershoots: navigator.undershoots,
                        overshoots: navigator.overshoots,
                        wrong_surface_landings: navigator.wrong_surface,
                        rejected_plans: navigator.rejected,
                        rejection: navigator.reason.map(|reason| match reason {
                            planner::Rejection::Narrow => ClockDuckRejection::TooNarrow,
                            planner::Rejection::High => ClockDuckRejection::TooHigh,
                            planner::Rejection::Range => ClockDuckRejection::OutOfRange,
                            planner::Rejection::Obstructed => ClockDuckRejection::Obstructed,
                        }),
                        acceleration_milli: self
                            .controller
                            .accelerations
                            .median()
                            .map(|a| (a * 1000.0).round() as u32),
                        generation_attempts: course.attempts,
                        fallback_course: course.fallback,
                        running_jumps: navigator.running_jumps,
                        flowing_fallbacks: navigator.flowing_fallbacks,
                        moving_landings: navigator.moving_landings,
                        skipped_platforms: navigator.skipped_platforms,
                    }
                }),
            }),
        }
    }

    pub fn select_jump_profile(&mut self, profile: Option<engine_common::ClockDuckJumpProfile>) {
        assert_eq!(
            self.tick, 0,
            "personality is selected only at event creation"
        );
        // A separate seeded stream keeps personality independent of course
        // generation, entrance side, and the automatic event schedule.
        self.controller.profile = profile.unwrap_or_else(|| {
            let mut rng = StdRng::seed_from_u64(self.seed ^ 0x4455_434b_5354_594c);
            if rng.random_bool(0.5) {
                engine_common::ClockDuckJumpProfile::Careful
            } else {
                engine_common::ClockDuckJumpProfile::Flowing
            }
        });
    }
}
