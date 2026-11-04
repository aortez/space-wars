use super::*;
use engine_terrain::{Brush, EditMode};

pub(super) struct Rng(pub(super) u64);
impl Rng {
    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    pub(super) fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * ((self.next() >> 40) as f32 / 16_777_216.0)
    }
}

impl FlightCandidate {
    /// Every index produces exactly one candidate. Rejections remain observable.
    pub fn generate(seed: u64, index: u64) -> Result<Self, &'static str> {
        let mut rng = Rng(seed ^ index.wrapping_mul(0xd1b54a32d192ed03));
        let family = [
            FlightFamily::Clear,
            FlightFamily::Moving,
            FlightFamily::Obstructed,
            FlightFamily::Altered,
        ][(index % 4) as usize];
        let max_speed = match family {
            FlightFamily::Clear | FlightFamily::Moving => 70.0,
            FlightFamily::Obstructed => 15.0,
            FlightFamily::Altered => 12.0,
        };
        let mut task = FlightTask::default();
        task.start.seed = rng.next();
        task.start.bearing = rng.range(-PI, PI);
        task.start.heading_offset = rng.range(-PI, PI);
        task.start.altitude = rng.range(100.0, 150.0);
        task.start.relative_velocity =
            Vec2::from_radians(rng.range(-PI, PI)) * rng.range(0.0, max_speed);
        task.start.spin = rng.range(-1.8, 1.8);
        task.start.gravity = rng.next().is_multiple_of(2);
        task.start.planet_spin = [-0.015, 0.0, 0.015][(rng.next() % 3) as usize];
        let angle = task.start.bearing;
        let rotate = |v: Vec2| v.rotate_radians(angle);
        let mut recipe = "clear";
        match family {
            FlightFamily::Clear | FlightFamily::Moving => {
                task.goal_offset = Vec2::from_radians(rng.range(-PI, PI)) * rng.range(30.0, 200.0);
                if family == FlightFamily::Moving {
                    recipe = "constant_velocity";
                    task.goal_velocity =
                        Vec2::from_radians(rng.range(-PI, PI)) * rng.range(5.0, 20.0);
                }
            }
            FlightFamily::Obstructed => match (index / 4) % 3 {
                0 => {
                    recipe = "wall";
                    let distance = rng.range(70.0, 140.0);
                    task.goal_offset = rotate(Vec2::new(distance, 0.0));
                    add_box(
                        &mut task,
                        rotate(Vec2::new(distance * 0.5, 0.0)),
                        Vec2::new(3.0, rng.range(25.0, 50.0)),
                        angle,
                    );
                }
                1 => {
                    recipe = "open_u";
                    let length = rng.range(32.0, 44.0);
                    let width = rng.range(19.0, 25.0);
                    task.goal_offset = rotate(Vec2::new(length + 35.0, 0.0));
                    for sign in [-1.0, 1.0] {
                        add_box(
                            &mut task,
                            rotate(Vec2::new(0.0, sign * width)),
                            Vec2::new(length, 2.0),
                            angle,
                        );
                    }
                    add_box(
                        &mut task,
                        rotate(Vec2::new(length, 0.0)),
                        Vec2::new(2.0, width + 2.0),
                        angle,
                    );
                }
                _ => {
                    recipe = "curved_corridor";
                    let radius = rng.range(45.0, 60.0);
                    let half_width = rng.range(16.0, 20.0);
                    task.goal_offset = rotate(Vec2::new(radius, radius));
                    for side in [-1.0, 1.0] {
                        let r = radius + side * half_width;
                        for i in 0..6 {
                            let a = -FRAC_PI_2 - 0.12 + (FRAC_PI_2 + 0.24) * i as f32 / 6.0;
                            let b = -FRAC_PI_2 - 0.12 + (FRAC_PI_2 + 0.24) * (i + 1) as f32 / 6.0;
                            let p = Vec2::new(0.0, radius) + Vec2::from_radians(a) * r;
                            let q = Vec2::new(0.0, radius) + Vec2::from_radians(b) * r;
                            add_box(
                                &mut task,
                                rotate((p + q) * 0.5),
                                Vec2::new(p.distance_to(q) * 0.5 + 0.5, 1.5),
                                angle + (q - p).y.atan2((q - p).x),
                            );
                        }
                    }
                }
            },
            FlightFamily::Altered if (index / 4).is_multiple_of(2) => {
                recipe = "carved_passage";
                task.start.altitude = rng.range(15.0, 25.0);
                task.start.planet_spin = 0.0;
                let base = FlightEnvironment::new(task.clone())?;
                let center = base.state.world.planets[0].position;
                // Carve a bay through the chord between two exterior points.
                // A diameter cut would detach half the planet into moving debris,
                // which is explicitly outside this first generator's scope.
                let goal = center + (base.frame.center_of_mass - center).rotate_radians(1.6);
                task.goal_offset = goal - base.frame.center_of_mass;
                let field = &base.state.world.terrain.planets[&0].field;
                let planet_angle = base
                    .state
                    .world
                    .physics
                    .world
                    .motion(base.state.world.physics.planet_body(0))
                    .unwrap()
                    .angle;
                let cut_position = Vec2::Y.rotate_radians(task.start.bearing + 0.8 - planet_angle)
                    * base.state.world.planets[0].radius;
                let cut_center = field
                    .local_to_cell(cut_position)
                    .ok_or("missing cut center")?;
                task.terrain_edits.push(TerrainEdit {
                    brush: Brush::Circle {
                        center: cut_center,
                        radius: (rng.range(36.0, 42.0) / field.cell_size()).ceil() as u32,
                    },
                    mode: EditMode::Remove,
                });
            }
            FlightFamily::Altered => {
                recipe = "built_protrusion";
                let distance = rng.range(70.0, 110.0);
                task.goal_offset = rotate(Vec2::new(distance, 0.0));
                add_box(
                    &mut task,
                    rotate(Vec2::new(distance * 0.5, 0.0)),
                    Vec2::new(rng.range(3.0, 7.0), rng.range(20.0, 35.0)),
                    angle,
                );
            }
        }
        Ok(Self {
            generator_version: GENERATOR_VERSION,
            seed,
            index,
            family,
            recipe: recipe.into(),
            task,
        })
    }
}

fn add_box(task: &mut FlightTask, offset: Vec2, half_extents: Vec2, angle: f32) {
    task.obstacles.push(FlightObstacle {
        offset,
        half_extents,
        angle,
    });
}
