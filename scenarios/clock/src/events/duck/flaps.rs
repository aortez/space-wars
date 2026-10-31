//! Rigid, visit-owned door panels. Rendering and contacts use the same pose.
use super::*;
use engine_common::{ClockDuckFlapHold, ClockDuckFlapState};

#[derive(Debug, Clone, Copy)]
pub(crate) struct DoorPanel {
    pub bottom: f32,
    pub top: f32,
    pub hinge: Vec2,
    pub angle: f32,
    pub width: f32,
    pub inward: f32,
}

impl DoorPanel {
    pub fn point(self, point: Vec2) -> Vec2 {
        self.hinge + (point - self.hinge).rotate_radians(self.angle)
    }

    pub fn center(self) -> Vec2 {
        self.point(Vec2::new(
            self.hinge.x + self.inward * self.width * 0.5,
            (self.bottom + self.top) * 0.5,
        ))
    }

    pub fn half_extents(self) -> Vec2 {
        Vec2::new(self.width * 0.5, (self.top - self.bottom) * 0.5)
    }

    /// Each midpoint box is expanded by a bound on every corner's motion in
    /// that interval. Follow the solver's linear center/angular interpolation,
    /// including the small difference from an ideal continuous hinge arc.
    pub(in crate::events::duck) fn sweep_touches(
        self,
        next: Self,
        point: Vec2,
        radius: f32,
    ) -> bool {
        const INTERVALS: usize = 8;
        let center = self.center();
        let translation = next.center() - center;
        let half = self.half_extents();
        let turn = (next.angle - self.angle).abs();
        let margin = (translation.length() + half.length() * turn) / (2.0 * INTERVALS as f32);
        (0..INTERVALS).any(|i| {
            let t = (i as f32 + 0.5) / INTERVALS as f32;
            let angle = self.angle + (next.angle - self.angle) * t;
            let local = (point - center - translation * t).rotate_radians(-angle);
            Vec2::new(
                (local.x.abs() - half.x).max(0.0),
                (local.y.abs() - half.y).max(0.0),
            )
            .length()
                <= radius + margin
        })
    }
}

#[derive(Clone, Copy, Default)]
struct Flap {
    open: f32,
    requested: f32,
    panel: Option<DoorPanel>,
    hold: Option<ClockDuckFlapHold>,
}

/// A claim cannot survive retirement/rejoining of the same shared arena.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlapHandle {
    generation: u64,
    side: usize,
}

#[derive(Clone, Copy)]
struct Attachment {
    handle: FlapHandle,
    local_point: Vec2,
}

#[derive(Default)]
pub(super) struct Flaps {
    panels: [Flap; 2],
    generation: u64,
    attachment: Option<Attachment>,
}

pub(super) fn body(side: usize) -> BodyId {
    BodyId::new(
        PhysicsId::new(crate::arena::ids::canonical::FLAPS.start + side as u64),
        BodyRole::PRIMARY,
    )
}

fn collider(side: usize) -> ColliderId {
    ColliderId::new(body(side).entity, ColliderRole::PRIMARY, 0)
}

impl DuckEvent {
    pub(in crate::events::duck) fn panel_at(&self, side: usize, open: f32) -> DoorPanel {
        let local_x = if side == 0 { 0.0 } else { self.width };
        let x = self.render_position(Vec2::new(local_x, 0.0)).x;
        let bottom = self.terrain.door_floor(local_x);
        let top = bottom + self.radius * 4.25;
        let inward = if x < 0.0 { 1.0 } else { -1.0 };
        DoorPanel {
            bottom,
            top,
            hinge: Vec2::new(x, top),
            angle: inward * open * open * (3.0 - 2.0 * open) * std::f32::consts::FRAC_PI_2,
            width: self.layout.frame_width,
            inward,
        }
    }

    /// Entrance then exit, independent of the screen-side mirroring.
    pub fn door_panels(&self) -> [Option<DoorPanel>; 2] {
        self.flaps.panels.map(|flap| flap.panel)
    }

    pub fn door_openness(&self) -> (f32, f32) {
        (self.flaps.panels[0].open, self.flaps.panels[1].open)
    }

    // The shared ledge actuator registers one generation-checked local anchor.
    pub fn flap_handle(&self, side: usize) -> Option<FlapHandle> {
        self.flaps.panels.get(side)?.panel?;
        Some(FlapHandle {
            generation: self.flaps.generation,
            side,
        })
    }

    pub fn attach_to_flap(
        &mut self,
        world: &PhysicsWorld,
        handle: FlapHandle,
        local_point: Vec2,
    ) -> bool {
        let attachment = Attachment {
            handle,
            local_point,
        };
        if !self.valid_attachment(world, attachment) {
            return false;
        }
        self.flaps.attachment = Some(attachment);
        true
    }

    pub fn release_flap_attachment(&mut self) {
        self.flaps.attachment = None;
    }

    fn valid_attachment(&self, world: &PhysicsWorld, attachment: Attachment) -> bool {
        if attachment.handle.generation != self.flaps.generation
            || self.phase == EventPhase::Resetting
        {
            return false;
        }
        let Some(panel) = self
            .flaps
            .panels
            .get(attachment.handle.side)
            .and_then(|f| f.panel)
        else {
            return false;
        };
        let Some(motion) = world.motion(DUCK_BODY) else {
            return false;
        };
        let point = attachment.local_point;
        let half = panel.half_extents();
        let tolerance = self.radius * 0.05;
        let on_edge = point.x.is_finite()
            && point.y.is_finite()
            && point.x.abs() <= half.x + tolerance
            && point.y.abs() <= half.y + tolerance
            && ((point.x.abs() - half.x).abs() <= tolerance
                || (point.y.abs() - half.y).abs() <= tolerance);
        let world_point = panel.center() + point.rotate_radians(panel.angle);
        on_edge && (world_point - motion.position).length() <= self.radius * 2.5
    }

    pub(super) fn remove_flaps(&mut self, world: &mut PhysicsWorld) {
        for side in 0..2 {
            world.remove_entity(body(side).entity);
        }
        self.flaps.panels = [Flap::default(); 2];
        self.flaps.attachment = None;
        self.flaps.generation += 1;
    }

    pub(super) fn advance_flaps(&mut self, world: &mut PhysicsWorld) {
        let (entrance, exit) = self.requested_door_openness();
        if self
            .flaps
            .attachment
            .is_some_and(|a| !self.valid_attachment(world, a))
        {
            self.release_flap_attachment();
        }
        for (side, requested) in [entrance, exit].into_iter().enumerate() {
            let old = self.flaps.panels[side];
            let closing = requested < old.open;
            let mut hold = None;
            let step = if closing {
                1.0 / 24.0
            } else {
                1.0 / OPENING_TICKS as f32
            };
            // A delayed closing request resumes from the held pose, never from
            // an expired animation deadline. The exit's normal opening is 24 ticks.
            let step = if side == 1 { 1.0 / 24.0 } else { step };
            let mut open = old.open + (requested - old.open).clamp(-step, step);
            if (open - requested).abs() < 1e-6 {
                open = requested;
            }
            let previous = old.panel.unwrap_or_else(|| self.panel_at(side, old.open));
            let next = self.panel_at(side, open);
            let standing = world.surface_contacts(DUCK_COLLIDER).any(|c| {
                c.collider == collider(side)
                    && c.normal.y > 0.7
                    && c.separation <= self.radius * 0.05
            });
            if closing && standing {
                hold = Some(ClockDuckFlapHold::Standing);
            } else if closing && self.flaps.attachment.is_some_and(|a| a.handle.side == side) {
                hold = Some(ClockDuckFlapHold::Attachment);
            } else if closing
                && open != old.open
                && world
                    .motion(DUCK_BODY)
                    .is_some_and(|m| previous.sweep_touches(next, m.position, self.radius * 1.02))
            {
                hold = Some(ClockDuckFlapHold::Obstructed);
            }
            let panel = if hold.is_some() {
                open = old.open;
                previous
            } else {
                next
            };
            if open > 0.0 {
                if !world.contains_body(body(side)) {
                    let half = previous.half_extents();
                    let mut shape = ColliderSpec::cuboid(collider(side), half.x, half.y);
                    shape.friction = 0.0;
                    shape.restitution = 0.0;
                    assert!(world.insert_body(
                        body(side),
                        BodySpec {
                            kind: BodyKind::KinematicPosition,
                            position: previous.center(),
                            angle: previous.angle,
                            ..BodySpec::default()
                        },
                        &[shape]
                    ));
                }
                assert!(world.set_next_kinematic_pose(body(side), panel.center(), panel.angle));
            } else {
                world.remove_entity(body(side).entity);
            }
            self.flaps.panels[side] = Flap {
                open,
                requested,
                panel: (open > 0.0).then_some(panel),
                hold,
            };
        }
    }

    pub(super) fn flap_diagnostics(&self, world: Option<&PhysicsWorld>) -> [ClockDuckFlapState; 2] {
        std::array::from_fn(|side| {
            let flap = self.flaps.panels[side];
            let panel = flap.panel.unwrap_or_else(|| self.panel_at(side, 0.0));
            let motion = world.and_then(|w| w.motion(body(side)));
            let milli = |v: Vec2| [(v.x * 1000.0).round() as i32, (v.y * 1000.0).round() as i32];
            ClockDuckFlapState {
                generation: self.flaps.generation,
                body_id: motion.map(|_| body(side).entity.value()),
                requested_open_milli: (flap.requested * 1000.0).round() as u32,
                open_milli: (flap.open * 1000.0).round() as u32,
                hinge_milli: milli(panel.hinge),
                angle_milliradians: (panel.angle * 1000.0).round() as i32,
                velocity_milli: motion.map_or([0, 0], |m| milli(m.linear_velocity)),
                angular_velocity_milliradians: motion
                    .map_or(0, |m| (m.angular_velocity * 1000.0).round() as i32),
                hold: flap.hold,
            }
        })
    }
}
