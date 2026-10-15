//! Retained tanks unlock after their residents leave, then follow water load.
use super::*;

const FULL_ANGLE: f64 = 15.0;
const FILM_ANGLE: f64 = 1.0;

impl FloorShape {
    pub fn aquarium(layout: Layout) -> Self {
        let mut shape = Self::clock(layout);
        shape.max_gap = shape.half_width * 0.18;
        shape.max_drop = (shape.half_width - shape.max_gap) * FULL_ANGLE.to_radians().tan();
        shape
    }

    pub fn angle_degrees(self, opening: f64) -> f64 {
        (opening * self.max_drop / (self.half_width - opening * self.max_gap))
            .atan()
            .to_degrees()
    }

    fn opening_for_angle(self, degrees: f64) -> f64 {
        let slope = degrees.to_radians().tan();
        (self.half_width * slope / (self.max_drop + self.max_gap * slope)).clamp(0.0, 1.0)
    }
}

impl ResponsiveFloor {
    pub fn step_retained(
        &mut self,
        water: &mut WaterWorld,
        dt: f64,
        full_depth: f64,
        film_depth: f64,
        hull: Option<(Vec2, f64)>,
    ) {
        let s = self.shape;
        // Departing airborne water no longer loads the floor. Measure volume
        // over the original width so tilting/retracting cannot increase load.
        self.load = water.pools()[..2]
            .iter()
            .flat_map(|p| p.columns())
            .map(|c| c.volume)
            .sum::<f64>()
            / (s.half_width * 2.0);
        let film = (self.load / film_depth).clamp(0.0, 1.0);
        let bulk = ((self.load - film_depth) / (full_depth - film_depth)).clamp(0.0, 1.0);
        let target = s.opening_for_angle(FILM_ANGLE * film + (FULL_ANGLE - FILM_ANGLE) * bulk);
        let difference = target - self.opening;
        let desired_speed = (difference / dt).clamp(-1.4, 1.4);
        // Bound acceleration of the moving lip, including the slope change
        // from horizontal retraction. A steady speed need not restart from
        // rest every frame, as the old quasi-static movement bound required.
        let acceleration = 400.0 * 0.35 * (1.0 - s.max_gap / s.half_width).powi(2) / s.max_drop;
        let speed = self.opening_speed
            + (desired_speed - self.opening_speed).clamp(-acceleration * dt, acceleration * dt);
        let next = self.opening + (speed * dt).clamp(difference.min(0.0), difference.max(0.0));
        if next < self.opening
            && hull.is_some_and(|(p, r)| {
                f64::from(p.x.abs()) < self.opening * s.max_gap + r + 2.0
                    && f64::from(p.y) - r < s.floor_y + 2.0
                    && f64::from(p.y) + r
                        > s.floor_y - self.opening * s.max_drop - s.thickness - 2.0
            })
        {
            self.clearance_holds += 1;
            self.opening_speed = 0.0;
            return;
        }
        match s.move_water(water, next, dt, true) {
            Ok(()) => {
                self.opening_speed = (next - self.opening) / dt;
                self.opening = next;
            }
            Err(WaterError::Capacity) => {
                self.deferrals += 1;
                self.opening_speed = 0.0;
            }
            Err(e) => panic!("bounded retained floor: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_water::WaterConfig;

    #[test]
    fn a_dry_retained_floor_waits_for_the_duck_then_closes_completely() {
        let shape = FloorShape::aquarium(Layout::new(800.0 / 480.0));
        let mut water = WaterWorld::new(
            WaterConfig {
                exit_y: -500.0,
                ..WaterConfig::default()
            },
            shape.pools_at(0.6).to_vec(),
        )
        .unwrap();
        shape.configure_at(&mut water, 0.6);
        let mut floor = ResponsiveFloor::new(shape, 0.0);
        floor.opening = 0.6;
        let dt = 1.0 / 60.0;
        let hull = Some((
            Vec2::new(0.0, (shape.floor_y - 0.6 * shape.max_drop) as f32),
            12.0,
        ));
        for _ in 0..30 {
            floor.step_retained(&mut water, dt, 350.0, 1.0, hull);
            water.step(dt).unwrap();
            assert_eq!(floor.opening, 0.6);
        }
        assert_eq!(floor.clearance_holds, 30);
        for _ in 0..120 {
            floor.step_retained(&mut water, dt, 350.0, 1.0, None);
            water.step(dt).unwrap();
        }
        assert_eq!(floor.opening, 0.0);
        assert_eq!(water.stats().injected, 0.0);
        assert_eq!(water.stats().reclaimed, 0.0);
    }
}
