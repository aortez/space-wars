//! A bounded, event-local obstacle course. The round body is physical; the
//! upright pixel character and side doors are presentation, not articulated rigs.

mod controller;
mod flow;
pub(crate) mod planner;
#[cfg(test)]
mod platform_tests;
#[cfg(test)]
mod tests;

use controller::{Controller, CourseContext, Movement, Observation};
use engine_common::{ClockDuckNavigationState, ClockDuckOutcome, ClockDuckState};
use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld, PhysicsWorldConfig,
};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::EventPhase;
use crate::layout::Layout;

pub const DUCK_TICKS: u64 = 35 * 60;
const EXIT_DELAY_TICKS: u64 = 20 * 60;
const OPENING_TICKS: u64 = 36;
const RESET_TICKS: u64 = 30;
const DT: f32 = 1.0 / 60.0;
const DUCK_ENTITY: PhysicsId = PhysicsId::new(1);
const DUCK_BODY: BodyId = BodyId::new(DUCK_ENTITY, BodyRole::PRIMARY);
const DUCK_COLLIDER: ColliderId = ColliderId::new(DUCK_ENTITY, ColliderRole::PRIMARY, 0);

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
    world: Option<PhysicsWorld>,
    seed: u64,
    movement: Movement,
    controller: Controller,
    jumps: u32,
    outcome: Option<ClockDuckOutcome>,
}

impl DuckEvent {
    pub fn new_platforms(layout: Layout, seed: u64) -> Self {
        let mut event = Self::new(layout, seed);
        // Preserve room for the full jump arc below the clock on very wide
        // displays. Standard device layouts keep the existing duck size.
        event.radius = event
            .radius
            .min((layout.face_origin.y - layout.floor_y - 4.0) / 10.5);
        event.movement = Movement::new(event.width, event.radius);
        event.course = Some(planner::Course::generated(event.width, event.radius, seed));
        event
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
            course: None,
            seed,
            movement: Movement::new(width, radius),
            controller: Controller::new(),
            jumps: 0,
            outcome: None,
        }
    }

    fn spawn(&mut self) {
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
        world.reserve(count, count, 0);
        let pit = self.obstacles[1];
        let floor = self.layout.floor_y;
        let geometry = if let Some(course) = &self.course {
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
                    position: Vec2::new((start + end) * 0.5, (bottom + top) * 0.5),
                    ..BodySpec::default()
                },
                &[collider],
            ));
        }
        let mut collider = ColliderSpec::ball(DUCK_COLLIDER, self.radius);
        collider.friction = 0.0;
        collider.restitution = 0.0;
        assert!(world.insert_body(
            DUCK_BODY,
            BodySpec {
                position: Vec2::new(self.radius * 6.0, floor + self.radius * 1.05),
                can_sleep: false,
                ccd_enabled: true,
                ..BodySpec::default()
            },
            &[collider],
        ));
        self.world = Some(world);
    }

    pub fn position(&self) -> Option<Vec2> {
        self.world
            .as_ref()?
            .motion(DUCK_BODY)
            .map(|motion| motion.position)
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

    fn run(&mut self) {
        let grounded = self.grounded();
        let support = self.supported_surface();
        let exit_visible = self.exit_visible();
        let Some(world) = &mut self.world else { return };
        let motion = world.motion(DUCK_BODY).expect("live duck body");
        let observed = Observation {
            position: motion.position,
            velocity: motion.linear_velocity,
            grounded,
            blocked: world.surface_contacts(DUCK_COLLIDER).any(|contact| {
                (contact.normal.x.abs() > 0.3 || contact.normal.y < -0.3)
                    && contact.separation <= self.radius * 0.05
            }),
            support,
        };
        let command = if let Some(course) = &self.course {
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
        let delta = self.movement.velocity_delta(observed, &command);
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
        self.world = None;
        self.enter(EventPhase::Resetting);
    }

    pub fn step(&mut self) -> bool {
        self.tick += 1;
        self.phase_tick += 1;
        match self.phase {
            EventPhase::Opening if self.phase_tick >= OPENING_TICKS => {
                self.spawn();
                self.enter(EventPhase::Running);
            }
            EventPhase::Running | EventPhase::Exiting => {
                if self.phase == EventPhase::Running && self.exit_visible() {
                    self.enter(EventPhase::Exiting);
                }
                self.run();
                let position = self.position().expect("running duck");
                if !position.x.is_finite()
                    || !position.y.is_finite()
                    || position.y < self.layout.floor_y - self.radius * 5.0
                {
                    self.reset(ClockDuckOutcome::Fell);
                } else if self.exit_visible() && position.x > self.width + self.radius * 2.0 {
                    self.reset(ClockDuckOutcome::Exited);
                } else if self.tick >= DUCK_TICKS - RESET_TICKS {
                    self.reset(ClockDuckOutcome::TimedOut);
                }
            }
            _ => {}
        }
        // Keep the catalog's fixed envelope even after an early recovery. The
        // course fades out, physics is already dropped, and live time continues.
        self.tick >= DUCK_TICKS
    }

    pub fn course_opacity(&self) -> f32 {
        match self.phase {
            EventPhase::Opening => self.phase_tick as f32 / OPENING_TICKS as f32,
            EventPhase::Resetting => (1.0 - self.phase_tick as f32 / RESET_TICKS as f32).max(0.0),
            _ => 1.0,
        }
    }

    pub fn exit_visible(&self) -> bool {
        self.tick >= OPENING_TICKS + EXIT_DELAY_TICKS
    }

    /// Character facing is independent of the entrance-side course transform.
    pub fn facing(&self) -> f32 {
        self.controller.direction
    }

    pub fn debug_arc(&self) -> Option<[Vec2; 25]> {
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

    /// Physics runs in entrance-to-exit coordinates; mirror the entire course
    /// for the other direction without a second controller or duplicated tuning.
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
            obstacle_count: self
                .course
                .as_ref()
                .map_or(self.obstacles.len(), |course| course.surfaces.len() - 1),
            entrance_open_milli: (entrance * 1000.0).round() as u32,
            exit_open_milli: (exit * 1000.0).round() as u32,
            outcome: self.outcome,
            navigation: Some(ClockDuckNavigationState {
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
