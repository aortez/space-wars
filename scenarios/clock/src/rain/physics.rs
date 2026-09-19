use engine_core::Vec2;
use engine_rapier::{
    buoyancy::{BuoyancyConfig, BuoyancyReport, BuoyantBody},
    world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
        PhysicsWorld, PhysicsWorldConfig,
    },
};
use engine_water::{WaterWorld, immersion::HullShape};

use crate::{floor::DrainGeometry, layout::Layout};

pub(super) const DT: f64 = 1.0 / 60.0;
pub(super) const DENSITY: f32 = 0.45;

pub(super) fn half_extents(layout: Layout) -> Vec2 {
    Vec2::new(layout.pitch * 0.45, layout.pitch * 0.18)
}

/// A passive dynamic hull. No path following, surface snapping or drain attraction.
/// Future control forces can be added between clear_forces and step.
pub(super) struct FloatWorld {
    pub world: PhysicsWorld,
    pub duck: Option<BuoyantBody>,
    pub report: BuoyancyReport,
    pub half_extents: Vec2,
}

impl FloatWorld {
    pub fn new(drain: DrainGeometry) -> Self {
        let layout = drain.layout();
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -400.0),
            length_unit: layout.pitch,
            max_ccd_substeps: 2,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        world.reserve(2, 5, 0);
        let bottom = layout.bounds_min.y;
        let half = layout.bounds_max.x;
        // Exact visible floor halves; in particular no collider bridges the drain.
        let supports = drain.slabs().chain([
            (
                Vec2::new(-half - 4.0, bottom),
                Vec2::new(-half, layout.bounds_max.y),
            ),
            (
                Vec2::new(half, bottom),
                Vec2::new(half + 4.0, layout.bounds_max.y),
            ),
        ]);
        let id = PhysicsId::new(1000);
        let colliders: Vec<_> = supports
            .enumerate()
            .map(|(part, (min, max))| {
                let mut collider = ColliderSpec::cuboid(
                    ColliderId::new(id, ColliderRole::PRIMARY, part as u16),
                    (max.x - min.x) * 0.5,
                    (max.y - min.y) * 0.5,
                );
                collider.local_position = (min + max) * 0.5;
                collider
            })
            .collect();
        assert!(world.insert_body(
            BodyId::new(id, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                ..BodySpec::default()
            },
            &colliders
        ));
        Self {
            world,
            duck: None,
            report: BuoyancyReport::default(),
            half_extents: half_extents(layout),
        }
    }

    pub fn spawn(&mut self, position: Vec2, angle: f32) {
        assert!(self.duck.is_none());
        self.duck = Some(
            BuoyantBody::insert(
                &mut self.world,
                PhysicsId::new(1),
                BodySpec {
                    position,
                    angle,
                    ccd_enabled: true,
                    can_sleep: false,
                    ..BodySpec::default()
                },
                HullShape::Box {
                    half_width: self.half_extents.x,
                    half_height: self.half_extents.y,
                },
                DENSITY,
            )
            .expect("one bounded floating duck"),
        );
    }

    pub fn step(&mut self, water: &WaterWorld, drag: f32) {
        self.world.clear_forces();
        if let Some(duck) = &self.duck {
            self.report = duck
                .apply_forces(
                    &mut self.world,
                    water,
                    BuoyancyConfig {
                        drag,
                        ..BuoyancyConfig::default()
                    },
                    DT,
                )
                .expect("live duck hull");
        }
        self.world.step(DT as f32);
    }

    pub fn position(&self) -> Option<Vec2> {
        self.duck
            .as_ref()
            .and_then(|duck| self.world.motion(duck.body()))
            .map(|m| m.position)
    }

    pub fn remove(&mut self) {
        if let Some(duck) = self.duck.take() {
            assert!(self.world.remove_entity(duck.body().entity));
        }
        self.report = BuoyancyReport::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::floor::test_drain;
    use engine_water::{Boundary, PoolSpec, WaterConfig};
    const COLUMNS: usize = 128;
    const PARCELS: usize = 128;

    fn fill(water: &mut WaterWorld, depth: f64) {
        for pool in 0..water.pools().len() {
            let spec = water.pools()[pool].spec().clone();
            for i in 0..spec.bed.len() {
                water
                    .add_to_pool(
                        pool,
                        spec.left + (i as f64 + 0.5) * spec.column_width,
                        depth * spec.column_width,
                    )
                    .unwrap();
            }
        }
    }

    #[test]
    fn passive_hull_floats_without_inventing_horizontal_motion() {
        let layout = Layout::new(1024.0 / 768.0);
        let mut water = WaterWorld::new(
            WaterConfig::default(),
            vec![PoolSpec {
                left: -320.0,
                column_width: 5.0,
                bed: vec![f64::from(layout.floor_y); 128],
                boundaries: [Boundary::Closed; 2],
            }],
        )
        .unwrap();
        fill(&mut water, 30.0);
        let mut physics = FloatWorld::new(test_drain(layout));
        physics.spawn(Vec2::new(-100.0, layout.floor_y + 40.0), 0.12);
        for _ in 0..1200 {
            water.step(DT).unwrap();
            physics.step(&water, 5.0);
        }
        let motion = physics
            .world
            .motion(physics.duck.as_ref().unwrap().body())
            .unwrap();
        assert!(
            (physics.report.submerged_fraction - f64::from(DENSITY)).abs() < 0.03,
            "{motion:?}"
        );
        assert!((motion.position.x + 100.0).abs() < 0.5, "{motion:?}");
        assert!(
            motion.linear_velocity.length() < 0.2 && motion.angle.abs() < 0.03,
            "{motion:?}"
        );
    }

    #[test]
    fn passive_current_carries_hull_through_either_drain_lip() {
        for aspect in [1024.0 / 768.0, 800.0 / 480.0, 480.0 / 800.0] {
            for side in [-1.0, 1.0] {
                for drag in [0.0, 5.0] {
                    let layout = Layout::new(aspect);
                    let mut water = test_drain(layout).water_world(COLUMNS, PARCELS);
                    fill(&mut water, f64::from(layout.pitch));
                    let mut physics = FloatWorld::new(test_drain(layout));
                    let start = side * layout.bounds_max.x * 0.80;
                    physics.spawn(Vec2::new(start, layout.floor_y + layout.pitch * 1.4), 0.0);
                    let mut exit_tick = None;
                    for tick in 0..2400 {
                        fill(&mut water, f64::from(layout.pitch) * 0.16 * DT);
                        water.step(DT).unwrap();
                        physics.step(&water, drag);
                        let p = physics.position().unwrap();
                        assert!(p.x.is_finite() && p.y.is_finite());
                        if p.y + physics.half_extents.length() < layout.bounds_min.y {
                            exit_tick = Some(tick);
                            break;
                        }
                    }
                    eprintln!(
                        "aspect={aspect} side={side} drag={drag} exit={exit_tick:?} p={:?}",
                        physics.position()
                    );
                    if drag == 0.0 {
                        assert!(exit_tick.is_none());
                        // Undamped bobbing/tilting can make small floor-contact
                        // translations; it must not reproduce the draining trip.
                        assert!(side * physics.position().unwrap().x > layout.bounds_max.x * 0.5);
                    } else {
                        assert!(
                            exit_tick.is_some(),
                            "current must carry the passive duck into the drain"
                        );
                    }
                    physics.remove();
                    assert_eq!(
                        (physics.world.body_count(), physics.world.collider_count()),
                        (1, 4)
                    );
                }
            }
        }
    }
}
