//! Explicit solid scenery for falling water. Pool beds remain receiving
//! surfaces, not infinitely deep walls: a digit ledge must still allow rain
//! underneath it. Callers opt in with the actual finite panel geometry.
use crate::{Section, WaterError, WaterWorld, finite_coordinate};
use engine_core::Vec2;

#[cfg(test)]
mod tests;

pub const MAX_SOLID_BOXES: usize = 8;
const CONTACT_SKIN: f32 = 0.0001;
const MAX_CONTACTS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolidBox {
    center: Vec2,
    half_extents: Vec2,
    axes: [Vec2; 2],
}

impl SolidBox {
    pub fn new(center: Vec2, half_extents: Vec2, angle: f32) -> Result<Self, WaterError> {
        if ![center.x, center.y, half_extents.x, half_extents.y]
            .iter()
            .all(|v| finite_coordinate(f64::from(*v)))
            || half_extents.x < 0.001
            || half_extents.y < 0.001
            || !angle.is_finite()
        {
            return Err(WaterError::InvalidGeometry);
        }
        let x = Vec2::new(1.0, 0.0).rotate_radians(angle);
        Ok(Self {
            center,
            half_extents,
            axes: [x, Vec2::new(-x.y, x.x)],
        })
    }

    /// Outward normals and offsets: a point is inside when dot(p,n) <= offset
    /// for every plane. Rendering uses the same finite shape as collision.
    pub fn planes(self) -> [(Vec2, f32); 4] {
        let [x, y] = self.axes;
        [
            (x, self.center.dot(x) + self.half_extents.x),
            (x * -1.0, -self.center.dot(x) + self.half_extents.x),
            (y, self.center.dot(y) + self.half_extents.y),
            (y * -1.0, -self.center.dot(y) + self.half_extents.y),
        ]
    }

    pub(crate) fn blocks_segment(self, from: Vec2, to: Vec2) -> bool {
        self.sweep(from, to).is_some()
    }

    fn sweep(self, from: Vec2, to: Vec2) -> Option<Contact> {
        let motion = to - from;
        let mut entry = 0.0_f64;
        let mut exit = 1.0_f64;
        let mut normal = Vec2::ZERO;
        let mut nearest = (f32::NEG_INFINITY, Vec2::ZERO);
        for (n, offset) in self.planes() {
            let distance = from.dot(n) - offset;
            if distance > nearest.0 {
                nearest = (distance, n);
            }
            let speed = motion.dot(n);
            if speed.abs() < 1e-12 {
                if distance >= 0.0 {
                    return None;
                }
                continue;
            }
            let t = -f64::from(distance) / f64::from(speed);
            if speed < 0.0 {
                if t >= entry {
                    entry = t;
                    normal = n;
                }
            } else {
                exit = exit.min(t);
            }
        }
        if nearest.0 < 0.0 {
            // A moving kinematic panel may start a tick over existing water.
            // Push to the nearest face without deleting or inventing volume.
            return Some(Contact {
                time: 0.0,
                position: from + nearest.1 * (CONTACT_SKIN - nearest.0),
                normal: nearest.1,
            });
        }
        if entry > exit || normal == Vec2::ZERO || !(0.0..=1.0).contains(&entry) {
            return None;
        }
        Some(Contact {
            time: entry,
            position: from + motion * entry as f32 + normal * CONTACT_SKIN,
            normal,
        })
    }
}

struct Contact {
    time: f64,
    position: Vec2,
    normal: Vec2,
}

pub(crate) struct Motion {
    pub position: Vec2,
    pub velocity: Vec2,
    pub caught: Option<(usize, usize)>,
    pub impact: Option<crate::splash::Impact>,
}

impl WaterWorld {
    /// Replace the small set of explicit scenery boxes, keeping storage bounded.
    /// Update these with the same pose used for the visible/rigid-body panels.
    pub fn set_solid_boxes(&mut self, boxes: &[SolidBox]) -> Result<(), WaterError> {
        if boxes.len() > MAX_SOLID_BOXES {
            return Err(WaterError::Capacity);
        }
        self.solids.clear();
        self.solids.extend_from_slice(boxes);
        Ok(())
    }

    pub fn solid_boxes(&self) -> &[SolidBox] {
        &self.solids
    }

    pub(crate) fn collide(
        &self,
        mut from: Vec2,
        mut to: Vec2,
        mut velocity: Vec2,
        leaving_pool: Option<usize>,
        collect: bool,
    ) -> Motion {
        let mut impact: Option<crate::splash::Impact> = None;
        for _ in 0..MAX_CONTACTS {
            let contact = self
                .solids
                .iter()
                .filter_map(|b| b.sweep(from, to))
                .min_by(|a, b| a.time.total_cmp(&b.time));
            // A pool bed and the top of its solid panel describe the same
            // boundary. Test collection through the true contact, not the
            // skin offset that leaves a sliding parcel safely outside. Allow
            // the two f32 representations a small spatial tolerance.
            let end = contact.as_ref().map_or(to, |c| {
                let motion = to - from;
                let t = (c.time as f32 + 2.0 * CONTACT_SKIN / motion.length().max(1e-6)).min(1.0);
                from + motion * t
            });
            if collect && let Some(caught) = self.catch(from, end, leaving_pool) {
                return Motion {
                    position: end,
                    velocity,
                    caught: Some(caught),
                    impact,
                };
            }
            let Some(contact) = contact else {
                return Motion {
                    position: to,
                    velocity,
                    caught: None,
                    impact,
                };
            };
            if self.config.splash.is_some() {
                let speed = -f64::from(velocity.dot(contact.normal));
                if speed > impact.map_or(0.0, |i| i.speed) {
                    impact = Some(crate::splash::Impact {
                        position: contact.position,
                        normal: contact.normal,
                        speed,
                    });
                }
            }
            // Inelastic wall contact: retain tangent motion and all water.
            // Sweep the remaining displacement too, so crossing a thin wall
            // or contacting two faces in one tick cannot tunnel through it.
            let mut remaining = (to - from) * (1.0 - contact.time) as f32;
            remaining -= contact.normal * remaining.dot(contact.normal).min(0.0);
            velocity -= contact.normal * velocity.dot(contact.normal).min(0.0);
            from = contact.position;
            to = from + remaining;
        }
        // A corner can consume the contact budget. Stay at the last contact,
        // never finish an unchecked displacement through solid scenery.
        Motion {
            position: from,
            velocity,
            caught: None,
            impact,
        }
    }

    pub(crate) fn advance_section(&self, section: &mut Section, dt: f64, bounds: Option<[f64; 2]>) {
        let from = section.position;
        section.advance(dt, self.config, bounds);
        let result = self.collide(from, section.position, section.velocity, None, false);
        section.position = result.position;
        section.velocity = result.velocity;
    }
}
