//! A visit read together with the arena it moves in. The world belongs to the
//! run rather than the visit, so anything that reports where the character is
//! has to be handed both.
use std::ops::Deref;

use super::*;

#[derive(Clone, Copy)]
pub(crate) struct DuckView<'a> {
    duck: &'a DuckEvent,
    world: Option<&'a PhysicsWorld>,
}

impl<'a> DuckView<'a> {
    /// `world` is `None` before the visit's first step and after a finished
    /// visit has dropped its arena.
    pub fn new(duck: &'a DuckEvent, world: Option<&'a PhysicsWorld>) -> Self {
        Self { duck, world }
    }

    pub fn position(&self) -> Option<Vec2> {
        self.world.and_then(|world| self.duck.position(world))
    }

    #[cfg(test)]
    pub fn grounded(&self) -> bool {
        self.world.is_some_and(|world| self.duck.grounded(world))
    }

    pub fn feet_moving(&self) -> bool {
        self.world.is_some_and(|world| self.duck.feet_moving(world))
    }

    pub fn ledge_anchor(&self) -> Option<Vec2> {
        self.world.and_then(|world| self.duck.ledge_anchor(world))
    }

    pub fn clearance_hull(&self) -> Option<(Vec2, f64)> {
        self.world.and_then(|world| self.duck.clearance_hull(world))
    }

    pub fn diagnostics(&self) -> ClockDuckState {
        self.duck.diagnostics(self.world)
    }

    pub fn player_diagnostics(&self) -> Option<engine_common::ClockPlayerDuckState> {
        self.duck.player_diagnostics(self.world)
    }
}

impl Deref for DuckView<'_> {
    type Target = DuckEvent;

    fn deref(&self) -> &DuckEvent {
        self.duck
    }
}
