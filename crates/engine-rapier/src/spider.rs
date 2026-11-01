//! A small spider that climbs solid supports and swings on one silk line.
//!
//! One dynamic ball is the whole body; the eight legs a renderer draws are not
//! simulated. Each tick the host passes a [`SpiderIntent`] to [`Spider::drive`]
//! before its own world step, along with which colliders the spider may hold.
//! The spider answers with bounded impulses on its own body, their reactions on
//! dynamic supports, and at most one rope tied to a real support. It never
//! steps the world itself, and a zero step freezes it where it is. It owns its
//! body's gravity scale: while its legs or reel carry it, its weight arrives
//! with their impulses rather than across the solver's substeps.
//!
//! The same intent carries an autonomous brain's requests and, later, a
//! player's, and both meet the same limits in [`SpiderSpec`].

use engine_core::Vec2;
use rapier2d::prelude::ColliderHandle;
use serde::Serialize;

use crate::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    JointId, PhysicsId, PhysicsWorld, RayCastOptions, RayHit,
};

mod grip;
mod silk;
#[cfg(test)]
mod tests;

const BODY_ROLE: BodyRole = BodyRole::new(40);
const COLLIDER_ROLE: ColliderRole = ColliderRole::new(40);
/// Each line tied takes a fresh joint role from here up.
const FIRST_LINE_ROLE: u32 = 40;

/// Separation, in radii, at which a foot still counts as touching.
const TOUCH: f32 = 0.05;
/// The steady pull, in body weights, that keeps feet on a held surface.
const PRESS: f32 = 0.25;
/// How quickly a held surface within reach is closed with, in seconds.
const CLOSE_SECONDS: f32 = 0.1;
/// Share of a velocity error the legs take out in one tick.
const RESPONSE: f32 = 0.5;
/// How long a spider that lost or let slip its hold waits before catching
/// again, in seconds, so a hold at its limit does not flicker.
const REARM_SECONDS: f32 = 0.15;
/// A surface ahead is one whose normal opposes the crawl at least this much.
const AHEAD: f32 = -0.5;
/// A line within this many radii of its length is taut.
const TAUT: f32 = 0.02;
/// How near its anchor, in radii, the anchor's own surface may stop a span.
const SPAN_MARGIN: f32 = 0.5;
/// Colliders examined for a surface to hold per tick.
pub const MAX_SURFACES: u32 = 8;
/// The most rays one tick casts: the line's span, a shot's arrival and a new
/// shot, and a look at each surface examined that the body does not touch.
pub const MAX_RAYS: u32 = 3 + MAX_SURFACES;

/// The spider's body and the limits it moves within. Distances are in body
/// radii, speeds in radii per second and forces in body weights, a weight
/// being the body's mass under `gravity`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpiderSpec {
    /// In world units.
    pub radius: f32,
    pub density: f32,
    /// The acceleration a body weight is measured under, in world units: the
    /// gravity the spider is built for. Its limits keep their size when the
    /// world's own gravity changes.
    pub gravity: f32,
    pub collision_groups: CollisionGroups,
    /// The most the legs pull the body toward a held surface.
    pub grip: f32,
    /// The most the legs push the body along a held surface.
    pub drive: f32,
    pub crawl_speed: f32,
    /// How far past the body a leg finds a surface.
    pub reach: f32,
    /// How far a silk shot reaches from the body's center.
    pub silk_range: f32,
    /// How fast a shot's line extends to its anchor.
    pub shot_speed: f32,
    /// The shortest and longest line, from the body's center to its anchor.
    pub min_line: f32,
    pub max_line: f32,
    /// Reeling in: the top speed up the line, and the most the reel pulls.
    pub reel_speed: f32,
    pub reel_pull: f32,
    /// The top speed letting line out.
    pub pay_out_speed: f32,
}

impl SpiderSpec {
    /// The default spider of the given size under the given gravity.
    pub fn new(radius: f32, gravity: f32) -> Self {
        Self {
            radius,
            density: 1.0,
            gravity,
            collision_groups: CollisionGroups::ALL,
            grip: 3.0,
            drive: 2.0,
            crawl_speed: 5.0,
            reach: 0.5,
            silk_range: 40.0,
            shot_speed: 160.0,
            min_line: 1.25,
            max_line: 60.0,
            reel_speed: 6.0,
            reel_pull: 2.5,
            pay_out_speed: 10.0,
        }
    }

    fn valid(&self) -> bool {
        let positive = [
            self.radius,
            self.density,
            self.gravity,
            self.grip,
            self.drive,
            self.crawl_speed,
            self.reach,
            self.silk_range,
            self.shot_speed,
            self.min_line,
            self.max_line,
            self.reel_speed,
            self.reel_pull,
            self.pay_out_speed,
        ];
        positive
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
            && self.min_line >= 1.0
            && self.min_line < self.max_line
    }

    fn weight_acceleration(&self, weights: f32) -> f32 {
        weights * self.gravity
    }
}

/// What the spider is asked to do for one tick. Out-of-range values are
/// clamped and non-finite ones ignored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct SpiderIntent {
    /// Crawl along the held surface, from -1 to 1 of the crawl speed. Positive
    /// keeps the surface on the spider's right: rightward across a floor, up a
    /// wall to its right, leftward along a ceiling.
    pub crawl: f32,
    /// Keep hold of the surface held, or catch one within reach. False lets go.
    pub grip: bool,
    /// Shoot a line this way. Its length is ignored. The new line replaces the
    /// one held when it ties, not before.
    pub shoot: Option<Vec2>,
    /// Cut the line held and any shot still extending.
    pub cut: bool,
    /// Reel the line, from -1 to 1: positive climbs it at up to the reel speed,
    /// negative lets it out at up to the pay-out speed.
    pub reel: f32,
}

impl SpiderIntent {
    fn sanitized(self) -> Self {
        let unit = |value: f32| {
            if value.is_finite() {
                value.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        };
        Self {
            crawl: unit(self.crawl),
            grip: self.grip,
            shoot: self
                .shoot
                .filter(|aim| aim.x.is_finite() && aim.y.is_finite() && aim.length_squared() > 0.0)
                .map(Vec2::normalized),
            cut: self.cut,
            reel: unit(self.reel),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpiderMode {
    /// Neither holding a surface nor hanging from a taut line.
    #[default]
    Free,
    /// Holding a surface, whatever its line is doing.
    Crawling,
    /// Holding no surface, on a taut line.
    Hanging,
}

/// What a spider let go of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpiderHold {
    Surface,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpiderReleaseReason {
    /// The intent let go: grip off, or the line cut.
    Requested,
    /// No part of the held surface is within reach any longer.
    OutOfReach,
    /// The surface or anchor left the world, or its identity now names
    /// something new.
    Removed,
    /// The caller no longer lets the spider hold it.
    Ineligible,
    /// Holding on would take more grip than the legs have, or the body
    /// parted from the surface while they pulled at their limit.
    Overloaded,
    /// Something solid came between the spider and its anchor.
    Obstructed,
    /// A new line tied.
    Replaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SpiderRelease {
    pub hold: SpiderHold,
    pub reason: SpiderReleaseReason,
}

/// Why a silk shot did not tie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SilkFailure {
    /// Nothing solid within the silk's range that way.
    NoTarget,
    /// The first solid that way may not hold silk.
    Blocked,
    /// A shot was already extending.
    Busy,
    /// The target left the world or was replaced before the line reached it.
    TargetLost,
    /// Something solid came between before the line reached it.
    Obstructed,
    /// The spider moved too far away before the line reached it.
    OutOfRange,
    /// The intent cut it, or the spider left the world, before it tied.
    Cut,
}

/// Running totals over the spider's life. Every shot ends tied or failed
/// unless it is still extending, and every line tied is retired once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SpiderCounts {
    pub catches: u64,
    pub surface_releases: u64,
    pub shots: u64,
    pub shot_failures: u64,
    pub lines_tied: u64,
    pub lines_retired: u64,
}

/// Queries the last tick made, within [`MAX_RAYS`] and [`MAX_SURFACES`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SpiderWork {
    pub rays: u32,
    /// Colliders measured for a surface to hold.
    pub surfaces: u32,
}

/// The surface held, where it is now.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SpiderSupport {
    pub collider: ColliderId,
    pub body: BodyId,
    /// On the support, in its body's own frame.
    pub local_point: Vec2,
    pub local_normal: Vec2,
    /// The same in world coordinates; the normal points out of the surface.
    pub point: Vec2,
    pub normal: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SpiderLine {
    pub joint: JointId,
    pub collider: ColliderId,
    pub body: BodyId,
    /// Where it is tied, in the anchor body's own frame and in the world.
    pub local_anchor: Vec2,
    pub anchor: Vec2,
    pub length: f32,
    /// From the spider's center to the anchor.
    pub distance: f32,
    pub slack: f32,
    /// Holding the spider, or being reeled in.
    pub taut: bool,
    /// What the line carried on the last tick, as a force: the rope's own
    /// load at the end of the step, estimated from its last solver substep,
    /// and the reel's pull.
    pub tension: f32,
}

/// A shot's line on its way to the anchor it will tie to: the cue to draw.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SpiderShot {
    pub collider: ColliderId,
    pub body: BodyId,
    pub local_target: Vec2,
    pub target: Vec2,
    /// How far the line's end has come from the spider's center.
    pub tip: Vec2,
}

/// Everything a host, a brain or a renderer reads about a spider. Reading it
/// changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SpiderState {
    pub mode: SpiderMode,
    pub position: Vec2,
    pub velocity: Vec2,
    /// The way the spider's back faces: away from its surface, up its taut
    /// line, or against gravity.
    pub up: Vec2,
    pub radius: f32,
    pub support: Option<SpiderSupport>,
    pub line: Option<SpiderLine>,
    pub shot: Option<SpiderShot>,
    /// The intent the last tick acted on, after clamping.
    pub intent: SpiderIntent,
    pub last_release: Option<SpiderRelease>,
    pub last_failure: Option<SilkFailure>,
    pub counts: SpiderCounts,
    pub work: SpiderWork,
    /// What the spider owns in the world: one body, one collider, and its line.
    pub bodies: usize,
    pub colliders: usize,
    pub joints: usize,
}

/// A point on one particular collider, kept in its body's frame so it moves
/// with the body. The handle tells a replacement under the same identity apart.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Surface {
    collider: ColliderId,
    body: BodyId,
    handle: ColliderHandle,
    local_point: Vec2,
    local_normal: Vec2,
}

impl Surface {
    fn from_hit(world: &PhysicsWorld, hit: &RayHit) -> Option<Self> {
        let body = world.collider_body(hit.collider)?;
        Some(Self {
            collider: hit.collider,
            body,
            handle: world.collider_handle(hit.collider)?,
            local_point: world.world_point_to_body(body, hit.point)?,
            local_normal: world.world_vector_to_body(body, hit.normal)?,
        })
    }

    /// Still the same collider on the same body.
    fn present(&self, world: &PhysicsWorld) -> bool {
        world.collider_handle(self.collider) == Some(self.handle)
            && world.collider_body(self.collider) == Some(self.body)
    }

    fn point(&self, world: &PhysicsWorld) -> Option<Vec2> {
        world.body_point_to_world(self.body, self.local_point)
    }

    fn normal(&self, world: &PhysicsWorld) -> Option<Vec2> {
        world.body_vector_to_world(self.body, self.local_normal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Line {
    joint: JointId,
    anchor: Surface,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Shot {
    target: Surface,
    travelled: f32,
}

/// One spider in a world it does not own.
#[derive(Debug, Clone, PartialEq)]
pub struct Spider {
    entity: PhysicsId,
    spec: SpiderSpec,
    support: Option<Surface>,
    line: Option<Line>,
    shot: Option<Shot>,
    /// Whether the last tick's hold needed more pull than the grip has.
    straining: bool,
    /// The force the reel pulled the line in with on the last tick.
    pull: f32,
    /// Whether the legs or the reel carried the spider's weight this tick.
    carried: bool,
    rearm: f32,
    next_line_role: u32,
    intent: SpiderIntent,
    last_release: Option<SpiderRelease>,
    last_failure: Option<SilkFailure>,
    counts: SpiderCounts,
    work: SpiderWork,
}

impl Spider {
    /// Add a spider's body at `position`, at rest and free. Refused when the
    /// entity already exists, the spec is malformed, or the body would start
    /// inside something solid.
    pub fn insert(
        world: &mut PhysicsWorld,
        entity: PhysicsId,
        position: Vec2,
        spec: SpiderSpec,
    ) -> Option<Self> {
        if world.contains_entity(entity)
            || !spec.valid()
            || !position.x.is_finite()
            || !position.y.is_finite()
            || !world.capsule_is_clear(position, 0.0, 0.0, spec.radius, spec.collision_groups)
        {
            return None;
        }
        let mut collider =
            ColliderSpec::ball(ColliderId::new(entity, COLLIDER_ROLE, 0), spec.radius);
        collider.density = spec.density;
        collider.friction = 0.0;
        collider.collision_groups = spec.collision_groups;
        collider.solver_groups = spec.collision_groups;
        let inserted = world.insert_body(
            BodyId::new(entity, BODY_ROLE),
            BodySpec {
                position,
                lock_rotation: true,
                can_sleep: false,
                ccd_enabled: true,
                ..BodySpec::default()
            },
            &[collider],
        );
        if !inserted {
            return None;
        }
        // The legs supply all traction, so the pair takes none from friction,
        // whatever the other surface's own friction is.
        let handle = world.collider_handle(ColliderId::new(entity, COLLIDER_ROLE, 0))?;
        world
            .raw
            .colliders
            .get_mut(handle)?
            .set_friction_combine_rule(rapier2d::prelude::CoefficientCombineRule::Min);
        Some(Self {
            entity,
            spec,
            support: None,
            line: None,
            shot: None,
            straining: false,
            pull: 0.0,
            carried: false,
            rearm: 0.0,
            next_line_role: FIRST_LINE_ROLE,
            intent: SpiderIntent::default(),
            last_release: None,
            last_failure: None,
            counts: SpiderCounts::default(),
            work: SpiderWork::default(),
        })
    }

    pub fn entity(&self) -> PhysicsId {
        self.entity
    }

    pub fn body(&self) -> BodyId {
        BodyId::new(self.entity, BODY_ROLE)
    }

    pub fn collider(&self) -> ColliderId {
        ColliderId::new(self.entity, COLLIDER_ROLE, 0)
    }

    pub fn spec(&self) -> &SpiderSpec {
        &self.spec
    }

    /// Act on one tick's intent ahead of the host's world step of `dt`, which
    /// must follow before the next drive. Only colliders `eligible` accepts
    /// are held or tied to; everything else stays a solid obstacle. A zero or
    /// invalid `dt` changes nothing. False when the spider's body is no longer
    /// in the world.
    pub fn drive(
        &mut self,
        world: &mut PhysicsWorld,
        dt: f32,
        intent: SpiderIntent,
        eligible: impl Fn(ColliderId) -> bool,
    ) -> bool {
        if !world.contains_body(self.body()) {
            return false;
        }
        if !dt.is_finite() || dt <= 0.0 {
            return true;
        }
        let intent = intent.sanitized();
        self.intent = intent;
        self.work = SpiderWork::default();
        self.pull = 0.0;
        self.carried = false;
        self.rearm = (self.rearm - dt).max(0.0);
        let candidates = self.touching(world, &eligible);
        self.keep_line(world, dt, &eligible);
        self.extend_shot(world, dt, &eligible);
        self.keep_hold(world, dt, &eligible, &candidates);
        // While the legs or the reel carry the spider, its weight comes with
        // their impulse, ahead of the step, so the two cancel exactly rather
        // than meeting across the solver's substeps. Free, it falls as every
        // other body does.
        world.set_gravity_scale(self.body(), if self.carried { 0.0 } else { 1.0 }, true);
        if self.carried {
            let weight = self.gravity(world) * (self.mass(world) * dt);
            world.apply_impulse(self.body(), weight, true);
        }
        true
    }

    /// The spider as the world now has it, or None once its body is gone.
    pub fn state(&self, world: &PhysicsWorld) -> Option<SpiderState> {
        let motion = world.motion(self.body())?;
        let position = motion.position;
        let support = self.support.and_then(|surface| {
            Some(SpiderSupport {
                collider: surface.collider,
                body: surface.body,
                local_point: surface.local_point,
                local_normal: surface.local_normal,
                point: surface.point(world)?,
                normal: surface.normal(world)?,
            })
        });
        let line = self.line.and_then(|line| {
            let tether = world.tether(line.joint)?;
            let distance = tether.anchor.distance_to(position);
            Some(SpiderLine {
                joint: line.joint,
                collider: line.anchor.collider,
                body: line.anchor.body,
                local_anchor: line.anchor.local_point,
                anchor: tether.anchor,
                length: tether.length,
                distance,
                slack: (tether.length - distance).max(0.0),
                // A reel pulling it in keeps the line taut, whatever span its
                // pull closes within the tick.
                taut: distance >= tether.length - TAUT * self.spec.radius || self.pull > 0.0,
                tension: tether.tension + self.pull,
            })
        });
        let shot = self.shot.and_then(|shot| {
            let target = shot.target.point(world)?;
            let span = target - position;
            let tip = position + span.normalized() * shot.travelled.min(span.length());
            Some(SpiderShot {
                collider: shot.target.collider,
                body: shot.target.body,
                local_target: shot.target.local_point,
                target,
                tip,
            })
        });
        let gravity = self.gravity(world);
        let (mode, up) = match (support, line) {
            (Some(support), _) => (SpiderMode::Crawling, support.normal),
            (None, Some(line)) if line.taut => {
                (SpiderMode::Hanging, (line.anchor - position).normalized())
            }
            _ => (
                SpiderMode::Free,
                if gravity.length_squared() > 0.0 {
                    -gravity.normalized()
                } else {
                    Vec2::Y
                },
            ),
        };
        Some(SpiderState {
            mode,
            position,
            velocity: motion.linear_velocity,
            up,
            radius: self.spec.radius,
            support,
            line,
            shot,
            intent: self.intent,
            last_release: self.last_release,
            last_failure: self.last_failure,
            counts: self.counts,
            work: self.work,
            bodies: 1,
            colliders: 1,
            joints: usize::from(line.is_some()),
        })
    }

    /// Take the spider out of the world: its body, its collider and its line,
    /// each once. Every other body and joint stays as it was. Returns the
    /// spider's final counts, which balance: its hold let go, its line
    /// retired and a shot still extending cut.
    pub fn remove(mut self, world: &mut PhysicsWorld) -> SpiderCounts {
        self.let_go(SpiderReleaseReason::Requested);
        if self.line.take().is_some() {
            self.counts.lines_retired += 1;
        }
        if self.shot.take().is_some() {
            self.fail(SilkFailure::Cut);
        }
        world.remove_entity(self.entity);
        self.counts
    }

    fn ray(
        &mut self,
        world: &PhysicsWorld,
        from: Vec2,
        toward: Vec2,
        distance: f32,
    ) -> Option<RayHit> {
        debug_assert!(self.work.rays < MAX_RAYS, "{:?}", self.work);
        self.work.rays += 1;
        world.cast_ray(
            from,
            toward,
            RayCastOptions {
                max_distance: distance,
                solid: true,
                include_sensors: false,
                collision_groups: self.spec.collision_groups,
                exclude_entity: Some(self.entity),
            },
        )
    }

    /// Whether nothing solid stands between `from` and a point `to` on
    /// `anchor`: the ray may stop short only on the anchor's own surface near
    /// the point, or on any surface right at it, such as a neighbor's across
    /// a seam.
    fn clear_span(
        &mut self,
        world: &PhysicsWorld,
        from: Vec2,
        to: Vec2,
        anchor: ColliderId,
    ) -> bool {
        let r = self.spec.radius;
        let span = to - from;
        let distance = span.length();
        if distance <= SPAN_MARGIN * r {
            return true;
        }
        match self.ray(world, from, span, distance) {
            None => true,
            Some(hit) => {
                let short = distance - hit.distance;
                short <= TOUCH * r || (hit.collider == anchor && short <= SPAN_MARGIN * r)
            }
        }
    }

    /// The spider's weight per unit mass: the world's gravity, whatever
    /// scale the spider last gave its own body.
    fn gravity(&self, world: &PhysicsWorld) -> Vec2 {
        world.gravity()
    }

    fn mass(&self, world: &PhysicsWorld) -> f32 {
        world.body_mass(self.body()).unwrap_or(0.0)
    }

    /// Deliver the reaction to an impulse the spider took from a support, at
    /// the point it took it, when the support is free to move.
    fn react(world: &mut PhysicsWorld, support: BodyId, impulse: Vec2, point: Vec2) {
        if world.body_kind(support) == Some(BodyKind::Dynamic) {
            world.apply_impulse_at_point(support, -impulse, point, true);
        }
    }
}
