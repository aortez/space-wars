//! A fixed, axis-aligned frame inferred from the persisted base endpoint.
use super::*;

// Bounds the translated geometry we qualify. Numerical admission remains
// separate: even an in-range scene can have unrepresentable motion or work.
const MAX_OFFSET: f64 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Frame {
    origin: [f64; 2],
}

impl Frame {
    pub(super) fn from_base(position: [f32; 2], role: usize) -> Option<Self> {
        let origin = [f64::from(position[0]) - role as f64, f64::from(position[1])];
        origin
            .iter()
            .all(|x| x.abs() <= MAX_OFFSET && f64::from(*x as f32) == *x)
            .then_some(Self { origin })
    }

    pub(super) fn x(self, world_x: f32) -> f64 {
        f64::from(world_x) - self.origin[0]
    }

    pub(super) fn valid_position(self, position: [f32; 2], role: usize) -> bool {
        position.iter().all(|x| x.is_finite())
            && f64::from(position[1]) == self.origin[1]
            && if role == 0 {
                self.x(position[0]) <= 1.0e-6
            } else {
                self.x(position[0]) == role as f64
            }
    }

    pub(super) fn revision(self, masses: &[f32]) -> u32 {
        if self.origin == [0.0; 2] {
            mass_revision(masses)
        } else {
            3
        }
    }

    pub(super) fn valid_history(self, history: &History) -> bool {
        if history.revision == 3 {
            // The initial relative coordinate is a subtraction of two f32
            // world coordinates in f64, not necessarily representable in f32.
            let initial = (history.reference[0] + self.origin[0]) as f32;
            self.origin != [0.0; 2]
                && initial.is_finite()
                && self.x(initial) == history.reference[0]
        } else {
            self.origin == [0.0; 2]
        }
    }
}
