use super::*;
use engine_rapier::world::physics_activity;

fn world() -> PhysicsWorld {
    PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -GRAVITY),
        solver_iterations: 8,
        internal_stabilization_iterations: 2,
        max_ccd_substeps: 4,
        ..Default::default()
    })
}

fn config(shape: GrainShape) -> ScorchedConfig {
    ScorchedConfig {
        demo: true,
        shape,
        cpu_policy: [CpuPolicy::Reactive; 2],
        ..Default::default()
    }
    .with_destruction(BankStrength::Weak)
}

fn step(duel: &mut HostedDuel, world: &mut PhysicsWorld) {
    let before = physics_activity().steps;
    let drives = duel.prepare_step(world).unwrap();
    assert_eq!(physics_activity().steps, before);
    world.step_with_surface_drives(DT, &drives);
    duel.finish_step(world).unwrap();
    assert_eq!(physics_activity().steps, before + 1);
}

#[test]
fn wider_hills_reserve_their_chunks_and_support_custom_landings() {
    let mut world = world();
    let config = config(GrainShape::Round);
    let mut place = Placement::new(20_000..60_000, Budget::required(config));
    place.terrain_width = 385;
    place.spawn_x = [-80.0, 80.0];
    assert!(matches!(
        HostedDuel::attach(&mut world, config, 42, place.clone()),
        Err(HostError::Capacity)
    ));
    assert_eq!(world.entity_ids().count(), 0);
    place.available = Budget::for_terrain(config.max_grains, false, place.terrain_width);
    let mut duel = HostedDuel::attach(&mut world, config, 42, place).unwrap();
    assert_eq!(
        duel.tanks(&world).map(|tank| tank.motion.position.x),
        [-80.0, 80.0]
    );
    duel.set_firing(false);
    for _ in 0..600 {
        step(&mut duel, &mut world);
    }
    assert!(
        duel.duel
            .driving_diagnostics()
            .iter()
            .all(|drive| drive.powered_ticks > 0),
        "both tanks must drive from the wider landing positions"
    );
    duel.set_firing(true);
    for _ in 0..1200 {
        step(&mut duel, &mut world);
    }
    assert!(duel.duel.shots > 0);
    assert!(
        duel.duel.tanks.iter().any(|tank| tank.speed > 48.0),
        "wider duels retain enough firing range"
    );
    assert!(duel.material_balance(&world).is_ok());
    duel.retire(&mut world);
    assert_eq!(world.entity_ids().count(), 0);
}

fn obstacle(world: &mut PhysicsWorld, id: u64, point: Vec2, half: Vec2, dynamic: bool) -> BodyId {
    let entity = PhysicsId::new(id);
    let body = BodyId::new(entity, BodyRole::PRIMARY);
    assert!(world.insert_body(
        body,
        BodySpec {
            position: point,
            kind: if dynamic {
                BodyKind::Dynamic
            } else {
                BodyKind::Fixed
            },
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            half.x,
            half.y
        )]
    ));
    body
}

#[test]
fn shared_duel_matches_standalone_with_both_dirt_shapes() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        let config = config(shape);
        let mut standalone = ScorchedState::new(config, 42);
        let mut world = world();
        let before = physics_activity().steps;
        let mut duel = HostedDuel::attach(
            &mut world,
            config,
            42,
            Placement::new(0..1_000_000, Budget::required(config)),
        )
        .unwrap();
        assert_eq!(
            physics_activity().steps,
            before,
            "attach must not step the host"
        );
        // Standalone has always performed this one initial physics step.
        world.step(DT);
        for tick in 0..1200 {
            standalone.advance(0.0);
            step(&mut duel, &mut world);
            assert_eq!(
                duel.observe(&world).payload,
                ScorchedScenario::observe(&standalone).payload,
                "{shape:?} tick {tick}"
            );
        }
        assert!(duel.shots() > 2);
        assert!(duel.duel.impacts > 0);
        assert!(duel.duel.loose.deposited_cells() > 0);
        assert!(
            duel.duel
                .driving_diagnostics()
                .iter()
                .any(|d| d.distance_while_driven > 1.0)
        );
        assert!(duel.tanks(&world).iter().any(|tank| tank.health < 100.0));
        assert_eq!(
            duel.material_balance(&world).unwrap(),
            standalone.audit().unwrap()
        );
    }
}

#[test]
fn translated_duel_hits_foreign_obstacles_and_leaves_them_owned_by_the_host() {
    let config = config(GrainShape::Hexagon);
    let mut world = world();
    let origin = Vec2::new(128.0, 64.0);
    let wall = obstacle(
        &mut world,
        900_000,
        origin + Vec2::new(0.0, 20.0),
        Vec2::new(0.5, 50.0),
        false,
    );
    let ball = obstacle(
        &mut world,
        900_001,
        origin + Vec2::new(80.0, 10.0),
        Vec2::splat(0.5),
        true,
    );
    let mut placement = Placement::new(1000..100_000, Budget::required(config));
    placement.origin = origin;
    let mut duel = HostedDuel::attach(&mut world, config, 42, placement).unwrap();
    let foreign_pose = world.motion(wall);
    let mut hits = 0;
    for _ in 0..600 {
        step(&mut duel, &mut world);
        for hit in duel.impacts() {
            if hit.collider.entity == wall.entity {
                hits += 1;
                assert!((hit.point.x - origin.x).abs() <= 0.51);
                assert!([1010, 1011].contains(&hit.source.owner.value()));
                assert!(hit.source.id > 0);
            }
        }
    }
    assert!(hits > 0, "real sweeps must hit the unrelated wall");
    assert!(world.motion(ball).unwrap().position.y < origin.y);
    assert_eq!(world.motion(wall), foreign_pose);
    duel.material_balance(&world).unwrap();
    duel.retire(&mut world);
    duel.retire(&mut world);
    assert_eq!(world.body_count(), 2);
    assert_eq!(world.collider_count(), 2);
    assert_eq!(world.joint_count(), 0);
    assert_eq!(world.motion(wall), foreign_pose);
    assert!(world.contains_body(ball));
}

#[test]
fn foreign_support_and_sprung_assembly_cleanup_share_the_real_world() {
    for suspension in [false, true] {
        let mut config = config(GrainShape::Round);
        config.suspension = suspension;
        config.cpu_driving = false;
        let mut world = world();
        let bridge = obstacle(
            &mut world,
            900_000,
            Vec2::new(-27.0, hill(-27.0, 42) + 0.5),
            Vec2::new(4.0, 0.5),
            false,
        );
        for _ in 0..3 {
            let mut duel = HostedDuel::attach(
                &mut world,
                config,
                42,
                Placement::new(1000..100_000, Budget::required(config)),
            )
            .unwrap();
            duel.set_firing(false);
            for _ in 0..120 {
                step(&mut duel, &mut world);
            }
            assert!(duel.tanks(&world)[0].supported);
            let contacts_bridge = world
                .collider_ids()
                .filter(|c| c.entity == duel.tanks(&world)[0].body.entity)
                .any(|collider| {
                    world
                        .surface_contacts(collider)
                        .any(|c| c.collider.entity == bridge.entity)
                });
            assert!(contacts_bridge, "tank must stand on a host-owned body");
            duel.retire(&mut world);
            assert_eq!(
                (
                    world.body_count(),
                    world.collider_count(),
                    world.joint_count()
                ),
                (1, 1, 0)
            );
            assert!(world.identity_fault().is_none());
        }
    }
}

#[test]
fn fire_cutoff_preserves_airborne_shots_after_firing_tank_removal_and_replays() {
    for suspension in [false, true] {
        let mut config = config(GrainShape::Hexagon);
        config.suspension = suspension;
        let mut world = world();
        let mut duel = HostedDuel::attach(
            &mut world,
            config,
            42,
            Placement::new(1000..100_000, Budget::required(config)),
        )
        .unwrap();
        for _ in 0..360 {
            step(&mut duel, &mut world);
            if duel.active_shells() > 0 {
                break;
            }
        }
        assert!(duel.active_shells() > 0);
        let shots = duel.shots();
        let source = duel.duel.tanks[0].body.entity;
        duel.set_firing(false);
        duel.retire_tank(&mut world, 0).unwrap();
        assert!(!world.contains_entity(source));
        assert!(duel.tanks(&world)[0].bounds.is_none());
        assert_eq!(duel.winner(), Some(1));
        let mut replay = duel.clone();
        let mut replay_world = world.clone();
        let mut ticks = 0;
        while duel.active_shells() > 0 {
            step(&mut duel, &mut world);
            step(&mut replay, &mut replay_world);
            assert_eq!(
                duel.observe(&world).payload,
                replay.observe(&replay_world).payload
            );
            assert_eq!(duel.impacts(), replay.impacts());
            assert_eq!(duel.shots(), shots);
            ticks += 1;
            assert!(ticks <= SHELL_LIFETIME_TICKS);
        }
        assert!(!duel.shot_observations().is_empty());
        assert_eq!(duel.shot_observations()[0].launch.owner, 0);
        assert_eq!(world.snapshot_bytes(), replay_world.snapshot_bytes());
        duel.retire(&mut world);
        assert!(
            duel.tanks(&world)
                .iter()
                .all(|t| !t.present && t.bounds.is_none())
        );
        duel.observe(&world);
        assert_eq!(world.body_count(), 0);
        assert_eq!(world.joint_count(), 0);
    }
}

#[test]
fn admission_refuses_before_mutating_and_reservations_bound_material_work() {
    let mut config = config(GrainShape::Round);
    config.max_grains = 32;
    let mut world = world();
    obstacle(
        &mut world,
        1010,
        Vec2::new(80.0, 0.0),
        Vec2::splat(1.0),
        false,
    );
    let before = world.snapshot_bytes();
    assert_eq!(
        HostedDuel::attach(
            &mut world,
            config,
            42,
            Placement::new(1000..100_000, Budget::required(config))
        )
        .err(),
        Some(HostError::Namespace)
    );
    assert_eq!(world.snapshot_bytes(), before);
    let mut placement = Placement::new(2000..100_000, Budget::required(config));
    placement.available.bodies -= 1;
    assert_eq!(
        HostedDuel::attach(&mut world, config, 42, placement).err(),
        Some(HostError::Capacity)
    );
    assert_eq!(world.snapshot_bytes(), before);
    let mut placement = Placement::new(2000..100_000, Budget::required(config));
    placement.reserved_grains = 28;
    let mut duel = HostedDuel::attach(&mut world, config, 42, placement).unwrap();
    let balance = duel.material_balance(&world).unwrap();
    let tank = duel.tanks(&world)[0];
    duel.duel
        .explode(&mut world, tank.motion.position - Vec2::Y * 2.0);
    assert_eq!(duel.material_balance(&world).unwrap(), balance);
    assert!(duel.tanks(&world)[0].health < tank.health);
    assert_eq!(duel.reserved_grains(), 28);
    assert_eq!(duel.duel.loose.config().max_grains, 4);
    let conversion_ids = duel.conversion_ids();
    assert_eq!(
        (conversion_ids.end - conversion_ids.start) as usize,
        duel.reserved_grains()
    );
    let conversion = obstacle(
        &mut world,
        conversion_ids.start,
        Vec2::new(85.0, 0.0),
        Vec2::splat(0.25),
        false,
    );
    duel.duel.next_id = conversion_ids.start - 1;
    duel.duel.explode(&mut world, tank.motion.position);
    assert_eq!(duel.material_balance(&world).unwrap(), balance);
    assert!(world.contains_entity(PhysicsId::new(1010)));
    duel.retire(&mut world);
    assert_eq!(
        (
            world.body_count(),
            world.collider_count(),
            world.joint_count()
        ),
        (2, 2, 0)
    );
    assert!(world.contains_body(conversion));
    assert!(world.identity_fault().is_none());
}

#[test]
fn host_gravity_and_step_order_keep_controls_and_material_in_the_shared_world() {
    let config = config(GrainShape::Round);
    let mut world = world();
    world.set_gravity(Vec2::new(0.0, -f32::from_bits(1)));
    let before = world.snapshot_bytes();
    assert_eq!(
        HostedDuel::attach(
            &mut world,
            config,
            42,
            Placement::new(1000..100_000, Budget::required(config)),
        )
        .err(),
        Some(HostError::Gravity),
    );
    assert_eq!(world.snapshot_bytes(), before);
    world.set_gravity(Vec2::new(0.0, -400.0));
    let foreign = obstacle(
        &mut world,
        900_000,
        Vec2::new(80.0, 0.0),
        Vec2::splat(0.5),
        true,
    );
    let mut duel = HostedDuel::attach(
        &mut world,
        config,
        42,
        Placement::new(1000..100_000, Budget::required(config)),
    )
    .unwrap();
    duel.set_firing(false);
    duel.set_driving(false);
    assert_eq!(duel.finish_step(&mut world), Err(HostError::StepOrder));
    let drives = duel.prepare_step(&mut world).unwrap();
    assert_eq!(
        duel.prepare_step(&mut world).err(),
        Some(HostError::StepOrder)
    );
    world.step_with_surface_drives(DT, &drives);
    duel.finish_step(&mut world).unwrap();
    assert_eq!(duel.finish_step(&mut world), Err(HostError::StepOrder));
    for _ in 0..120 {
        step(&mut duel, &mut world);
    }
    assert_eq!(duel.shots(), 0);
    for tank in duel.tanks(&world) {
        assert!(tank.supported);
        assert!(tank.bounds.unwrap().contains(tank.motion.position));
    }
    // A real crater creates material bodies after the solve; those bodies
    // must use the same acceleration as the existing tanks on the next step.
    let tank = duel.tanks(&world)[0];
    duel.duel
        .explode(&mut world, tank.motion.position - Vec2::Y * 2.0);
    assert!(duel.grains().count() > 0);
    for gravity in [-400.0, -180.0] {
        world.set_gravity(Vec2::new(0.0, gravity));
        step(&mut duel, &mut world);
        assert_eq!(world.gravity(), Vec2::new(0.0, gravity));
        assert_eq!(world.gravity_scale(foreign), Some(1.0));
        for body in duel.body_ids() {
            assert_eq!(world.gravity_scale(body), Some(GRAVITY / -gravity));
        }
        duel.material_balance(&world).unwrap();
    }
    duel.retire(&mut world);
    assert_eq!(
        duel.prepare_step(&mut world).err(),
        Some(HostError::Retired)
    );
    assert_eq!(world.body_count(), 1);
    assert!(world.contains_body(foreign));
}

#[test]
fn scaled_duels_drive_deform_and_replay_with_native_mass_accounting() {
    for (scale, suspension) in [(0.75, false), (3.0, false), (3.0, true)] {
        let mut config = config(GrainShape::Hexagon);
        config.suspension = suspension;
        let mut world = world();
        world.set_gravity(Vec2::new(0.0, -400.0));
        world.set_length_unit(scale);
        let mut placement = Placement::new(1000..100_000, Budget::required(config));
        placement.scale = scale;
        placement.origin = Vec2::new(128.0, 64.0);
        let mut duel = HostedDuel::attach(&mut world, config, 42, placement).unwrap();
        for _ in 0..600 {
            step(&mut duel, &mut world);
        }
        let mut replay = duel.clone();
        let mut replay_world = world.clone();
        for _ in 0..600 {
            step(&mut duel, &mut world);
            step(&mut replay, &mut replay_world);
            assert_eq!(
                duel.observe(&world).payload,
                replay.observe(&replay_world).payload
            );
            assert_eq!(duel.impacts(), replay.impacts());
        }
        assert!(duel.shots() > 0, "scale={scale}, sprung={suspension}");
        assert!(duel.duel.impacts > 0);
        assert!(
            duel.duel
                .driving_diagnostics()
                .iter()
                .any(|drive| drive.distance_while_driven > 1.0)
        );
        assert_eq!(duel.terrain()[0].terrain.cell_size(), CELL_SIZE * scale);
        duel.material_balance(&world).unwrap();
        duel.retire(&mut world);
        assert_eq!(
            (
                world.body_count(),
                world.collider_count(),
                world.joint_count()
            ),
            (0, 0, 0)
        );
    }
}
