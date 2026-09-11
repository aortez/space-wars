//! Clock-local presentation, independent of scheduling, timekeeping and physics.
//! Content supplies immutable cells and pivots. A bounded recipe samples fresh
//! geometry/material from those cells; it never integrates or edits their poses.
pub(crate) mod content;
pub(crate) mod effects;
mod font;
#[cfg(test)]
mod tests;

use engine_core::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl Bounds {
    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }
    pub fn size(self) -> Vec2 {
        self.max - self.min
    }
}

/// A convex quadrilateral clipped to a rectangular presentation viewport has
/// at most eight vertices. Scratch storage is fixed, even for offscreen text.
pub(crate) fn clip_quad(points: [Vec2; 4], bounds: Bounds) -> ([Vec2; 8], usize) {
    let mut input = [Vec2::ZERO; 8];
    input[..4].copy_from_slice(&points);
    let mut count = 4;
    for (axis, edge, keep_greater) in [
        (0, bounds.min.x, true),
        (0, bounds.max.x, false),
        (1, bounds.min.y, true),
        (1, bounds.max.y, false),
    ] {
        if count == 0 {
            break;
        }
        let coord = |p: Vec2| if axis == 0 { p.x } else { p.y };
        let inside = |p: Vec2| {
            if keep_greater {
                coord(p) >= edge
            } else {
                coord(p) <= edge
            }
        };
        let mut output = [Vec2::ZERO; 8];
        let mut written = 0;
        let mut previous = input[count - 1];
        for current in input[..count].iter().copied() {
            if inside(previous) != inside(current) {
                let t = (edge - coord(previous)) / (coord(current) - coord(previous));
                let mut intersection = previous + (current - previous) * t;
                if axis == 0 {
                    intersection.x = edge;
                } else {
                    intersection.y = edge;
                }
                output[written] = intersection;
                written += 1;
            }
            if inside(current) {
                output[written] = current;
                written += 1;
            }
            previous = current;
        }
        input = output;
        count = written;
    }
    (input, count)
}
