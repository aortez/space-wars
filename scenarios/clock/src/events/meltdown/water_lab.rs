//! Optional buoyancy/displacement fixtures; normal Meltdown stays particle/column only.
use crate::{ClockWaterLab, layout::Layout};
use engine_core::Vec2;
use engine_rapier::{
    buoyancy::{BuoyancyConfig, BuoyancyReport, BuoyantBody},
    world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId,
        PhysicsWorld, PhysicsWorldConfig,
    },
};
use engine_water::{Boundary, WaterWorld, displacement::DisplacementBox, immersion::HullShape};

pub(crate) struct LabBody {
    pub body: BuoyantBody,
    pub report: BuoyancyReport,
    pub palette: usize,
}

pub(crate) struct Piston {
    pub body: BodyId,
    pub half_extents: Vec2,
    x: f32,
    raised_y: f32,
    lowered_y: f32,
}

pub(crate) struct WaterLab {
    pub world: PhysicsWorld,
    /// Shared by rendering and colliders, so the banks cannot visually disagree.
    pub supports: Vec<(Vec2, Vec2)>,
    /// Explicitly opted-in buoyant bodies. The controlled piston is separate.
    pub bodies: Vec<LabBody>,
    pub piston: Option<Piston>,
    pub reference_y: Option<f32>,
    displacement_enabled: bool,
    /// Index into `bodies`; exactly one box can own this tank's occupancy input.
    dynamic_displacer: Option<usize>,
}

impl WaterLab {
    pub fn new(water: &WaterWorld, layout: Layout, mode: ClockWaterLab) -> Self {
        let displacement = mode.is_tank();
        let initial_level = water.pools()[0].columns().next().unwrap().surface as f32;
        let initial_depth = initial_level - water.pools()[0].spec().bed[0] as f32;
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
                        Vec2::new(
                            x as f32 + 2.0,
                            bed as f32
                                + if displacement {
                                    initial_depth * 2.4
                                } else {
                                    layout.pitch * 2.0
                                },
                        ),
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
        if displacement {
            let spec = water.pools()[0].spec();
            let width = (spec.column_width * spec.bed.len() as f64) as f32;
            let half_extents = Vec2::new(width * 0.10, initial_depth * 0.35);
            let piston = Piston {
                body: BodyId::new(PhysicsId::new(10), BodyRole::PRIMARY),
                half_extents,
                x: -width * 0.20,
                raised_y: spec.bed[0] as f32 + initial_depth * 1.85,
                lowered_y: spec.bed[0] as f32 + initial_depth * 0.45,
            };
            if !mode.is_dynamic_tank() {
                assert!(world.insert_body(
                    piston.body,
                    BodySpec {
                        kind: BodyKind::KinematicPosition,
                        position: Vec2::new(piston.x, piston.raised_y),
                        ..BodySpec::default()
                    },
                    &[ColliderSpec::cuboid(
                        ColliderId::new(piston.body.entity, ColliderRole::PRIMARY, 0),
                        half_extents.x,
                        half_extents.y
                    )]
                ));
            }
            let observer = BuoyantBody::insert(
                &mut world,
                PhysicsId::new(1),
                BodySpec {
                    position: Vec2::new(width * 0.25, initial_level + initial_depth * 0.1),
                    can_sleep: false,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                HullShape::Circle {
                    radius: initial_depth * 0.15,
                },
                0.55,
            )
            .unwrap();
            let mut bodies = vec![LabBody {
                body: observer,
                report: BuoyancyReport::default(),
                palette: 1,
            }];
            if mode.is_dynamic_tank() {
                bodies.push(LabBody {
                    body: BuoyantBody::insert(
                        &mut world,
                        piston.body.entity,
                        BodySpec {
                            position: Vec2::new(piston.x, piston.raised_y),
                            lock_rotation: true,
                            can_sleep: false,
                            ccd_enabled: true,
                            ..BodySpec::default()
                        },
                        HullShape::Box {
                            half_width: half_extents.x,
                            half_height: half_extents.y,
                        },
                        if mode == ClockWaterLab::Sinking {
                            1.8
                        } else {
                            0.55
                        },
                    )
                    .unwrap(),
                    report: BuoyancyReport::default(),
                    palette: if mode == ClockWaterLab::Sinking { 2 } else { 0 },
                });
            }
            return Self {
                world,
                supports,
                bodies,
                piston: (!mode.is_dynamic_tank()).then_some(piston),
                reference_y: Some(initial_level),
                displacement_enabled: mode.has_displacement(),
                dynamic_displacer: mode.is_dynamic_tank().then_some(1),
            };
        }
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
                palette: i,
            })
            .collect();
        Self {
            world,
            supports,
            bodies,
            piston: None,
            reference_y: None,
            displacement_enabled: false,
            dynamic_displacer: None,
        }
    }

    pub fn prepare_displacement(&mut self, water: &mut WaterWorld, tick: u64) {
        self.sync_dynamic_displacement(water);
        let Some(piston) = &self.piston else {
            return;
        };
        let t = match tick {
            0..=60 => 0.0,
            61..=150 => (tick - 60) as f32 / 90.0,
            151..=240 => 1.0,
            241..=330 => 1.0 - (tick - 240) as f32 / 90.0,
            _ => 0.0,
        };
        let t = t * t * (3.0 - 2.0 * t);
        let target = Vec2::new(
            piston.x,
            piston.raised_y + (piston.lowered_y - piston.raised_y) * t,
        );
        self.world.set_next_kinematic_pose(piston.body, target, 0.0);
        water
            .set_displacer(
                0,
                self.displacement_enabled.then_some(DisplacementBox {
                    center: target,
                    half_extents: piston.half_extents,
                }),
            )
            .expect("one bounded axis-aligned box in a closed tank");
    }

    fn sync_dynamic_displacement(&self, water: &mut WaterWorld) {
        if let Some(index) = self.dynamic_displacer {
            if self.displacement_enabled {
                self.bodies[index]
                    .body
                    .sync_displacement(&self.world, water, 0)
                    .expect("one rotation-locked dynamic box in a closed tank");
            } else {
                water.set_displacer(0, None).expect("one-way tank control");
            }
        }
    }

    pub fn step(&mut self, water: &mut WaterWorld) {
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
        // Keep final-frame water occupancy aligned with the actual collider.
        // No second water/physics step and no liquid added by this submission.
        self.sync_dynamic_displacement(water);
    }
}
