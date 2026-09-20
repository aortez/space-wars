//! Load-responsive floor proof, using the real water and rigid-body adapters.
//! Uses the same actuator as Rain; no extra launcher scenario or fake current.
use super::*;
use crate::floor::responsive::{FloorShape, ResponsiveFloor};
use engine_core::Vec2;
use engine_rapier::{
    buoyancy::{BuoyancyConfig, BuoyantBody},
    world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
        PhysicsWorld, PhysicsWorldConfig,
    },
};
use engine_water::immersion::HullShape;

const N: usize = 32;
const HALF: f64 = 80.0;
const GAP: f64 = 14.0;
const DROP: f64 = 12.0;
const DT: f64 = 1.0 / 60.0;
const DUCK_RADIUS: f64 = 3.24;

fn shape() -> FloorShape {
    FloorShape {
        half_width: HALF,
        floor_y: 0.0,
        max_gap: GAP,
        max_drop: DROP,
        thickness: 4.0,
        columns: N,
        load_depth: 8.0,
    }
}

pub struct ResponsiveFloorFixture {
    pub water: WaterWorld,
    pub opening: f64,
    pub load: f64,
    pub deferrals: u64,
    pub clearance_holds: u64,
    pub duck_exited: bool,
    world: PhysicsWorld,
    duck: Option<BuoyantBody>,
    floor: ResponsiveFloor,
}

impl ResponsiveFloorFixture {
    pub fn new(depth: f64, duck: bool) -> Self {
        assert!(depth.is_finite() && (0.0..=20.0).contains(&depth));
        let mut water = WaterWorld::new(
            WaterConfig {
                max_parcels: 256,
                exit_y: -80.0,
                ..WaterConfig::default()
            },
            shape().pools().into(),
        )
        .unwrap();
        shape().configure(&mut water);
        for pool in 0..2 {
            if depth > 0.0 {
                for i in 0..N {
                    water
                        .add_to_pool(
                            pool,
                            -HALF + pool as f64 * HALF + (i as f64 + 0.5) * HALF / N as f64,
                            depth * HALF / N as f64,
                        )
                        .unwrap();
                }
            }
        }
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -400.0),
            length_unit: 8.0,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        world.reserve(3, 3, 0);
        for side in 0..2 {
            let id = PhysicsId::new(100 + side);
            let (position, angle) = panel_pose(side as usize, 0.0);
            assert!(world.insert_body(
                BodyId::new(id, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::KinematicPosition,
                    position,
                    angle,
                    ..BodySpec::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(id, ColliderRole::PRIMARY, 0),
                    48.0,
                    2.0
                )]
            ));
        }
        let duck = duck.then(|| {
            BuoyantBody::insert(
                &mut world,
                PhysicsId::new(1),
                BodySpec {
                    position: Vec2::new(-52.0, depth as f32 + 5.0),
                    ccd_enabled: true,
                    can_sleep: false,
                    ..BodySpec::default()
                },
                HullShape::Box {
                    half_width: 3.0,
                    half_height: 1.2,
                },
                0.45,
            )
            .unwrap()
        });
        Self {
            water,
            opening: 0.0,
            load: depth,
            deferrals: 0,
            clearance_holds: 0,
            duck_exited: false,
            world,
            duck,
            floor: ResponsiveFloor::new(shape(), depth),
        }
    }

    pub fn duck_pose(&self) -> Option<(Vec2, f32)> {
        self.duck
            .as_ref()
            .and_then(|d| self.world.motion(d.body()))
            .map(|m| (m.position, m.angle))
    }

    /// Supply a depth-per-second rain load spread over the two floor panels.
    /// A deterministic source isolates floor response from random rain timing.
    pub fn step(&mut self, feed: f64) {
        assert!(feed.is_finite() && (0.0..=10.0).contains(&feed));
        if feed > 0.0 {
            for p in 0..2 {
                let spec = self.water.pools()[p].spec();
                let left = spec.left;
                let dx = spec.column_width;
                for i in 0..N {
                    self.water
                        .add_to_pool(p, left + (i as f64 + 0.5) * dx, feed * dx * DT)
                        .unwrap();
                }
            }
        }
        let hull = self.duck_pose().map(|(p, _)| (p, DUCK_RADIUS));
        self.floor.step(&mut self.water, DT, hull);
        self.opening = self.floor.opening;
        self.load = self.floor.load;
        self.deferrals = self.floor.deferrals;
        self.clearance_holds = self.floor.clearance_holds;
        for side in 0..2 {
            let (position, angle) = panel_pose(side, self.opening);
            assert!(self.world.set_next_kinematic_pose(
                BodyId::new(PhysicsId::new(100 + side as u64), BodyRole::PRIMARY),
                position,
                angle
            ));
        }
        self.water.step(DT).unwrap();
        self.world.clear_forces();
        if let Some(duck) = &self.duck {
            duck.apply_forces(
                &mut self.world,
                &self.water,
                BuoyancyConfig {
                    drag: 5.0,
                    ..BuoyancyConfig::default()
                },
                DT,
            )
            .unwrap();
        }
        self.world.step(DT as f32);
        if self.duck_pose().is_some_and(|(p, _)| p.y < -80.0) {
            let duck = self.duck.take().unwrap();
            assert!(self.world.remove_entity(duck.body().entity));
            self.duck_exited = true;
        }
    }

    pub fn frame(&self) -> RenderFrame {
        let mut frame = super::frame(&self.water, 0.0, -20.0, 140.0);
        if let Some((position, angle)) = self.duck_pose() {
            let (sin, cos) = angle.sin_cos();
            let points = [(-3.0, -1.2), (3.0, -1.2), (3.0, 1.2), (-3.0, 1.2)].map(|(x, y)| {
                RenderPoint::new(
                    position.x + x * cos - y * sin,
                    position.y + x * sin + y * cos,
                )
            });
            frame.push_primitive(
                20,
                RenderPrimitive::Polygon(RenderPolygon::filled(
                    points.into(),
                    RenderColor::rgb(1.0, 0.85, 0.12),
                )),
            );
        }
        frame
    }
}

fn panel_pose(side: usize, open: f64) -> (Vec2, f32) {
    shape().panel_pose(side, open)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_outfalls_keep_the_merged_jet_connected() {
        let mut f = ResponsiveFloorFixture::new(7.0, true);
        for _ in 0..120 {
            f.step(1.0);
        }
        let mut spans: Vec<_> = f
            .water
            .parcels()
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                if !matches!(
                    f.water.spill_source(i),
                    Some(engine_water::SpillSource::Junction { .. })
                ) {
                    return None;
                }
                if !(-50.0..-15.0).contains(&p.position.y) {
                    return None;
                }
                let ribbon = f
                    .water
                    .spill_ribbon(i)
                    .expect("continuous moving-floor mixed slice");
                let min = ribbon
                    .quads
                    .iter()
                    .flatten()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min);
                let max = ribbon
                    .quads
                    .iter()
                    .flatten()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                if max < -50.0 || min > -15.0 {
                    return None;
                }
                Some((min, max))
            })
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(spans.len() > 3);
        for pair in spans.windows(2) {
            assert!(
                pair[1].0 - pair[0].1 < 0.05,
                "disconnected moving jet: {pair:?}"
            );
        }
    }

    #[test]
    fn clearance_and_nearby_falling_water_delay_closing_without_fake_load() {
        let mut f = ResponsiveFloorFixture::new(7.0, true);
        for _ in 0..60 {
            f.step(0.0);
        }
        f.water.reclaim(); // Explicit dry test setup, not controller behavior.
        let opening = f.opening;
        let body = f.duck.as_ref().unwrap().body();
        for _ in 0..120 {
            // Pin a dynamic hull in the passage to exercise the interlock;
            // buoyancy correctly rejects a massless kinematic test substitute.
            assert!(
                f.world
                    .set_pose(body, Vec2::new(0.0, -(opening * DROP) as f32), 0.0, true)
            );
            assert!(f.world.set_velocity(body, Vec2::ZERO, 0.0, true));
            f.step(0.0);
            assert!(f.opening >= opening);
        }
        assert_eq!(f.load, 0.0);
        assert!(f.clearance_holds >= 120);
        f.duck = None;
        assert!(f.world.remove_entity(body.entity));
        for _ in 0..600 {
            f.step(0.0);
        }
        assert_eq!(f.opening, 0.0);

        let mut f = ResponsiveFloorFixture::new(7.0, false);
        for _ in 0..60 {
            f.step(0.0);
        }
        f.water.reclaim();
        let opening = f.opening;
        f.water
            .add_falling(engine_water::Parcel {
                position: Vec2::new(0.0, 15.0),
                velocity: Vec2::new(0.0, -10.0),
                volume: 2.0,
                duration: DT,
                horizontal_bounds: None,
            })
            .unwrap();
        f.step(0.0);
        assert_eq!(
            f.load, 0.0,
            "airborne water delays closing, not fictitious floor weight"
        );
        assert!(f.opening >= opening);
    }

    #[test]
    fn responsive_floor_loads_drain_and_reclose_without_lost_water() {
        for feed in [0.0, 0.2, 1.0, 3.0] {
            let mut fixture = ResponsiveFloorFixture::new(0.0, false);
            let mut maximum: f64 = 0.0;
            for tick in 0..3000 {
                fixture.step(if tick < 600 { feed } else { 0.0 });
                maximum = maximum.max(fixture.opening);
                let s = fixture.water.stats();
                assert!(
                    (s.injected - s.pooled - s.in_flight - s.drained).abs() < 1e-7,
                    "{s:?}"
                );
                assert_eq!(s.reclaimed, 0.0);
                assert!(s.parcels <= 256);
            }
            assert!(
                fixture.opening < 0.005,
                "feed={feed} open={} load={}",
                fixture.opening,
                fixture.load
            );
            if feed == 0.0 {
                assert_eq!(maximum, 0.0);
            } else {
                assert!(maximum > 0.05);
            }
            assert_eq!(fixture.deferrals, 0);
        }
    }

    #[test]
    fn floating_hull_clears_the_moving_floor_with_no_steering() {
        let mut f = ResponsiveFloorFixture::new(7.0, true);
        for tick in 0..2400 {
            f.step(if tick < 600 { 1.0 } else { 0.0 });
            let s = f.water.stats();
            assert!((s.injected - s.pooled - s.in_flight - s.drained).abs() < 1e-7);
            if let Some((p, _)) = f.duck_pose() {
                assert!(p.x.is_finite() && p.y.is_finite());
            }
            for side in 0..2 {
                let motion = f
                    .world
                    .motion(BodyId::new(
                        PhysicsId::new(100 + side as u64),
                        BodyRole::PRIMARY,
                    ))
                    .unwrap();
                let (sin, cos) = motion.angle.sin_cos();
                for c in f.water.pools()[side].columns() {
                    let x = (c.left + c.width * 0.5) as f32;
                    // The collider's local top y=2, rotated into world space.
                    let y = motion.position.y + (x - motion.position.x) * sin / cos + 2.0 / cos;
                    assert!((y as f64 - c.bed_at(x as f64)).abs() < 3e-5);
                }
            }
        }
        assert!(
            f.duck_exited,
            "pose={:?} open={} load={}",
            f.duck_pose(),
            f.opening,
            f.load
        );
        assert!(f.clearance_holds > 0);
        assert_eq!(f.world.body_count(), 2);
        assert_eq!(f.world.collider_count(), 2);
    }
}
