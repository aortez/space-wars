//! A bounded, event-local obstacle course. The round body is physical; the
//! upright pixel character and side doors are presentation, not articulated rigs.

#[cfg(test)]
mod tests;

use engine_common::{ClockDuckOutcome, ClockDuckState};
use engine_core::Vec2;
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
    PhysicsWorld, PhysicsWorldConfig,
};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::EventPhase;
use crate::layout::Layout;

pub const DUCK_TICKS: u64 = 10 * 60;
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
    pub direction: f32,
    pub obstacles: [Obstacle; 3],
    world: Option<PhysicsWorld>,
    speed: f32,
    gravity: f32,
    jumped: u8,
    jumps: u32,
    cleared: usize,
    outcome: Option<ClockDuckOutcome>,
}

impl DuckEvent {
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
            speed: width / 7.0,
            gravity: radius * 50.0,
            jumped: 0,
            jumps: 0,
            cleared: 0,
            outcome: None,
        }
    }

    fn spawn(&mut self) {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -self.gravity),
            length_unit: self.radius,
            max_ccd_substeps: 2,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        // Two floor halves, two hurdles and exactly one dynamic body.
        world.reserve(5, 5, 0);
        let pit = self.obstacles[1];
        let floor = self.layout.floor_y;
        for (index, (start, end, bottom, top)) in [
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
        .into_iter()
        .enumerate()
        {
            let entity = PhysicsId::new(100 + index as u64);
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
                position: Vec2::new(0.0, floor + self.radius * 1.05),
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

    fn run(&mut self) {
        let grounded = self.grounded();
        let Some(world) = &mut self.world else { return };
        let motion = world.motion(DUCK_BODY).expect("live duck body");
        // A small velocity servo accelerates the runner horizontally. Gravity,
        // obstacle contacts and the jump impulse govern its vertical motion.
        let dx = (self.speed - motion.linear_velocity.x)
            .clamp(-self.speed * DT * 6.0, self.speed * DT * 6.0);
        world.apply_velocity_delta(DUCK_BODY, Vec2::new(dx, 0.0), true);
        for (index, obstacle) in self.obstacles.iter().enumerate() {
            if motion.position.x > obstacle.end + self.radius {
                self.cleared = self.cleared.max(index + 1);
            } else if grounded
                && self.jumped & (1 << index) == 0
                && motion.position.x >= obstacle.start - self.radius - self.speed * 0.12
            {
                let jump_speed = (2.0 * self.gravity * self.radius * 3.0).sqrt();
                world.apply_velocity_delta(
                    DUCK_BODY,
                    Vec2::new(0.0, jump_speed - motion.linear_velocity.y),
                    true,
                );
                self.jumped |= 1 << index;
                self.jumps += 1;
                break;
            }
        }
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
                self.run();
                let position = self.position().expect("running duck");
                if !position.x.is_finite()
                    || !position.y.is_finite()
                    || position.y < self.layout.floor_y - self.radius * 5.0
                {
                    self.reset(ClockDuckOutcome::Fell);
                } else if position.x > self.width + self.radius * 2.0 {
                    self.reset(ClockDuckOutcome::Exited);
                } else if self.tick >= DUCK_TICKS - RESET_TICKS {
                    self.reset(ClockDuckOutcome::TimedOut);
                } else if self.phase == EventPhase::Running && position.x > self.width * 0.87 {
                    self.enter(EventPhase::Exiting);
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
            cleared_obstacles: self.cleared,
            obstacle_count: self.obstacles.len(),
            entrance_open_milli: (entrance * 1000.0).round() as u32,
            exit_open_milli: (exit * 1000.0).round() as u32,
            outcome: self.outcome,
        }
    }
}
