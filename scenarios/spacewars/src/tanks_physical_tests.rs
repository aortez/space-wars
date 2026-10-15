//! Focused local material-frame probes of the canonical controller/assembly.
use super::*;
use engine_rapier::world::{
    BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsWorldConfig,
};

fn step(tanks: &mut Tanks, world: &mut PhysicsWorld) {
    tanks.begin_gravity_step();
    for body in tanks.bodies() {
        tanks.record_gravity(body, Vec2::new(0.0, -18.0));
    }
    tanks.prepare_controls(world, false);
    world.step(1.0 / 60.0);
    tanks.record_physics_step();
}
fn solid(
    world: &mut PhysicsWorld,
    id: u64,
    position: Vec2,
    angle: f32,
    half: Vec2,
    material: bool,
) -> BodyId {
    let entity = PhysicsId::new(id);
    let body = BodyId::new(entity, BodyRole::PRIMARY);
    let mut collider = ColliderSpec::cuboid(
        ColliderId::new(entity, ColliderRole::PRIMARY, 0),
        half.x,
        half.y,
    );
    collider.collision_groups = if material {
        crate::physics::terrain_spec().collision_groups
    } else {
        CollisionGroups::new(1 << 5, 1 << 14)
    };
    collider.solver_groups = collider.collision_groups;
    collider.friction = 0.9;
    assert!(world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            angle,
            ..Default::default()
        },
        &[collider]
    ));
    body
}
fn course(angle: f32) -> (PhysicsWorld, Tanks, TankId) {
    let (mut world, mut tanks, id) = unsettled_course(angle);
    for _ in 0..120 {
        step(&mut tanks, &mut world);
    }
    (world, tanks, id)
}

fn unsettled_course(angle: f32) -> (PhysicsWorld, Tanks, TankId) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -18.0),
        ..Default::default()
    });
    solid(
        &mut world,
        100,
        Vec2::ZERO,
        angle,
        Vec2::new(100.0, 0.5),
        true,
    );
    let mut tanks = Tanks::default();
    let id = tanks
        .insert(
            &mut world,
            RoverSpawnPose {
                wheel_center: Vec2::Y.rotate_radians(angle) * (0.5 + 0.75 + 0.08),
                up_angle: std::f32::consts::FRAC_PI_2 + angle,
            },
            crate::physics::tank_spec(),
        )
        .unwrap();
    tanks.command(
        &mut world,
        id,
        TankCommand {
            brake: true,
            ..Default::default()
        },
    );
    (world, tanks, id)
}
#[test]
fn real_slopes_drive_and_brake_in_the_contact_frame() {
    for angle in [-0.35, 0.0, 0.35] {
        let (mut world, mut tanks, id) = course(angle);
        let start = tanks.snapshot(&world, id).unwrap();
        assert!(start.support.is_some());
        let tangent = Vec2::X.rotate_radians(angle);
        tanks.command(
            &mut world,
            id,
            TankCommand {
                drive: 1.0,
                ..Default::default()
            },
        );
        let mut supported = 0;
        for _ in 0..180 {
            step(&mut tanks, &mut world);
            let s = tanks.snapshot(&world, id).unwrap();
            supported += usize::from(s.support.is_some());
        }
        let driven = tanks.snapshot(&world, id).unwrap();
        let progress =
            (driven.assembly.chassis.position - start.assembly.chassis.position).dot(tangent);
        eprintln!(
            "slope={angle} progress={progress} supported={supported} angle={}",
            driven.assembly.chassis.angle
        );
        assert!(progress > 3.0);
        assert!(supported > 160);
        tanks.command(
            &mut world,
            id,
            TankCommand {
                brake: true,
                ..Default::default()
            },
        );
        for _ in 0..120 {
            step(&mut tanks, &mut world);
        }
        let braked = tanks.snapshot(&world, id).unwrap();
        let support = braked.support.unwrap();
        let relative = (braked.assembly.chassis.linear_velocity - support.velocity).dot(tangent);
        eprintln!("slope={angle} brake={relative}");
        assert!(relative.abs() < 0.8);
    }
}
#[test]
fn ordinary_obstacle_and_step_refusal_preserve_support_without_pose_correction() {
    for (material, height) in [(false, 1.0), (true, 1.0), (true, 0.1)] {
        let (mut world, mut tanks, id) = course(0.0);
        solid(
            &mut world,
            101,
            Vec2::new(26.0, height / 2.0),
            0.0,
            Vec2::new(20.0, 0.5 + height / 2.0),
            material,
        );
        // Flush the real shape into the same completed-world query index.
        step(&mut tanks, &mut world);
        tanks.command(
            &mut world,
            id,
            TankCommand {
                drive: 1.0,
                ..Default::default()
            },
        );
        let mut blocked = 0;
        for _ in 0..300 {
            step(&mut tanks, &mut world);
            let s = tanks.snapshot(&world, id).unwrap();
            blocked += usize::from(s.drive_blocked);
            assert!(
                s.assembly.chassis.position.y > 0.5,
                "must not tunnel through floor"
            );
        }
        let end = tanks.snapshot(&world, id).unwrap();
        eprintln!(
            "material={material} step={height} blocked={blocked} end={:?}",
            end.assembly.chassis.position
        );
        assert!(end.support.is_some());
        if height > 0.6 {
            assert!(blocked > 120);
            assert!(end.assembly.chassis.position.x < 4.0);
        } else {
            assert!(end.assembly.chassis.position.x > 7.0);
        }
    }
}

#[test]
fn canonical_tanks_receive_real_tank_ship_and_pilot_collision_impulses() {
    use engine_rapier::spaceling::{SpacelingAssembly, SpacelingSpec};
    for actor in ["tank", "ship", "pilot"] {
        let initial = crate::SpacewarsScenario::init_terrain_fixture(42);
        let mut host = crate::physics::SpacewarsPhysics::new(500.0, &initial.ships, None, &[]);
        let mut tanks = Tanks::default();
        let target = tanks
            .insert(
                &mut host.world,
                RoverSpawnPose {
                    wheel_center: Vec2::new(500.0, 500.0),
                    up_angle: std::f32::consts::FRAC_PI_2,
                },
                crate::physics::tank_spec(),
            )
            .unwrap();
        let snapshot = tanks.snapshot(&host.world, target).unwrap();
        let target_entity = tanks.actors[&target].assembly.entity();
        let incoming: Vec<BodyId> = match actor {
            "tank" => {
                let id = tanks
                    .insert(
                        &mut host.world,
                        RoverSpawnPose {
                            wheel_center: Vec2::new(490.0, 500.0),
                            up_angle: std::f32::consts::FRAC_PI_2,
                        },
                        crate::physics::tank_spec(),
                    )
                    .unwrap();
                tanks.actors[&id].assembly.bodies().to_vec()
            }
            "ship" => {
                let body = host.ship_body(0);
                assert!(host.world.set_pose(
                    body,
                    snapshot.assembly.chassis.position - Vec2::X * 18.0,
                    0.0,
                    true
                ));
                vec![body]
            }
            _ => {
                let pilot = SpacelingAssembly::insert(
                    &mut host.world,
                    PhysicsId::new(700),
                    snapshot.assembly.chassis.position - Vec2::X * 8.0,
                    0.0,
                    SpacelingSpec {
                        collision_groups: crate::physics::spaceling_collision_groups(),
                        ..Default::default()
                    },
                )
                .unwrap();
                vec![pilot.body()]
            }
        };
        for &body in &incoming {
            assert!(host.world.set_velocity(body, Vec2::X * 6.0, 0.0, true));
        }
        let incoming_entity = incoming[0].entity;
        let mut impulse = 0.0_f32;
        let mut contacts = 0;
        for _ in 0..240 {
            // Collision-only probe: no controller, gravity or pose transport.
            host.world.step(1.0 / 60.0);
            for event in host.world.contact_events() {
                if (event.collider_a.entity == target_entity
                    && event.collider_b.entity == incoming_entity)
                    || (event.collider_b.entity == target_entity
                        && event.collider_a.entity == incoming_entity)
                {
                    contacts += 1;
                    impulse = impulse.max(event.impulse_magnitude);
                }
            }
        }
        let after = tanks.snapshot(&host.world, target).unwrap();
        let displacement = after
            .assembly
            .chassis
            .position
            .distance_to(snapshot.assembly.chassis.position);
        let speed = host.world.motion(incoming[0]).unwrap().linear_velocity.x;
        eprintln!(
            "collision={actor} contacts={contacts} impulse={impulse} target_displacement={displacement} incoming_speed={speed}"
        );
        assert!(
            contacts > 0 && impulse > 0.01,
            "actual reciprocal solver response required"
        );
        assert!(displacement > 0.01, "target must physically respond");
        assert!(speed < 5.9, "incoming body must exchange momentum");
        let count = tanks.len();
        let before = host.world.body_count();
        tanks.clear(&mut host.world);
        assert_eq!(host.world.body_count() + count * 3, before);
    }
}

#[test]
fn actually_released_round_and_hexagon_grains_exchange_momentum_with_a_tank() {
    use crate::tank_fixture::{FixtureCase, TankFixture};
    use engine_common::DirtMode;
    for mode in [DirtMode::Round, DirtMode::Angular] {
        let mut fixture = TankFixture::new(42, mode, FixtureCase::Drive);
        for _ in 0..481 {
            fixture.prepare_step();
            fixture.step();
            fixture.sample();
        }
        let id = fixture.state.tank_ids().nth(1).unwrap();
        let before = fixture.state.tank_snapshot(id).unwrap();
        let support = before.support.unwrap();
        let target_entity = fixture.state.physics.tanks.actors[&id].assembly.entity();
        let grain = fixture
            .state
            .loose_terrain()
            .unwrap()
            .iter()
            .next()
            .unwrap();
        let shape = grain.shape();
        let body = grain.body();
        let grain_entity = body.entity;
        assert_eq!(
            shape,
            if mode == DirtMode::Round {
                engine_rapier::terrain::GrainShape::Round
            } else {
                engine_rapier::terrain::GrainShape::Hexagon
            }
        );
        let world = &mut fixture.state.physics.world;
        // Isolate a collision using a real conserved released actor and its
        // authoritative shape/groups; relocation is setup, never tank driving.
        world.clear_forces();
        world.set_gravity(Vec2::ZERO);
        world.set_pose(
            body,
            before.assembly.chassis.position + support.normal * 5.0,
            0.0,
            true,
        );
        world.set_velocity(body, -support.normal * 8.0, 0.0, true);
        let mut contacts = 0;
        let mut impulse = 0.0_f32;
        for _ in 0..90 {
            world.step(1.0 / 60.0);
            for event in world.contact_events() {
                if (event.collider_a.entity == target_entity
                    && event.collider_b.entity == grain_entity)
                    || (event.collider_b.entity == target_entity
                        && event.collider_a.entity == grain_entity)
                {
                    contacts += 1;
                    impulse = impulse.max(event.impulse_magnitude);
                }
            }
        }
        let speed = world
            .motion(body)
            .unwrap()
            .linear_velocity
            .dot(-support.normal);
        let after = fixture.state.tank_snapshot(id).unwrap();
        let change = (after.assembly.chassis.linear_velocity
            - before.assembly.chassis.linear_velocity)
            .length();
        eprintln!(
            "released_shape={shape:?} contacts={contacts} impulse={impulse} incoming_speed={speed} tank_velocity_change={change}"
        );
        assert!(contacts > 0 && impulse > 0.01);
        assert!(speed < 7.5 && change > 0.01);
    }
}

#[test]
fn repeated_host_clear_readmission_and_physics_restore_preserve_actor_handles() {
    use engine_common::{DirtMode, Scenario};
    let tick = |state: &mut crate::SpacewarsState| {
        crate::SpacewarsScenario::step(state, &[], std::time::Duration::from_secs_f32(1.0 / 60.0));
    };
    for mode in [DirtMode::Round, DirtMode::Angular] {
        let mut state = crate::SpacewarsScenario::init_terrain_fixture(42);
        state
            .enable_loose_terrain(crate::surface_sortie::dirt_config(mode))
            .unwrap();
        state.ships[0].position = Vec2::new(700.0, 700.0);
        state.ships[1].position = Vec2::new(800.0, 500.0);
        tick(&mut state);
        let mut previous = Vec::new();
        for cycle in 0..4 {
            let a = state
                .place_tank(state.planets[0].position + Vec2::Y * 65.0, -Vec2::Y)
                .unwrap();
            tick(&mut state);
            let b = state
                .place_tank(state.planets[0].position - Vec2::X * 65.0, Vec2::X)
                .unwrap();
            tick(&mut state);
            for &old in &previous {
                assert!(!state.command_tank(old, TankCommand::default()));
            }
            let handles = state.physics.tanks.bodies();
            let mut restored = state.clone();
            restored.physics.world =
                PhysicsWorld::from_snapshot_bytes(&state.physics.world.snapshot_bytes().unwrap())
                    .unwrap();
            for _ in 0..30 {
                for (id, command) in [
                    (
                        a,
                        TankCommand {
                            drive: 1.0,
                            turret_angle: 0.4,
                            ..Default::default()
                        },
                    ),
                    (
                        b,
                        TankCommand {
                            brake: true,
                            ..Default::default()
                        },
                    ),
                ] {
                    assert!(state.command_tank(id, command));
                    assert!(restored.command_tank(id, command));
                }
                tick(&mut state);
                tick(&mut restored);
                assert_eq!(state.tank_snapshot(a), restored.tank_snapshot(a));
                assert_eq!(state.tank_snapshot(b), restored.tank_snapshot(b));
            }
            let counts = (
                state.physics.world.body_count(),
                state.physics.world.collider_count(),
                state.physics.world.joint_count(),
            );
            state.clear_tanks();
            restored.clear_tanks();
            assert_eq!(state.physics.world.body_count() + 6, counts.0);
            assert_eq!(state.physics.world.collider_count() + 6, counts.1);
            assert_eq!(state.physics.world.joint_count() + 4, counts.2);
            for body in handles {
                assert!(state.physics.world.motion(body).is_none());
                assert!(restored.physics.world.motion(body).is_none());
            }
            previous.extend([a, b]);
            tick(&mut state);
            eprintln!(
                "reset mode={mode:?} cycle={cycle} removed bodies=6 colliders=6 joints=4; serialized replay matched"
            );
        }
    }
}

#[test]
fn steep_material_slope_refuses_motor_intent_without_righting_the_body() {
    let (mut world, mut tanks, id) = unsettled_course(0.9);
    let mut reference_world = world.clone();
    let mut reference_tanks = tanks.clone();
    tanks.command(
        &mut world,
        id,
        TankCommand {
            drive: 1.0,
            ..Default::default()
        },
    );
    reference_tanks.command(
        &mut reference_world,
        id,
        TankCommand {
            brake: true,
            ..Default::default()
        },
    );
    let mut blocked = 0;
    let mut steep_contacts = 0;
    let mut rotated = 0.0_f32;
    for tick in 0..120 {
        tanks.begin_gravity_step();
        for body in tanks.bodies() {
            tanks.record_gravity(body, Vec2::new(0.0, -18.0));
        }
        let motions: Vec<_> = tanks
            .bodies()
            .into_iter()
            .map(|body| (body, world.motion(body).unwrap()))
            .collect();
        let support = tanks.snapshot(&world, id).unwrap().support;
        let steep = support.is_some_and(|c| c.normal.dot(Vec2::Y) < 0.7);
        steep_contacts += usize::from(steep);
        tanks.prepare_controls(&mut world, false);
        // A refusal may set motors and wake bodies, but cannot correct pose or
        // velocity. Ordinary solver rotation happens only in the following step.
        for &(body, before) in &motions {
            assert_eq!(world.motion(body).unwrap(), before);
        }
        if steep {
            assert!(tanks.snapshot(&world, id).unwrap().drive_blocked);
        }
        world.step(1.0 / 60.0);
        tanks.record_physics_step();
        step(&mut reference_tanks, &mut reference_world);
        let sample = tanks.snapshot(&world, id).unwrap();
        blocked += usize::from(sample.drive_blocked);
        for &(body, before) in &motions {
            let after = world.motion(body).unwrap();
            let delta = after.angle - before.angle;
            rotated = rotated.max(delta.sin().atan2(delta.cos()).abs());
            assert_eq!(
                after,
                reference_world.motion(body).unwrap(),
                "refused driving must retain the ordinary brake-only physical trajectory, tick={tick}"
            );
        }
        if tick % 30 == 0 {
            eprintln!(
                "steep tick={tick} support={} angle={} spin={} blocked={}",
                sample.support.is_some(),
                sample.assembly.chassis.angle,
                sample.assembly.chassis.angular_velocity,
                sample.drive_blocked
            );
        }
    }
    eprintln!(
        "steep slope=0.9 blocked={blocked} steep_contacts={steep_contacts} maximum_physical_rotation_per_step={rotated}"
    );
    assert!(blocked > 100 && steep_contacts > 10);
}

#[test]
fn route_budget_counts_filtered_colliders_and_refuses_without_pose_changes() {
    let (mut world, mut tanks, id) = course(0.0);
    for n in 0..SPATIAL_BUDGET {
        solid(
            &mut world,
            10000 + n as u64,
            Vec2::new(1000.0 + n as f32 * 3.0, 1000.0),
            0.0,
            Vec2::new(0.5, 0.5),
            false,
        );
    }
    tanks.command(
        &mut world,
        id,
        TankCommand {
            drive: 1.0,
            ..Default::default()
        },
    );
    let before: Vec<_> = tanks.bodies().iter().map(|b| world.motion(*b)).collect();
    tanks.prepare_controls(&mut world, false);
    assert_eq!(tanks.work.spatial_traversal, SPATIAL_BUDGET);
    assert_eq!(tanks.work.spatial_exhaustions, 1);
    assert!(tanks.snapshot(&world, id).unwrap().drive_blocked);
    assert_eq!(
        before,
        tanks
            .bodies()
            .iter()
            .map(|b| world.motion(*b))
            .collect::<Vec<_>>()
    );
}

#[test]
fn compound_route_budget_preserves_obstacles_and_never_returns_partial_clearance() {
    use engine_rapier::world::{ColliderShape, CompoundChild};
    let (mut world, _, _) = course(0.0);
    let entity = PhysicsId::new(200);
    let mut collider =
        ColliderSpec::cuboid(ColliderId::new(entity, ColliderRole::PRIMARY, 0), 1.0, 1.0);
    collider.shape = ColliderShape::Compound {
        children: (0..40)
            .map(|n| CompoundChild {
                shape: ColliderShape::Cuboid {
                    half_width: 0.5,
                    half_height: 0.5,
                },
                position: Vec2::new(n as f32 * 2.0, 0.0),
                angle: 0.0,
            })
            .collect(),
    };
    collider.collision_groups = crate::physics::terrain_spec().collision_groups;
    assert!(world.insert_body(
        BodyId::new(entity, BodyRole::PRIMARY),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(10.0, 2.0),
            ..Default::default()
        },
        &[collider]
    ));
    let query = |budget: &mut usize| {
        world.tank_route_query(
            Vec2::new(10.0, 5.0),
            -Vec2::Y,
            RayCastOptions {
                max_distance: 10.0,
                collision_groups: crate::physics::tank_material_groups(),
                ..Default::default()
            },
            Vec2::new(10.0, 2.0),
            0.0,
            0.5,
            0.5,
            crate::physics::tank_spec().collision_groups,
            budget,
        )
    };
    let mut short = 12;
    assert_eq!(query(&mut short), Err(()));
    assert_eq!(short, 0);
    let mut enough = 200;
    let (hit, clear) = query(&mut enough).unwrap();
    assert_eq!(hit.unwrap().collider.entity, entity);
    assert!(!clear, "compound obstacle must remain occupied");
    assert!(200 - enough >= 40, "all compound children are charged");
}

#[test]
fn genuinely_detached_fragment_support_tracks_motion_and_disappears_after_cut() {
    use engine_rapier::terrain::TerrainFragment;
    use engine_terrain::{Brush, CellCoord, EditMode, Material, MaterialId, Terrain, TerrainEdit};
    let (mut world, mut tanks, id) = course(0.0);
    assert!(world.remove_entity(PhysicsId::new(100)));
    let mut terrain = Terrain::generate(
        120,
        12,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |p| {
            if p.x < 65 || p.y >= 4 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    // Sever the narrow connection, transferring the smaller shelf via the same
    // connectivity operation used by planetary edits.
    terrain
        .apply(TerrainEdit {
            brush: Brush::Capsule {
                start: CellCoord::new(65, 0),
                end: CellCoord::new(65, 12),
                radius: 0,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    let detached = terrain.detach_disconnected().unwrap().pop().unwrap();
    let parent = engine_rapier::world::BodyMotion {
        position: Vec2::ZERO,
        angle: 0.0,
        linear_velocity: Vec2::ZERO,
        angular_velocity: 0.0,
    };
    let mut fragment = TerrainFragment::insert(
        &mut world,
        PhysicsId::new(300),
        detached,
        parent,
        Vec2::ZERO,
        crate::physics::terrain_spec(),
    )
    .unwrap();
    let body = fragment.assembly.body();
    // Align the detached shelf's real upper face with the existing wheel line.
    let pose = world.motion(body).unwrap();
    let top = fragment
        .terrain
        .cell_center(CellCoord::new(10, fragment.terrain.height() as i32 - 1))
        .y
        + 0.5;
    world.set_pose(
        body,
        Vec2::new(
            -fragment.terrain.cell_center(CellCoord::new(10, 0)).x,
            0.5 - top,
        ),
        pose.angle,
        true,
    );
    // Keep a heavy detached slab supported while the tank settles, then release
    // both into inertial translation/rotation without prescribed frame transport.
    solid(
        &mut world,
        301,
        Vec2::new(0.0, 0.5 - fragment.terrain.height() as f32 - 0.5),
        0.0,
        Vec2::new(100.0, 0.5),
        true,
    );
    for _ in 0..120 {
        step(&mut tanks, &mut world);
    }
    let settled = tanks.snapshot(&world, id).unwrap();
    assert_eq!(settled.support.unwrap().collider.entity, fragment.id());
    tanks.command(
        &mut world,
        id,
        TankCommand {
            drive: 1.0,
            ..Default::default()
        },
    );
    tanks.prepare_controls(&mut world, false);
    assert!(
        !tanks.snapshot(&world, id).unwrap().drive_blocked,
        "detached material must pass lookahead without admitting unrelated solids"
    );
    let initial_fragment = world.motion(body).unwrap();
    world.set_velocity(body, Vec2::new(2.0, 0.0), 0.04, true);
    let mut contacts = 0;
    for _ in 0..15 {
        step(&mut tanks, &mut world);
        if let Some(support) = tanks.snapshot(&world, id).unwrap().support {
            if support.collider.entity == fragment.id() {
                contacts += 1;
                assert!(
                    (support.velocity - world.velocity_at_point(body, support.position).unwrap())
                        .length()
                        < 0.001
                );
            }
        }
    }
    eprintln!(
        "detached support contacts={contacts}; motion={:?}",
        world.motion(body).unwrap()
    );
    assert!(contacts > 5);
    assert!(world.motion(body).unwrap().position.x > initial_fragment.position.x + 0.1);
    let mut free_world = world.clone();
    let mut free_tanks = tanks.clone();
    free_world.remove_entity(PhysicsId::new(301));
    free_world.set_velocity(body, Vec2::new(2.0, 0.0), 0.04, true);
    for _ in 0..15 {
        step(&mut free_tanks, &mut free_world);
    }
    assert!(free_world.motion(body).unwrap().angle.abs() > 0.001);
    let before_cut = tanks.snapshot(&world, id).unwrap();
    assert_eq!(before_cut.support.unwrap().collider.entity, fragment.id());
    let before_cut = before_cut.assembly.chassis;
    fragment
        .terrain
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(10, 4),
                radius: 100,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    fragment.geometry.refresh(&fragment.terrain);
    fragment
        .assembly
        .synchronize(&mut world, &fragment.terrain, &fragment.geometry)
        .unwrap();
    for _ in 0..15 {
        step(&mut tanks, &mut world);
    }
    let after_cut = tanks.snapshot(&world, id).unwrap();
    assert!(after_cut.support.is_none());
    assert!(after_cut.assembly.chassis.position.y < before_cut.position.y - 0.5);
}

#[test]
fn naturally_released_grain_piles_support_drive_and_brake_for_both_shapes() {
    use engine_common::DirtMode;
    use engine_rapier::terrain::{LooseTerrain, PreparedRelease, TerrainAssembly, TerrainBodyMut};
    use engine_terrain::{
        Brush, CellCoord, EditMode, Material, MaterialId, Terrain, TerrainEdit, TerrainGeometry,
    };
    for mode in [DirtMode::Round, DirtMode::Angular] {
        let (mut world, mut tanks, old) = unsettled_course(0.0);
        tanks.remove(&mut world, old);
        let mut terrain = Terrain::generate(
            80,
            1,
            0.5,
            vec![Material {
                id: MaterialId(1),
                hardness: 100,
            }],
            |_| MaterialId(1),
        )
        .unwrap();
        let spec = crate::physics::terrain_spec();
        let mut geometry = TerrainGeometry::with_surface(&terrain, spec.surface);
        let mut assembly = TerrainAssembly::insert(
            &mut world,
            PhysicsId::new(500),
            BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(0.0, 1.5),
                ..Default::default()
            },
            &terrain,
            &geometry,
            spec,
        )
        .unwrap();
        let mut config = crate::surface_sortie::dirt_config(mode);
        config.max_grains = 512;
        let mut loose = LooseTerrain::new(config).unwrap();
        let plan = PreparedRelease::new(
            &terrain,
            &[TerrainEdit {
                brush: Brush::Circle {
                    center: CellCoord::new(40, 2),
                    radius: 100,
                },
                mode: EditMode::Damage(255),
            }],
        )
        .unwrap();
        let mut next = 1000;
        loose
            .commit(
                &mut world,
                TerrainBodyMut {
                    terrain: &mut terrain,
                    geometry: &mut geometry,
                    assembly: &mut assembly,
                },
                plan,
                &mut next,
            )
            .unwrap();
        assert_eq!(loose.len(), 80);
        for _ in 0..120 {
            world.step(1.0 / 60.0);
        }
        let id = tanks
            .insert(
                &mut world,
                RoverSpawnPose {
                    wheel_center: Vec2::new(0.0, 3.7),
                    up_angle: std::f32::consts::FRAC_PI_2,
                },
                crate::physics::tank_spec(),
            )
            .unwrap();
        let mut grain_support = 0;
        for _ in 0..240 {
            step(&mut tanks, &mut world);
            if tanks
                .snapshot(&world, id)
                .unwrap()
                .support
                .is_some_and(|s| loose.iter().any(|g| g.body().entity == s.collider.entity))
            {
                grain_support += 1;
            }
        }
        let start = tanks.snapshot(&world, id).unwrap();
        tanks.command(
            &mut world,
            id,
            TankCommand {
                drive: 1.0,
                ..Default::default()
            },
        );
        let mut unblocked = 0;
        for _ in 0..120 {
            step(&mut tanks, &mut world);
            let snapshot = tanks.snapshot(&world, id).unwrap();
            unblocked += usize::from(!snapshot.drive_blocked);
            if snapshot
                .support
                .is_some_and(|s| loose.iter().any(|g| g.body().entity == s.collider.entity))
            {
                grain_support += 1;
            }
        }
        let driven = tanks.snapshot(&world, id).unwrap();
        tanks.command(
            &mut world,
            id,
            TankCommand {
                brake: true,
                ..Default::default()
            },
        );
        for _ in 0..180 {
            step(&mut tanks, &mut world);
        }
        let braked = tanks.snapshot(&world, id).unwrap();
        eprintln!(
            "natural grains mode={mode:?} support={grain_support} unblocked={unblocked} progress={} brake={}",
            driven.assembly.chassis.position.x - start.assembly.chassis.position.x,
            braked.assembly.chassis.linear_velocity.x
        );
        assert!(grain_support > 30);
        assert!(unblocked > 30);
        assert!(driven.assembly.chassis.position.x > start.assembly.chassis.position.x + 0.5);
        assert!(braked.assembly.chassis.linear_velocity.x.abs() < 0.5);
        loose.audit(&world).unwrap();
    }
}

#[test]
fn host_admission_exhaustion_and_full_capacity_do_no_partial_insertion() {
    use engine_common::Scenario;
    let mut state = crate::SpacewarsScenario::init_terrain_fixture(42);
    crate::SpacewarsScenario::step(
        &mut state,
        &[],
        std::time::Duration::from_secs_f32(1.0 / 60.0),
    );
    for n in 0..ADMISSION_BUDGET {
        solid(
            &mut state.physics.world,
            20000 + n as u64,
            Vec2::new(1000.0 + n as f32 * 3.0, 1000.0),
            0.0,
            Vec2::new(0.5, 0.5),
            false,
        );
    }
    let counts = (
        state.physics.world.body_count(),
        state.physics.world.collider_count(),
        state.physics.world.joint_count(),
    );
    assert!(
        state
            .place_tank(Vec2::new(496.0, 565.0), -Vec2::Y)
            .is_none()
    );
    assert_eq!(
        state.tank_admission_work(),
        TankAdmissionWork {
            traversal: ADMISSION_BUDGET,
            exhausted: true,
            capacity_rejected: false
        }
    );
    assert_eq!(state.tank_ids().count(), 0);
    assert_eq!(
        counts,
        (
            state.physics.world.body_count(),
            state.physics.world.collider_count(),
            state.physics.world.joint_count()
        )
    );
    for n in 0..MAX_TANKS {
        assert!(
            state
                .physics
                .tanks
                .insert(
                    &mut state.physics.world,
                    RoverSpawnPose {
                        wheel_center: Vec2::new(100.0 + n as f32 * 10.0, 200.0),
                        up_angle: std::f32::consts::FRAC_PI_2
                    },
                    crate::physics::tank_spec()
                )
                .is_some()
        );
    }
    let bodies = state.physics.world.body_count();
    assert!(
        state
            .place_tank(Vec2::new(f32::NAN, 0.0), Vec2::ZERO)
            .is_none()
    );
    assert_eq!(
        state.tank_admission_work(),
        TankAdmissionWork {
            traversal: 0,
            exhausted: false,
            capacity_rejected: true
        }
    );
    assert_eq!(state.physics.world.body_count(), bodies);
}

fn edited_planet_course(
    mode: engine_common::DirtMode,
    grade: f32,
    step_height: f32,
) -> (crate::SpacewarsState, TankId) {
    use engine_common::Scenario;
    use engine_rapier::terrain::TerrainAssembly;
    use engine_terrain::{
        Brush, CellCoord, EditMode, Material, MaterialId, Terrain, TerrainEdit, TerrainGeometry,
    };
    let mut state = crate::SpacewarsScenario::init_terrain_fixture(42);
    state.ships[0].position = Vec2::new(700.0, 700.0);
    state.ships[1].position = Vec2::new(800.0, 500.0);
    state
        .enable_loose_terrain(crate::surface_sortie::dirt_config(mode))
        .unwrap();
    state.terrain.surface = engine_terrain::TerrainSurface::Interpolated;
    state.planets[0].radius = 24.0;
    state.planets[0].mass = crate::body_mass(24.0);
    state.physics =
        crate::physics::SpacewarsPhysics::new(500.0, &state.ships, None, &state.planets);
    state.physics.material_planets.insert(0);
    let field = Terrain::generate(
        64,
        64,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            let p = Vec2::new((c.x as f32 + 0.5) - 32.0, (c.y as f32 + 0.5) - 32.0);
            if p.length() < 22.8 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let geometry = TerrainGeometry::with_surface(&field, state.terrain.surface);
    let entity = crate::physics::planet_entity(0);
    state.physics.world.remove_entity(entity);
    let assembly = TerrainAssembly::insert(
        &mut state.physics.world,
        entity,
        BodySpec {
            kind: BodyKind::KinematicPosition,
            position: state.planets[0].position,
            ..Default::default()
        },
        &field,
        &geometry,
        engine_rapier::terrain::TerrainSpec {
            surface: state.terrain.surface,
            ..crate::physics::terrain_spec()
        },
    )
    .unwrap();
    let terrain = state.terrain.planets.get_mut(&0).unwrap();
    terrain.hash = field.hash();
    terrain.field = field;
    terrain.geometry = geometry;
    terrain.assembly = assembly;
    // Real queued cuts form a connected slope/step in the planetary field.
    for x in 0..64 {
        let px = (x as f32 + 0.5) - 32.0;
        let roof = 16.0 + px * grade.tan() + if px >= 0.0 { step_height } else { 0.0 };
        let y = (roof + 32.0 + 0.5).floor() as i32;
        if y < 64 && y >= 0 {
            state
                .queue_planet_edit(
                    0,
                    TerrainEdit {
                        brush: Brush::Capsule {
                            start: CellCoord::new(x, y),
                            end: CellCoord::new(x, 63),
                            radius: 0,
                        },
                        mode: EditMode::Remove,
                    },
                )
                .unwrap();
        }
    }
    crate::SpacewarsScenario::step(
        &mut state,
        &[],
        std::time::Duration::from_secs_f32(1.0 / 60.0),
    );
    assert!(state.terrain_removed_cells() > 0);
    // Signed samples describe the edited plane, agree with every conserved
    // cell, and use the host's existing interpolated material geometry.
    let t = state.terrain.planets.get_mut(&0).unwrap();
    t.field = t
        .field
        .clone()
        .with_surface_distances(|p| {
            let roof = 16.0 + p.x * grade.tan() + if p.x >= 0.0 { step_height } else { 0.0 };
            (22.8 - p.length()).min((roof - p.y) * grade.cos())
        })
        .unwrap();
    t.geometry = TerrainGeometry::with_surface(&t.field, state.terrain.surface);
    // with_surface_distances is an initialization API, not a revisioned edit.
    // Recreate the fixture assembly so no old generation can suppress geometry.
    state.physics.world.remove_entity(entity);
    t.assembly = TerrainAssembly::insert(
        &mut state.physics.world,
        entity,
        BodySpec {
            kind: BodyKind::KinematicPosition,
            position: state.planets[0].position,
            angle: state.planets[0].wrapper_angle,
            ..Default::default()
        },
        &t.field,
        &t.geometry,
        engine_rapier::terrain::TerrainSpec {
            surface: state.terrain.surface,
            ..crate::physics::terrain_spec()
        },
    )
    .unwrap();
    t.hash = t.field.hash();
    state.physics.material_queries_dirty = true;
    crate::SpacewarsScenario::step(
        &mut state,
        &[],
        std::time::Duration::from_secs_f32(1.0 / 60.0),
    );

    let x = if grade.abs() > 0.7 { 0.0 } else { -6.0 };
    let point = Vec2::new(x, 16.0 + x * grade.tan());
    let up = Vec2::new(-grade.sin(), grade.cos());
    let planet = state.planets[0];
    // The explicit assembly API supplies a clearance above the sculpted
    // staircase; the ray-based convenience placer may conservatively reject
    // its single-normal pose when an uphill wheel would overlap material.
    let wheel_center = planet.position
        + (point + up * (crate::physics::tank_spec().wheel_radius + 0.65))
            .rotate_radians(planet.wrapper_angle);
    let id = state
        .physics
        .tanks
        .insert(
            &mut state.physics.world,
            RoverSpawnPose {
                wheel_center,
                up_angle: up.y.atan2(up.x) + planet.wrapper_angle,
            },
            crate::physics::tank_spec(),
        )
        .unwrap();
    (state, id)
}

fn planetary_step(state: &mut crate::SpacewarsState) {
    use engine_common::Scenario;
    state.planets[0].position += Vec2::X * (0.3 / 60.0);
    crate::SpacewarsScenario::step(state, &[], std::time::Duration::from_secs_f32(1.0 / 60.0));
}

#[test]
fn edited_moving_planet_slopes_steps_and_steep_refusal_for_both_dirt_modes() {
    use engine_common::DirtMode;
    for mode in [DirtMode::Round, DirtMode::Angular] {
        for (grade, height) in [(-0.25, 0.0), (0.25, 0.0), (0.0, 0.1), (0.0, 1.5)] {
            let (mut state, id) = edited_planet_course(mode, grade, height);
            state.command_tank(
                id,
                TankCommand {
                    brake: true,
                    ..Default::default()
                },
            );
            for _ in 0..120 {
                planetary_step(&mut state);
            }
            let start = state.tank_snapshot(id).unwrap();
            assert!(
                start.support.is_some(),
                "mode={mode:?} grade={grade} step={height} snapshot={start:?}"
            );
            let planet = state.planets[0];
            let start_local = (start.assembly.chassis.position - planet.position)
                .rotate_radians(-planet.wrapper_angle);
            state.command_tank(
                id,
                TankCommand {
                    drive: 1.0,
                    ..Default::default()
                },
            );
            let mut supported = 0;
            let mut blocked = 0;
            let drive_steps = 360;
            for _ in 0..drive_steps {
                let bodies = state.physics.tanks.bodies();
                let before: Vec<_> = bodies
                    .iter()
                    .map(|b| state.physics.world.motion(*b))
                    .collect();
                state
                    .physics
                    .tanks
                    .prepare_controls(&mut state.physics.world, false);
                assert_eq!(
                    before,
                    bodies
                        .iter()
                        .map(|b| state.physics.world.motion(*b))
                        .collect::<Vec<_>>()
                );
                let steps = state.tank_step_audit().physics_steps;
                planetary_step(&mut state);
                assert_eq!(state.tank_step_audit().physics_steps, steps + 1);
                assert_eq!(state.tank_step_audit().duplicate_gravity_applications, 0);
                let sample = state.tank_snapshot(id).unwrap();
                supported += usize::from(sample.support.is_some());
                blocked += usize::from(sample.drive_blocked);
                let p = state.planets[0];
                let local = (sample.assembly.chassis.position - p.position)
                    .rotate_radians(-p.wrapper_angle);
                let field = state.planet_terrain(0).unwrap();
                assert!(
                    field
                        .local_to_cell(local)
                        .and_then(|c| field.cell(c))
                        .is_none_or(|c| c.material == engine_terrain::MaterialId::VOID),
                    "chassis must not tunnel into material"
                );
            }
            let end = state.tank_snapshot(id).unwrap();
            let planet = state.planets[0];
            let local = (end.assembly.chassis.position - planet.position)
                .rotate_radians(-planet.wrapper_angle);
            eprintln!(
                "planet mode={mode:?} grade={grade} step={height} progress={} supported={supported} blocked={blocked} local={local:?}",
                local.x - start_local.x
            );
            eprintln!("planet work={:?}", state.tank_work());
            assert_eq!(supported, drive_steps);
            if height > 0.6 {
                assert!(blocked > 100);
                assert!(local.x < 0.5);
            } else {
                assert!(local.x - start_local.x > 4.0);
                if height > 0.0 {
                    assert!(local.x > 1.0);
                }
            }
            state.command_tank(
                id,
                TankCommand {
                    brake: true,
                    ..Default::default()
                },
            );
            let mut brake_samples = 0;
            let mut speed_sum = 0.0;
            for _ in 0..180 {
                planetary_step(&mut state);
                let stopped = state.tank_snapshot(id).unwrap();
                if let Some(support) = stopped.support {
                    let relative = (stopped.assembly.chassis.linear_velocity - support.velocity)
                        .dot(Vec2::new(support.normal.y, -support.normal.x));
                    speed_sum += relative;
                    brake_samples += 1;
                } else {
                    let p = state.planets[0];
                    let local = (stopped.assembly.chassis.position - p.position)
                        .rotate_radians(-p.wrapper_angle);
                    eprintln!(
                        "brake unsupported mode={mode:?} grade={grade} local={local:?} motion={:?}",
                        stopped.assembly.chassis
                    );
                }
            }
            let relative = speed_sum / brake_samples.max(1) as f32;
            eprintln!(
                "planet mode={mode:?} grade={grade} step={height} relative_brake={relative} brake_samples={brake_samples}"
            );
            assert_eq!(brake_samples, 180);
            assert!(relative.abs() < 0.8);
        }
        let (mut state, id) = edited_planet_course(mode, 1.1, 0.0);
        state.command_tank(
            id,
            TankCommand {
                drive: 1.0,
                ..Default::default()
            },
        );
        let mut steep = 0;
        let mut refused = 0;
        for _ in 0..60 {
            // Sliding changes the radial gravity frame: a steep plane can become
            // safe farther downhill. Compare the brake trajectory at each refusal
            // rather than requiring permanent refusal on a remembered slope.
            let mut reference = state.clone();
            reference.command_tank(
                id,
                TankCommand {
                    brake: true,
                    ..Default::default()
                },
            );
            state
                .physics
                .tanks
                .prepare_controls(&mut state.physics.world, false);
            let blocked = state.tank_snapshot(id).unwrap().drive_blocked;
            planetary_step(&mut state);
            planetary_step(&mut reference);
            let snapshot = state.tank_snapshot(id).unwrap();
            let steep_contact = snapshot
                .support
                .is_some_and(|c| c.normal.dot(-snapshot.local_gravity.normalized()) < 0.7);
            steep += usize::from(steep_contact);
            if steep_contact {
                assert!(
                    snapshot.drive_blocked,
                    "actual steep support must refuse drive"
                );
            }
            if blocked && snapshot.drive_blocked {
                refused += 1;
                for body in state.physics.tanks.bodies() {
                    assert_eq!(
                        state.physics.world.motion(body),
                        reference.physics.world.motion(body)
                    );
                }
            }
        }
        eprintln!("planet mode={mode:?} steep_contacts={steep} refused={refused}");
        assert!(steep > 0);
        assert!(refused > 40);
    }
}
