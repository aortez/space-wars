//! Enter Falling's existing arena without replacing its floor or Rapier world.
use super::*;
use crate::floor::DrainGeometry;

impl DuckEvent {
    pub fn new_drain_player(
        drain: DrainGeometry,
        seed: u64,
        session: u64,
        seat: u8,
        mut world: PhysicsWorld,
    ) -> Self {
        let layout = drain.layout();
        let mut duck = Self::with_player(Self::new(layout, seed), session, seat);
        duck.fit_character();
        let lip = layout.drain_half_width();
        duck.course = Some(planner::Course {
            surfaces: vec![
                planner::Surface {
                    start: 0.0,
                    end: duck.width * 0.5 - lip,
                    height: 0.0,
                },
                planner::Surface {
                    start: duck.width * 0.5 + lip,
                    end: duck.width,
                    height: 0.0,
                },
            ],
            pattern: engine_common::ClockDuckCoursePattern::Platforms,
            attempts: 0,
            fallback: false,
        });
        // Keep all bodies, contacts, and solver settings. Falling compensates
        // its body's gravity scales; the duck keeps its calibrated acceleration
        // and density-based buoyancy if a later event brings water here.
        assert!(world.set_gravity(Vec2::new(0.0, -duck.movement.gravity)));
        duck.world = Some(world);
        duck.drain_floor = Some(drain);
        duck.preserve_arena_opacity(1.0);
        // Both old walls stay physical for the bars. The far backstage door
        // ends the visit at its visible threshold, before the wall behind it.
        duck
    }
}
