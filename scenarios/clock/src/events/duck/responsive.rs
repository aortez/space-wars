//! Player visits on Rain's existing moving panels, without a second world.
use super::*;

impl DuckEvent {
    pub fn new_responsive_player(
        layout: Layout,
        seed: u64,
        session_id: u64,
        seat: u8,
        facing: f32,
        floor: ResponsiveFloor,
    ) -> Self {
        // No obstacle generation/planning is needed to join existing panels.
        let mut duck = Self::with_player(Self::new(layout, seed), session_id, seat);
        duck.fit_character();
        duck.terrain.direction = facing;
        duck.terrain.responsive_floor = Some(floor);
        duck.adopt_frame();
        duck.preserve_arena_opacity(1.0);
        duck
    }

    /// Continue Rain's passive actor from its exact translation, already past
    /// the entrance, instead of walking a fresh one in.
    pub fn adopt_passive_actor(
        &mut self,
        arena: &mut Option<PhysicsWorld>,
        (mut position, velocity): (Vec2, Vec2),
    ) {
        // Preserve an existing floating actor in place. When it is already
        // resting on a panel, give the round player hull minimal clearance
        // above that face rather than introducing a solver penetration.
        let floor = self.terrain.responsive_floor.as_ref().unwrap();
        if f64::from(position.x.abs())
            > floor.opening * floor.shape.max_gap + f64::from(self.radius)
            && position.y > self.layout.floor_y - floor.shape.max_drop as f32 - self.radius
        {
            position.y = position.y.max(self.terrain.panel_spawn_y(position.x));
        }
        self.spawn_motion = Some((position, velocity));
        self.spawn(arena);
        self.tick = OPENING_TICKS + PLAYER_EXIT_DELAY_TICKS;
        self.enter(EventPhase::Exiting);
    }

    /// Only one character can own this arena, so clearance is one bounded hull.
    pub fn clearance_hull(&self, world: &PhysicsWorld) -> Option<(Vec2, f64)> {
        self.position(world)
            .map(|p| (self.render_position(p), f64::from(self.radius)))
    }

    pub(super) fn prepare_responsive_spawn(&mut self, water: Option<&WaterWorld>) {
        let x = self.render_position(Vec2::new(self.radius * 6.0, 0.0)).x;
        let mut y = self.terrain.panel_spawn_y(x);
        if let Some(water) = water {
            // Only the two panel pools supply the entrance, not digit ledges.
            for pool in water.pools().iter().take(2) {
                for c in pool.columns_in_range(f64::from(x) - 1e-4, f64::from(x) + 1e-4) {
                    y = y.max(c.surface as f32 + self.radius * 1.05);
                }
            }
        }
        self.spawn_motion = Some((Vec2::new(x, y), Vec2::ZERO));
    }
}
