use rand::{SeedableRng, rngs::StdRng};

use super::{EventContext, EventPhase};
use crate::{SegmentRepresentation, digits, physics::FallingWorld};

pub const FALLING_TICKS: u64 = 210;
pub const REFORMING_TICKS: u64 = 90;

pub(crate) enum FallingEvent {
    Falling { tick: u64, world: Box<FallingWorld> },
    Reforming { tick: u64 },
}

impl FallingEvent {
    pub fn new(context: EventContext<'_>, seed: u64) -> Self {
        Self::Falling {
            tick: 0,
            world: Box::new(FallingWorld::new(
                context.layout,
                context.segments,
                &mut StdRng::seed_from_u64(seed),
            )),
        }
    }

    pub fn step(&mut self, context: EventContext<'_>) -> bool {
        match self {
            Self::Falling { tick, world } => {
                world.step(context.segments);
                *tick += 1;
                if *tick >= FALLING_TICKS {
                    for segment in context.segments.iter_mut() {
                        let (position, angle) = match segment.representation {
                            SegmentRepresentation::Rigid { position, angle } => (position, angle),
                            _ => (context.layout.segment_center(segment.id), 0.0),
                        };
                        segment.representation = SegmentRepresentation::Reforming {
                            position,
                            angle,
                            was_lit: segment.lit,
                        };
                    }
                    // Replacing the variant drops all rigid bodies before reformation.
                    *self = Self::Reforming { tick: 0 };
                    digits::apply_snapshot(context.segments, context.display);
                }
                false
            }
            Self::Reforming { tick } => {
                *tick += 1;
                *tick >= REFORMING_TICKS
            }
        }
    }

    pub fn phase(&self) -> EventPhase {
        match self {
            Self::Falling { .. } => EventPhase::Falling,
            Self::Reforming { .. } => EventPhase::Reforming,
        }
    }

    pub fn phase_tick(&self) -> u64 {
        match self {
            Self::Falling { tick, .. } | Self::Reforming { tick } => *tick,
        }
    }

    pub fn physics_counts(&self) -> (usize, usize) {
        match self {
            Self::Falling { world, .. } => (world.body_count(), world.collider_count()),
            Self::Reforming { .. } => (0, 0),
        }
    }
}
