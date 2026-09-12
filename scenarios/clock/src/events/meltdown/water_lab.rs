//! Optional three-body demonstration; normal Meltdown stays particle/column only.
use crate::layout::Layout;
use engine_core::Vec2;
use engine_rapier::{
    buoyancy::{BuoyancyConfig, BuoyancyReport, BuoyantBody},
    world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
        PhysicsWorld, PhysicsWorldConfig,
    },
};
use engine_water::{Boundary, WaterWorld, immersion::HullShape};

pub(crate) struct LabBody {
    pub body: BuoyantBody,
    pub report: BuoyancyReport,
}

pub(crate) struct WaterLab {
    pub world: PhysicsWorld,
    /// Shared by rendering and colliders, so the banks cannot visually disagree.
    pub supports: Vec<(Vec2, Vec2)>,
    /// Cork box, floating ball, dense box (in that order).
    pub bodies: Vec<LabBody>,
}

impl WaterLab {
    pub fn new(water: &WaterWorld, layout: Layout) -> Self {
        let mut supports = Vec::with_capacity(7);
        for pool in water.pools() {
            let spec = pool.spec();
            let mut first = 0;
            for end in 1..=spec.bed.len() {
                if end == spec.bed.len() || spec.bed[end] != spec.bed[first] {
                    supports.push((
                        Vec2::new(
                            (spec.left + first as f64 * spec.column_width) as f32,
                            spec.bed[first] as f32 - 4.0,
                        ),
                        Vec2::new(
                            (spec.left + end as f64 * spec.column_width) as f32,
                            spec.bed[first] as f32,
                        ),
                    ));
                    first = end;
                }
            }
            for edge in 0..2 {
                if spec.boundaries[edge] == Boundary::Closed {
                    let (x, bed) = if edge == 0 {
                        (spec.left, spec.bed[0])
                    } else {
                        (
                            spec.left + spec.bed.len() as f64 * spec.column_width,
                            *spec.bed.last().unwrap(),
                        )
                    };
                    supports.push((
                        Vec2::new(x as f32 - 2.0, bed as f32 - 4.0),
                        Vec2::new(x as f32 + 2.0, bed as f32 + layout.pitch * 2.0),
                    ));
                }
            }
        }
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::new(0.0, -400.0),
            length_unit: layout.pitch,
            max_ccd_substeps: 2,
            collect_events: false,
            ..PhysicsWorldConfig::default()
        });
        world.reserve(4, 10, 0);
        let entity = PhysicsId::new(1000);
        let colliders: Vec<_> = supports
            .iter()
            .enumerate()
            .map(|(i, (min, max))| {
                let mut c = ColliderSpec::cuboid(
                    ColliderId::new(entity, ColliderRole::PRIMARY, i as u16),
                    (max.x - min.x) * 0.5,
                    (max.y - min.y) * 0.5,
                );
                c.local_position = (*min + *max) * 0.5;
                c
            })
            .collect();
        assert!(world.insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                ..BodySpec::default()
            },
            &colliders
        ));
        let pitch = layout.pitch.max(12.0);
        let upper = water.pools()[0].spec();
        let lower = water.pools()[1].spec();
        let x = upper.left + upper.column_width * upper.bed.len() as f64 * 0.4;
        let surface = water.pools()[0]
            .columns_in_range(x, x + 0.01)
            .next()
            .unwrap()
            .surface as f32;
        let fixtures = [
            (
                HullShape::Box {
                    half_width: pitch * 0.45,
                    half_height: pitch * 0.16,
                },
                0.35,
                Vec2::new(x as f32, surface + pitch * 0.55),
                0.35,
            ),
            (
                HullShape::Circle {
                    radius: pitch * 0.2,
                },
                0.55,
                Vec2::new(
                    (lower.left + lower.column_width * lower.bed.len() as f64 * 0.28) as f32,
                    -226.0 + pitch * 1.2,
                ),
                0.0,
            ),
            (
                HullShape::Box {
                    half_width: pitch * 0.18,
                    half_height: pitch * 0.18,
                },
                1.8,
                Vec2::new(
                    (lower.left + lower.column_width * lower.bed.len() as f64 * 0.70) as f32,
                    -226.0 + pitch * 1.3,
                ),
                0.2,
            ),
        ];
        let bodies = fixtures
            .into_iter()
            .enumerate()
            .map(|(i, (shape, density, position, angle))| LabBody {
                body: BuoyantBody::insert(
                    &mut world,
                    PhysicsId::new(i as u64 + 1),
                    BodySpec {
                        position,
                        angle,
                        ccd_enabled: true,
                        can_sleep: false,
                        ..BodySpec::default()
                    },
                    shape,
                    density,
                )
                .expect("bounded water-lab body"),
                report: BuoyancyReport::default(),
            })
            .collect();
        Self {
            world,
            supports,
            bodies,
        }
    }

    pub fn step(&mut self, water: &WaterWorld) {
        self.world.clear_forces();
        for b in &mut self.bodies {
            b.report = b
                .body
                .apply_forces(
                    &mut self.world,
                    water,
                    BuoyancyConfig::default(),
                    1.0 / 60.0,
                )
                .expect("valid water-lab coupling");
        }
        self.world.step(1.0 / 60.0);
    }
}
