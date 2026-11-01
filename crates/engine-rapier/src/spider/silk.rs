//! The one silk line: shooting it, tying it to what the shot reached, reeling
//! along it, and letting it go when its anchor goes or something solid comes
//! between. A cut or replaced line leaves nothing behind.

use super::*;
use crate::world::{JointRole, TetherSpec, TetherState};

impl Spider {
    pub(super) fn keep_line(
        &mut self,
        world: &mut PhysicsWorld,
        dt: f32,
        eligible: &impl Fn(ColliderId) -> bool,
    ) {
        let Some(line) = self.line else {
            return;
        };
        let reason = if !world.contains_joint(line.joint) || !line.anchor.present(world) {
            Some(SpiderReleaseReason::Removed)
        } else if !eligible(line.anchor.collider) {
            Some(SpiderReleaseReason::Ineligible)
        } else if self.intent.cut {
            Some(SpiderReleaseReason::Requested)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.cut_line(world, reason);
            return;
        }
        let (Some(motion), Some(tether)) = (world.motion(self.body()), world.tether(line.joint))
        else {
            self.cut_line(world, SpiderReleaseReason::Removed);
            return;
        };
        if !self.clear_span(world, motion.position, tether.anchor, line.anchor.collider) {
            self.cut_line(world, SpiderReleaseReason::Obstructed);
            return;
        }
        self.reel(
            world,
            dt,
            line,
            motion.position,
            motion.linear_velocity,
            tether,
        );
    }

    /// Reeling in takes up slack for free and climbs a taut line with a
    /// bounded pull, keeping whatever span the pull closed. Letting out
    /// lengthens the line, and gravity does the rest.
    fn reel(
        &mut self,
        world: &mut PhysicsWorld,
        dt: f32,
        line: Line,
        position: Vec2,
        velocity: Vec2,
        tether: TetherState,
    ) {
        let r = self.spec.radius;
        let shortest = self.spec.min_line * r;
        let longest = self.spec.max_line * r;
        let reel = self.intent.reel;
        let distance = tether.anchor.distance_to(position);
        let mut length = tether.length;
        if reel < 0.0 {
            length = (length - reel * self.spec.pay_out_speed * r * dt).min(longest);
        } else if reel > 0.0 {
            let gather = reel * self.spec.reel_speed * r * dt;
            if distance < length - gather - TAUT * r {
                // More slack than a tick takes in: gather it, pulling nothing.
                length = (length - gather).max(shortest);
            } else {
                let inward = (tether.anchor - position) / distance;
                let anchor = world
                    .velocity_at_point(line.anchor.body, tether.anchor)
                    .unwrap_or(Vec2::ZERO);
                let climbing = (velocity - anchor).dot(inward);
                // The line keeps the span a climb closed, and only that. Never
                // shorter than its span, the rope itself holds only what
                // gravity and the swing put on it.
                if climbing > 0.0 {
                    length = length.min(distance);
                }
                length = length.max(shortest);
                let target = reel * self.spec.reel_speed * r;
                let falling = self.gravity(world).dot(inward) * dt;
                let needed = target - climbing - falling;
                let most = self.spec.weight_acceleration(self.spec.reel_pull) * dt;
                // A reel too weak to lift the spider holds it where it is, and
                // the line carries the weight.
                if distance > shortest && needed > 0.0 && most > -falling {
                    let impulse = inward * (needed.min(most) * self.mass(world));
                    world.apply_impulse(self.body(), impulse, true);
                    Self::react(world, line.anchor.body, impulse, tether.anchor);
                    self.pull = impulse.length() / dt;
                    self.carried = true;
                }
            }
        }
        if length != tether.length {
            world.set_tether_length(line.joint, length);
        }
    }

    /// Carry a shot's line toward its target, tie it on arrival, and start a
    /// new shot when one is asked for.
    pub(super) fn extend_shot(
        &mut self,
        world: &mut PhysicsWorld,
        dt: f32,
        eligible: &impl Fn(ColliderId) -> bool,
    ) {
        if self.intent.cut && self.shot.take().is_some() {
            self.fail(SilkFailure::Cut);
        }
        if let Some(shot) = self.shot.take()
            && let Err(failure) = self.arrive(world, dt, shot, eligible)
        {
            self.fail(failure);
        }
        let Some(aim) = self.intent.shoot else {
            return;
        };
        self.counts.shots += 1;
        if self.shot.is_some() {
            self.fail(SilkFailure::Busy);
            return;
        }
        let Some(center) = world.motion(self.body()).map(|motion| motion.position) else {
            return;
        };
        let range = self.spec.silk_range * self.spec.radius;
        let target = match self.ray(world, center, aim, range) {
            None => Err(SilkFailure::NoTarget),
            Some(hit) if !eligible(hit.collider) => Err(SilkFailure::Blocked),
            Some(hit) => Surface::from_hit(world, &hit).ok_or(SilkFailure::NoTarget),
        };
        match target {
            Ok(target) => {
                self.shot = Some(Shot {
                    target,
                    travelled: 0.0,
                });
            }
            Err(failure) => self.fail(failure),
        }
    }

    /// One tick of a shot's line on its way. On arrival the span is checked
    /// again from where the spider now is, and the new line replaces the old.
    fn arrive(
        &mut self,
        world: &mut PhysicsWorld,
        dt: f32,
        mut shot: Shot,
        eligible: &impl Fn(ColliderId) -> bool,
    ) -> Result<(), SilkFailure> {
        if !shot.target.present(world) {
            return Err(SilkFailure::TargetLost);
        }
        if !eligible(shot.target.collider) {
            return Err(SilkFailure::Blocked);
        }
        let (Some(target), Some(motion)) = (shot.target.point(world), world.motion(self.body()))
        else {
            return Err(SilkFailure::TargetLost);
        };
        let r = self.spec.radius;
        let distance = target.distance_to(motion.position);
        shot.travelled += self.spec.shot_speed * r * dt;
        if shot.travelled < distance {
            self.shot = Some(shot);
            return Ok(());
        }
        if distance > self.spec.max_line * r {
            return Err(SilkFailure::OutOfRange);
        }
        if !self.clear_span(world, motion.position, target, shot.target.collider) {
            return Err(SilkFailure::Obstructed);
        }
        let joint = self.fresh_joint(world);
        self.cut_line(world, SpiderReleaseReason::Replaced);
        let tied = world.insert_tether(
            joint,
            TetherSpec {
                anchor_body: shot.target.body,
                anchor: shot.target.local_point,
                body: self.body(),
                attachment: Vec2::ZERO,
                length: distance.clamp(self.spec.min_line * r, self.spec.max_line * r),
            },
        );
        if !tied {
            return Err(SilkFailure::TargetLost);
        }
        self.line = Some(Line {
            joint,
            anchor: shot.target,
        });
        self.counts.lines_tied += 1;
        Ok(())
    }

    /// A joint identity no line of this spider has used.
    fn fresh_joint(&mut self, world: &PhysicsWorld) -> JointId {
        loop {
            let joint = JointId::new(self.entity, JointRole::new(self.next_line_role));
            self.next_line_role = self
                .next_line_role
                .checked_add(1)
                .unwrap_or(FIRST_LINE_ROLE);
            if !world.contains_joint(joint) {
                return joint;
            }
        }
    }

    /// Retire the line, once: its joint leaves the world and nothing of it
    /// stays behind.
    pub(super) fn cut_line(&mut self, world: &mut PhysicsWorld, reason: SpiderReleaseReason) {
        if let Some(line) = self.line.take() {
            world.remove_joint(line.joint);
            self.counts.lines_retired += 1;
            self.last_release = Some(SpiderRelease {
                hold: SpiderHold::Line,
                reason,
            });
        }
    }

    pub(super) fn fail(&mut self, failure: SilkFailure) {
        self.counts.shot_failures += 1;
        self.last_failure = Some(failure);
    }
}
