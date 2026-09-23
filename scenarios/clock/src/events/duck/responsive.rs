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
        motion: Option<(Vec2, Vec2)>,
    ) -> Self {
        // No obstacle generation/planning is needed to join existing panels.
        let mut duck = Self::with_player(Self::new(layout, seed), session_id, seat);
        duck.fit_character();
        duck.direction = facing;
        duck.responsive_floor = Some(floor);
        if let Some((mut position, velocity)) = motion {
            // Preserve an existing floating actor in place. When it is already
            // resting on a panel, give the round player hull minimal clearance
            // above that face rather than introducing a solver penetration.
            let floor = duck.responsive_floor.as_ref().unwrap();
            if f64::from(position.x.abs())
                > floor.opening * floor.shape.max_gap + f64::from(duck.radius)
                && position.y > layout.floor_y - floor.shape.max_drop as f32 - duck.radius
            {
                position.y = position.y.max(duck.panel_spawn_y(position.x));
            }
            duck.spawn_motion = Some((position, velocity));
            duck.spawn();
            duck.tick = OPENING_TICKS + 44;
            duck.enter(EventPhase::Exiting);
        }
        duck
    }

    pub fn responsive_floor(&self) -> Option<&ResponsiveFloor> {
        self.responsive_floor.as_ref()
    }

    pub fn sync_responsive_floor(&mut self, floor: &ResponsiveFloor) {
        assert!(self.responsive_floor.is_some());
        self.responsive_floor = Some(floor.clone());
    }

    /// Only one character can own this arena, so clearance is one bounded hull.
    pub fn clearance_hull(&self) -> Option<(Vec2, f64)> {
        self.position()
            .map(|p| (self.render_position(p), f64::from(self.radius)))
    }

    fn panel_spawn_y(&self, x: f32) -> f32 {
        let floor = self.responsive_floor.as_ref().unwrap();
        let slope = (floor.opening * floor.shape.max_drop)
            / (floor.shape.half_width - floor.opening * floor.shape.max_gap);
        floor.shape.surface_y(f64::from(x), floor.opening) as f32
            + self.radius * (1.0 + slope * slope).sqrt() as f32
            + self.radius * 0.05
    }

    pub(super) fn prepare_responsive_spawn(&mut self, water: Option<&WaterWorld>) {
        let x = self.render_position(Vec2::new(self.radius * 6.0, 0.0)).x;
        let mut y = self.panel_spawn_y(x);
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

    pub fn door_floor(&self, local_x: f32) -> f32 {
        self.responsive_floor
            .as_ref()
            .map_or(self.layout.floor_y, |floor| {
                floor.shape.surface_y(
                    f64::from(self.render_position(Vec2::new(local_x, 0.0)).x),
                    floor.opening,
                ) as f32
            })
    }
}
