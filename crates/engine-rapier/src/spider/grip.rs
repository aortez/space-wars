//! Holding a surface: finding one within reach, crawling along it, turning
//! onto the next one ahead, and letting go when it is lost, removed, or more
//! than the legs can hold.
//!
//! Every surface is measured afresh each tick from where the body is now: the
//! nearest point of each collider it may hold within reach, seen from the
//! body's center with nothing solid between.

use super::*;

/// An eligible surface within reach of the body, as this tick found it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Touch {
    surface: Surface,
    /// The surface's point nearest the body's center, in world coordinates.
    point: Vec2,
    /// Out of the surface, toward the body's center.
    normal: Vec2,
    /// The surface's own velocity at the point through the coming step.
    velocity: Vec2,
    /// From the body's skin to the point.
    separation: f32,
}

/// Along a surface with this normal, with the surface on the right.
fn tangent(normal: Vec2) -> Vec2 {
    Vec2::new(normal.y, -normal.x)
}

impl Spider {
    /// Eligible surfaces within reach of the body, the held one among them
    /// if it still is: of at most [`MAX_NEARBY`] colliders looked at, the
    /// nearest [`MAX_SURFACES`] the spider may hold.
    pub(super) fn touching(
        &mut self,
        world: &PhysicsWorld,
        dt: f32,
        eligible: &impl Fn(ColliderId) -> bool,
    ) -> Vec<Touch> {
        let Some(center) = world.motion(self.body()).map(|motion| motion.position) else {
            return Vec::new();
        };
        let held = self
            .support
            .map(|held| held.collider)
            .filter(|&collider| eligible(collider));
        let nearby = world.colliders_within(
            center,
            self.spec.radius * (1.0 + self.spec.reach),
            NearbyOptions {
                collision_groups: self.spec.collision_groups,
                exclude_entity: Some(self.entity),
                max_visited: MAX_NEARBY as usize,
                max_found: MAX_SURFACES as usize,
            },
            |collider| Some(collider) != held && eligible(collider),
        );
        // The held surface first, so a crowd of others never hides it.
        let colliders: Vec<ColliderId> = held
            .into_iter()
            .chain(nearby.colliders)
            .take(MAX_SURFACES as usize)
            .collect();
        self.work.nearby = nearby.visited as u32;
        self.work.surfaces = colliders.len() as u32;
        colliders
            .into_iter()
            .filter_map(|collider| self.touch(world, center, dt, collider))
            .collect()
    }

    /// A collider's surface as the body's center finds it, if it is within
    /// reach with nothing solid between.
    fn touch(
        &mut self,
        world: &PhysicsWorld,
        center: Vec2,
        dt: f32,
        collider: ColliderId,
    ) -> Option<Touch> {
        let r = self.spec.radius;
        let (point, inside) = world.nearest_boundary_point(collider, center)?;
        let offset = center - point;
        let distance = offset.length();
        if distance <= f32::EPSILON * r {
            return None;
        }
        let (normal, separation) = if inside {
            (-offset / distance, -distance - r)
        } else {
            (offset / distance, distance - r)
        };
        if separation > self.spec.reach * r {
            return None;
        }
        // Nothing fits between the body and a surface it touches; a leg
        // reaching further must find nothing solid in the way.
        if separation > TOUCH * r && !self.clear_span(world, center, point, collider) {
            return None;
        }
        let body = world.collider_body(collider)?;
        let surface = Surface {
            collider,
            body,
            handle: world.collider_handle(collider)?,
            local_point: world.world_point_to_body(body, point)?,
            local_normal: world.world_vector_to_body(body, normal)?,
        };
        Some(Touch {
            surface,
            point,
            normal,
            velocity: world.incoming_velocity_at_point(body, point, dt)?,
            separation,
        })
    }

    pub(super) fn keep_hold(
        &mut self,
        world: &mut PhysicsWorld,
        dt: f32,
        eligible: &impl Fn(ColliderId) -> bool,
        candidates: &[Touch],
    ) {
        if let Some(held) = self.support {
            let reason = if !eligible(held.collider) {
                Some(SpiderReleaseReason::Ineligible)
            } else if !self.intent.grip {
                Some(SpiderReleaseReason::Requested)
            } else {
                None
            };
            if let Some(reason) = reason {
                self.let_go(reason);
            }
        }
        let touching = TOUCH * self.spec.radius;
        let mut touch = None;
        if let Some(held) = self.support {
            touch = candidates
                .iter()
                .find(|c| c.surface.collider == held.collider)
                .copied();
            // Where the held surface falls away from under the feet, a
            // surface they touch carries the hold on: a seam between
            // colliders, or a corner two of them share.
            if touch.is_none_or(|t| t.separation > touching)
                && let Some(next) = candidates
                    .iter()
                    .filter(|c| c.separation <= touching && self.can_hold(world, c.normal))
                    .min_by(|a, b| a.separation.total_cmp(&b.separation))
            {
                touch = Some(*next);
            }
            if touch.is_none() {
                // Pulling as hard as the legs can and still parted from it.
                self.let_go(if self.straining {
                    SpiderReleaseReason::Overloaded
                } else {
                    SpiderReleaseReason::OutOfReach
                });
            }
        }
        // Crawling into a surface ahead, at an inside corner, turns onto it.
        if let Some(current) = touch
            && self.intent.crawl != 0.0
        {
            let ahead = tangent(current.normal) * self.intent.crawl.signum();
            let next = candidates
                .iter()
                .filter(|c| {
                    c.separation <= touching
                        && c.normal.dot(ahead) < AHEAD
                        && self.can_hold(world, c.normal)
                })
                .min_by(|a, b| a.normal.dot(ahead).total_cmp(&b.normal.dot(ahead)));
            if let Some(next) = next {
                touch = Some(*next);
            }
        }
        // A free spider catches the nearest surface within reach it can hold.
        if self.support.is_none() && self.intent.grip && self.rearm == 0.0 {
            touch = candidates
                .iter()
                .filter(|c| self.can_hold(world, c.normal))
                .min_by(|a, b| a.separation.total_cmp(&b.separation))
                .copied();
            if touch.is_some() {
                self.counts.catches += 1;
            }
        }
        if let Some(touch) = touch {
            self.hold(world, dt, touch);
        }
    }

    /// Whether the legs can keep the body on a surface facing this way.
    fn can_hold(&self, world: &PhysicsWorld, normal: Vec2) -> bool {
        self.pull_needed(world, normal) <= self.spec.weight_acceleration(self.spec.grip)
    }

    /// The pull toward a surface, as an acceleration, that holding still on
    /// it takes: gravity's share away from it and the steady press.
    fn pull_needed(&self, world: &PhysicsWorld, normal: Vec2) -> f32 {
        self.gravity(world).dot(normal).max(0.0) + self.spec.weight_acceleration(PRESS)
    }

    /// One tick of the legs on a held surface: close any gap, press, and
    /// crawl at the asked speed relative to the surface, each within its limit.
    fn hold(&mut self, world: &mut PhysicsWorld, dt: f32, touch: Touch) {
        if self.pull_needed(world, touch.normal) > self.spec.weight_acceleration(self.spec.grip) {
            self.support = Some(touch.surface);
            self.let_go(SpiderReleaseReason::Overloaded);
            return;
        }
        let Some(motion) = world.motion(self.body()) else {
            return;
        };
        let r = self.spec.radius;
        let gravity = self.gravity(world);
        let normal = touch.normal;
        let along = tangent(normal);
        let relative = motion.linear_velocity - touch.velocity;
        let closing = -(touch.separation - TOUCH * r).max(0.0) / CLOSE_SECONDS;
        let grip = self.spec.weight_acceleration(self.spec.grip);
        let toward = RESPONSE * (closing - relative.dot(normal)) / dt
            - gravity.dot(normal)
            - self.spec.weight_acceleration(PRESS);
        self.straining = toward < -grip;
        let toward = toward.clamp(-grip, 0.0);
        let drive = self.spec.weight_acceleration(self.spec.drive);
        let target = self.intent.crawl * self.spec.crawl_speed * r;
        let forward = (RESPONSE * (target - relative.dot(along)) / dt - gravity.dot(along))
            .clamp(-drive, drive);
        let impulse = (normal * toward + along * forward) * (self.mass(world) * dt);
        world.apply_impulse(self.body(), impulse, true);
        Self::react(world, touch.surface.body, impulse, touch.point);
        self.support = Some(touch.surface);
        self.carried = true;
    }

    pub(super) fn let_go(&mut self, reason: SpiderReleaseReason) {
        self.straining = false;
        if self.support.take().is_some() {
            self.counts.surface_releases += 1;
            self.last_release = Some(SpiderRelease {
                hold: SpiderHold::Surface,
                reason,
            });
            if reason != SpiderReleaseReason::Requested {
                self.rearm = REARM_SECONDS;
            }
        }
    }
}
