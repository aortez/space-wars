//! A fixed cardinal frame inferred from the persisted base endpoint.
use super::*;
use crate::world::QuarterTurn;

const MAX_OFFSET: f64 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Frame {
    origin: [f64; 2],
    turn: QuarterTurn,
    axis: usize,
    sign: f32,
}

impl Frame {
    #[cfg(test)]
    pub(super) fn from_base(position: [f32; 2], role: usize) -> Option<Self> {
        Self::at_base(position, 0.0, role)
    }

    pub(super) fn at_base(position: [f32; 2], angle: f32, role: usize) -> Option<Self> {
        let turn = QuarterTurn::from_angle(angle)?;
        let n = turn.rotate(Vec2::X);
        let axis = usize::from(n.y != 0.0);
        let sign = [n.x, n.y][axis];
        let mut origin = position.map(f64::from);
        origin[axis] -= f64::from(sign) * role as f64;
        origin
            .iter()
            .all(|x| x.abs() <= MAX_OFFSET && f64::from(*x as f32) == *x)
            .then_some(Self {
                origin,
                turn,
                axis,
                sign,
            })
    }

    pub(super) fn x(self, position: [f32; 2]) -> f64 {
        f64::from(self.sign) * (f64::from(position[self.axis]) - self.origin[self.axis])
    }

    pub(super) fn speed(self, velocity: [f32; 2]) -> f32 {
        self.sign * velocity[self.axis]
    }

    pub(super) fn vector(self, speed: f32) -> Vector {
        let mut v = [0.0; 2];
        v[self.axis] = self.sign * speed;
        Vector::new(v[0], v[1])
    }

    pub(super) fn world_axis(self, position: [f32; 2]) -> f64 {
        f64::from(position[self.axis])
    }

    pub(super) fn valid_endpoint(self, endpoint: &Endpoint) -> bool {
        endpoint.valid()
            && QuarterTurn::from_angle(endpoint.angle) == Some(self.turn)
            && endpoint.velocity[1 - self.axis] == 0.0
    }

    pub(super) fn valid_position(self, position: [f32; 2], role: usize) -> bool {
        position.iter().all(|x| x.is_finite())
            && f64::from(position[1 - self.axis]) == self.origin[1 - self.axis]
            && if role == 0 {
                self.x(position) <= 1.0e-6
            } else {
                self.x(position) == role as f64
            }
    }

    pub(super) fn revision(self, masses: &[f32]) -> u32 {
        if self.turn != QuarterTurn::Deg0 {
            4
        } else if self.origin == [0.0; 2] {
            mass_revision(masses)
        } else {
            3
        }
    }

    pub(super) fn valid_history(self, history: &History) -> bool {
        let matching_frame = match history.revision {
            1 | 2 => self.turn == QuarterTurn::Deg0 && self.origin == [0.0; 2],
            3 => self.turn == QuarterTurn::Deg0 && self.origin != [0.0; 2],
            4 => self.turn != QuarterTurn::Deg0,
            _ => false,
        };
        // Relative coordinates are exact differences of world f32 endpoints,
        // which need not be representable in f32 themselves.
        let initial = (f64::from(self.sign) * history.reference[0] + self.origin[self.axis]) as f32;
        let mut position = self.origin.map(|x| x as f32);
        position[self.axis] = initial;
        matching_frame && initial.is_finite() && self.x(position) == history.reference[0]
    }
}
