use super::*;
use engine_material::{SOFT_TISSUE, Section};
use engine_rapier::world::physics_activity;
use engine_rapier::world::weapon::{Reachable, Recipient};
use std::collections::BTreeMap;

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
    // Held fire leaves the CPUs driving in, so hold it only long enough to
    // see both move and keep the shots long.
    duel.set_firing(false);
    for _ in 0..120 {
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

/// A box of the host's scenery: in the duel's world and out of its fight.
fn scenery(world: &mut PhysicsWorld, id: u64, point: Vec2, half: Vec2, dynamic: bool) -> BodyId {
    let body = obstacle(world, id, point, half, dynamic);
    assert!(world.set_collision_groups(
        ColliderId::new(body.entity, ColliderRole::PRIMARY, 0),
        CollisionGroups {
            memberships: SCENERY,
            filter: u32::MAX,
        },
    ));
    body
}

#[test]
fn scenery_rests_on_the_hills_but_the_tanks_and_shells_pass_through_it() {
    let (mut duel, mut world) = quiet(1.0);
    let place = duel.placement().clone();
    let down = RayCastOptions {
        max_distance: 100.0,
        ..Default::default()
    };
    let tank = tank_at(&duel, &world, 1);
    let between = (tank_at(&duel, &world, 0).x + tank.x) / 2.0;
    let hill = duel
        .duel
        .cast_ray(&world, Vec2::new(between, 60.0), -Vec2::Y, down)
        .expect("the hill between the tanks");
    let block = scenery(
        &mut world,
        900_000,
        place.point_to_world(hill.point + Vec2::Y * 3.0),
        Vec2::splat(0.4),
        true,
    );
    // A slab through the right tank's hull and over it.
    scenery(
        &mut world,
        900_001,
        place.point_to_world(tank + Vec2::Y * 0.5),
        Vec2::new(4.0, 2.0),
        false,
    );
    let hull = world.motion(duel.duel.tanks[1].body).unwrap().position;
    for _ in 0..240 {
        step(&mut duel, &mut world);
    }
    // The block lands on the hill, and the duel's own ray down through it
    // finds the hill under it, not the block.
    let rest = world.motion(block).unwrap();
    assert!(rest.linear_velocity.length() < 0.2, "{rest:?}");
    let at = place.point_from_world(rest.position);
    let ground = duel
        .duel
        .cast_ray(&world, at + Vec2::Y * 10.0, -Vec2::Y, down)
        .expect("the hill under the block");
    assert_ne!(ground.collider.entity, block.entity);
    assert!(
        (at.y - 0.4 - ground.point.y).abs() < 0.15,
        "the block rests on the hill: {at:?} over {:?}",
        ground.point
    );
    // The tank inside the slab never felt it, and a shell falling through
    // the slab strikes the tank under it.
    let moved = world.motion(duel.duel.tanks[1].body).unwrap().position - hull;
    assert!(
        moved.length() < 0.02,
        "the slab moved the tank by {moved:?}"
    );
    drop_shell(
        &mut duel,
        &mut world,
        &mut armor::NoRecipients,
        1,
        8.0,
        20.0,
    );
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
fn a_host_points_a_tanks_barrel_either_way_and_below_the_level() {
    let config = config(GrainShape::Round);
    let mut world = world();
    let mut place = Placement::new(1000..100_000, Budget::required(config));
    place.scale = 2.0;
    let mut duel = HostedDuel::attach(&mut world, config, 42, place).unwrap();
    for direction in [
        Vec2::new(1.0, 0.5),
        Vec2::new(-1.0, -0.3),
        Vec2::new(-0.2, 1.0),
        Vec2::new(0.6, -0.8),
    ] {
        duel.point_barrel(1, direction);
        let [pivot, muzzle] = duel.tanks(&world)[1].barrel;
        let barrel = muzzle - pivot;
        assert!((barrel.length() - 2.8 * 2.0).abs() < 1e-4);
        assert!(
            (barrel.normalized() - direction.normalized()).length() < 1e-5,
            "{barrel:?} along {direction:?}"
        );
    }
    duel.retire(&mut world);
    assert_eq!(world.entity_ids().count(), 0);
}

#[test]
fn a_host_plots_a_shot_at_a_point_fires_it_once_the_duel_is_over_and_traces_its_burst() {
    for scale in [1.0, 3.0] {
        let (mut duel, mut world) = quiet(scale);
        duel.retire_tank(&mut world, 0).unwrap();
        let shooter = duel.tanks(&world)[1].motion.position;
        // A point just above the hill, 24 units in front of the winner.
        let ground = world
            .cast_ray(
                shooter + Vec2::new(-24.0, 20.0) * scale,
                Vec2::new(0.0, -1.0),
                RayCastOptions {
                    max_distance: 60.0 * scale,
                    ..Default::default()
                },
            )
            .unwrap()
            .point;
        let target = ground + Vec2::Y * (0.5 * scale);
        let mut aim = duel.aim_at(&world, 1, target);
        let mut ticks = 0;
        while !aim.plotted() {
            duel.plot(&world, &mut aim);
            ticks += 1;
        }
        assert!((5..=40).contains(&ticks), "{ticks} ticks to plot");
        let direction = aim.direction().expect("an arc that reaches the point");
        assert!(direction.x < 0.0 && direction.y > 0.0, "{direction:?}");
        let flight = aim.flight_ticks().unwrap();
        let (shots, observations) = (duel.shots(), duel.shot_observations().len());
        let source = duel.fire_aim(&mut world, &aim).expect("a shot");
        assert_eq!(duel.shots(), shots + 1);
        let [pivot, muzzle] = duel.tanks(&world)[1].barrel;
        assert!(((muzzle - pivot).normalized() - direction).length() < 1e-4);
        let traced = duel.landing(&world, source, None).expect("a burst");
        assert!(
            traced.distance_to(target) < 1.5 * scale,
            "{traced:?} for {target:?}"
        );
        let mut flown = 0_u32;
        let impact = loop {
            step(&mut duel, &mut world);
            flown += 1;
            if let Some(impact) = duel.impacts().iter().find(|i| i.source == source) {
                break *impact;
            }
            assert!(flown < 400, "the shell never burst");
        };
        assert!(
            flown.abs_diff(flight) <= 2,
            "flew {flown} ticks, plotted {flight}"
        );
        assert!(
            impact.point.distance_to(traced) < 0.05 * scale,
            "{:?} from {traced:?}",
            impact.point
        );
        assert_eq!(duel.active_shells(), 0);
        assert!(duel.landing(&world, source, None).is_none());
        // The CPUs neither learned from it nor took it for their own.
        assert_eq!(duel.shot_observations().len(), observations);
        assert!(duel.material_balance(&world).is_ok());
        duel.retire(&mut world);
        assert_eq!(world.entity_ids().count(), 0);
    }
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
        // Each tank's bodies, and a sprung tank's running gear in host units.
        for slot in 0..2 {
            let hull = duel.tanks(&world)[slot].body;
            let bodies: Vec<_> = duel.tank_bodies(slot).collect();
            assert_eq!(bodies[0], hull);
            let Some(gear) = duel.running_gear(&world, slot) else {
                assert!(!suspension);
                assert_eq!(bodies.len(), 1);
                continue;
            };
            assert!(suspension);
            assert_eq!(bodies.len(), 3);
            assert_eq!(gear.hull_half_extents, Vec2::new(1.6, 0.45) * scale);
            assert_eq!(gear.wheel_radius, WheelTuning::default().radius * scale);
            // In the hull's frame, a wheel hangs below each end.
            let hull = world.motion(hull).unwrap();
            let [left, right] = gear
                .wheels
                .map(|wheel| (wheel.position - hull.position).rotate_radians(-hull.angle));
            assert!(left.x < 0.0 && right.x > 0.0, "{left:?} {right:?}");
            assert!(left.y < 0.0 && right.y < 0.0, "{left:?} {right:?}");
        }
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

/// A hosted duel at `scale` whose tanks have settled and hold their fire.
fn quiet(scale: f32) -> (HostedDuel, PhysicsWorld) {
    let mut config = config(GrainShape::Hexagon);
    config.cpu_driving = false;
    let mut world = world();
    world.set_gravity(Vec2::new(0.0, -GRAVITY * scale));
    world.set_length_unit(scale);
    let mut placement = Placement::new(1000..100_000, Budget::required(config));
    placement.scale = scale;
    let mut duel = HostedDuel::attach(&mut world, config, 42, placement).unwrap();
    for tank in &mut duel.duel.tanks {
        tank.next_shot = u64::MAX;
    }
    for _ in 0..90 {
        step(&mut duel, &mut world);
    }
    (duel, world)
}

/// Where tank `slot` is, in the duel's units.
fn tank_at(duel: &HostedDuel, world: &PhysicsWorld, slot: usize) -> Vec2 {
    let body = duel.duel.tanks[slot].body;
    duel.placement()
        .point_from_world(world.motion(body).unwrap().position)
}

/// Drops a shell at `speed` from `height` above tank `slot` and steps until
/// it strikes the tank, with `recipients` in its way.
fn drop_shell(
    duel: &mut HostedDuel,
    world: &mut PhysicsWorld,
    recipients: &mut impl Recipients,
    slot: usize,
    height: f32,
    speed: f32,
) {
    duel.duel.shells.push(Shell {
        owner: 1 - slot,
        position: tank_at(duel, world, slot) + Vec2::Y * height,
        velocity: Vec2::new(0.0, -speed),
        age: 10,
        flight: None,
        source: armor::shell(500, None, speed),
        keys: Default::default(),
    });
    let impacts = duel.duel.impacts;
    for _ in 0..120 {
        let drives = duel.prepare_step(world).unwrap();
        world.step_with_surface_drives(DT, &drives);
        duel.finish_step_with(world, recipients).unwrap();
        if duel.duel.impacts > impacts {
            let body = duel.duel.tanks[slot].body;
            assert_eq!(duel.impacts()[0].collider.entity, body.entity);
            return;
        }
    }
    panic!("the shell never struck");
}

#[test]
fn scaled_duels_take_the_same_damage_from_the_same_shell() {
    let mut lost = Vec::new();
    for scale in [1.0, 3.0] {
        let (mut duel, mut world) = quiet(scale);
        drop_shell(
            &mut duel,
            &mut world,
            &mut armor::NoRecipients,
            0,
            6.0,
            30.0,
        );
        lost.push(100.0 - duel.duel.tanks[0].health);
    }
    assert!(lost[0] > 0.0);
    assert!((lost[0] - lost[1]).abs() < 0.01, "{lost:?}");
}

/// Moss the host keeps, and bodies outside the material model, in world
/// units at `scale`.
struct Field {
    scale: f32,
    /// The depth of every bed's section, a fifth of a unit unless a test
    /// spoils it.
    depth: f32,
    circles: Vec<Reachable>,
    moss: BTreeMap<u64, Region>,
    /// Each tuft's charges: how many, and the work they came to.
    charges: BTreeMap<u64, (u32, f32)>,
}

impl Field {
    fn new(scale: f32) -> Self {
        Self {
            scale,
            depth: 0.2,
            circles: Vec::new(),
            moss: BTreeMap::new(),
            charges: BTreeMap::new(),
        }
    }

    /// A bed of moss a unit across at `center`, in the duel's units, with
    /// `volume` of tissue.
    fn with_moss(mut self, duel: &HostedDuel, key: u64, center: Vec2, volume: f32) -> Self {
        let region = Region::new(SOFT_TISSUE, volume * self.scale.powi(3)).unwrap();
        self.moss.insert(key, region);
        self.circles.push(Reachable {
            recipient: Recipient::Tissue(key),
            center: duel.placement().point_to_world(center),
            radius: 0.5 * self.scale,
            thickness: 0.5 * self.scale,
        });
        self
    }

    /// A body outside the material model as thick as a burst's front, at
    /// `center` in the duel's units.
    fn with_obstacle(mut self, duel: &HostedDuel, center: Vec2, radius: f32) -> Self {
        let body = BodyId::new(PhysicsId::new(77_777), BodyRole::PRIMARY);
        self.circles.push(Reachable {
            recipient: Recipient::Unmodeled(body),
            center: duel.placement().point_to_world(center),
            radius: radius * self.scale,
            thickness: 2.0 * self.scale,
        });
        self
    }
}

impl Recipients for Field {
    fn near(&self, center: Vec2, radius: f32) -> Vec<Reachable> {
        self.circles
            .iter()
            .filter(|circle| circle.center.distance_to(center) - circle.radius <= radius)
            .copied()
            .collect()
    }

    fn region(&self, key: u64) -> Option<(Region, Section)> {
        let section = Section::new(0.5 * self.scale, self.depth * self.scale);
        Some((*self.moss.get(&key)?, section))
    }

    fn absorb(&mut self, key: u64, work: f32) {
        self.moss.get_mut(&key).unwrap().absorb(work).unwrap();
        let charge = self.charges.entry(key).or_default();
        charge.0 += 1;
        charge.1 += work;
    }
}

#[test]
fn a_shell_passes_the_hosts_moss_on_its_way_and_hits_as_it_would_without_it() {
    let mut struck = Vec::new();
    for moss in [false, true] {
        let (mut duel, mut world) = quiet(1.0);
        let tank = tank_at(&duel, &world, 1);
        let mut field = Field::new(1.0);
        if moss {
            // Beyond the burst's reach.
            field = field
                .with_moss(&duel, 1, tank + Vec2::Y * 9.0, 0.1)
                .with_moss(&duel, 2, tank + Vec2::Y * 11.0, 0.1);
        }
        drop_shell(&mut duel, &mut world, &mut field, 1, 14.0, 30.0);
        if moss {
            // Each tuft is charged once, though the shell crosses it over
            // several steps.
            assert_eq!(field.charges.keys().copied().collect::<Vec<_>>(), [1, 2]);
            for (calls, work) in field.charges.values() {
                assert_eq!(*calls, 1);
                assert!(*work > 0.0);
            }
        }
        let event = duel.impacts()[0];
        struck.push((duel.duel.tanks[1].health, event.ledger.payload));
    }
    assert!(struck[0].0 < 100.0);
    assert_eq!(struck[0], struck[1]);
}

#[test]
fn a_burst_shares_its_payload_with_the_hosts_moss_and_stops_at_its_bodies() {
    let (mut duel, mut world) = quiet(1.0);
    let tank = tank_at(&duel, &world, 1);
    let mut field = Field::new(1.0).with_moss(&duel, 1, tank + Vec2::new(3.0, 0.5), 20.0);
    let mut open = duel.clone();
    let mut flawed = duel.clone();
    drop_shell(&mut duel, &mut world, &mut field, 1, 6.0, 30.0);
    let event = duel.impacts()[0];
    let (calls, work) = field.charges[&1];
    assert_eq!(calls, 1);
    assert!(work > 0.0);
    assert!(field.moss[&1].integrity() > 0.0);
    let payload = event.ledger.payload;
    assert!(payload.delivered <= 28_000.0);
    // The payload went to the tank's armor and the moss, and nowhere twice.
    let armor = f64::from(event.absorbed[1]) - event.ledger.kinetic.absorbed;
    assert!((armor + f64::from(work) - payload.absorbed).abs() < 0.1);
    assert_eq!(event.unmodeled, 0.0);

    // A host body between a burst and the tank stops what meets it first.
    let center = tank - Vec2::X * 3.0;
    let mut shielded = open.clone();
    let release = |duel: &mut HostedDuel, recipients: &mut Field| {
        duel.duel.release_payload(
            &world,
            recipients,
            center,
            &mut armor::charge(9, None),
            &mut armor::Keys::default(),
        )
    };
    let open = release(&mut open, &mut Field::new(1.0));
    let mut obstacle = Field::new(1.0).with_obstacle(&duel, tank - Vec2::X * 2.4, 0.4);
    let shielded = release(&mut shielded, &mut obstacle);
    assert!(open.absorbed[1] > 0.0);
    assert!(shielded.absorbed[1] < open.absorbed[1]);
    assert_eq!(open.unmodeled, 0.0);
    assert!(shielded.unmodeled > 0.0);

    // Tissue the law refuses stops energy as a body does, and the tank still
    // takes the share that passes it.
    let mut spoiled = Field::new(1.0).with_moss(&duel, 2, tank - Vec2::X * 2.4, 20.0);
    spoiled.depth = 0.0;
    let flawed = release(&mut flawed, &mut spoiled);
    assert!(flawed.absorbed[1] > 0.0);
    assert!(flawed.absorbed[1] < open.absorbed[1]);
    assert!(flawed.unmodeled > 0.0);
    assert!(spoiled.charges.is_empty());
}

#[test]
fn the_hosts_moss_loses_the_same_share_of_its_capacity_at_any_scale() {
    let mut left = Vec::new();
    for scale in [1.0, 3.0] {
        let (mut duel, mut world) = quiet(scale);
        let tank = tank_at(&duel, &world, 1);
        // A bed a slow shell passes and the burst reaches, and one only
        // reached.
        let mut field = Field::new(scale)
            .with_moss(&duel, 1, tank + Vec2::Y * 4.5, 20.0)
            .with_moss(&duel, 2, tank + Vec2::new(4.5, 0.5), 20.0);
        drop_shell(&mut duel, &mut world, &mut field, 1, 6.0, 2.0);
        assert_eq!(field.charges[&1].0, 2);
        left.push(
            field
                .moss
                .values()
                .map(|moss| moss.integrity())
                .collect::<Vec<_>>(),
        );
    }
    assert!(
        left[0]
            .iter()
            .all(|&integrity| integrity > 0.0 && integrity < 1.0)
    );
    for (one, three) in left[0].iter().zip(&left[1]) {
        assert!((one - three).abs() < 1e-4, "{left:?}");
    }
}

#[test]
fn a_retired_tank_takes_no_share_and_its_airborne_shell_still_counts() {
    let mut config = config(GrainShape::Round);
    config.cpu_driving = false;
    let mut world = world();
    let placement = Placement::new(1000..100_000, Budget::required(config));
    let mut duel = HostedDuel::attach(&mut world, config, 42, placement).unwrap();
    for tank in &mut duel.duel.tanks {
        tank.next_shot = u64::MAX;
    }
    for _ in 0..30 {
        step(&mut duel, &mut world);
    }
    let firer = duel.duel.tanks[0].body.entity;
    let at = world.motion(duel.duel.tanks[0].body).unwrap().position;
    let target = world.motion(duel.duel.tanks[1].body).unwrap().position;
    duel.duel.shells.push(Shell {
        owner: 0,
        position: target + Vec2::Y * 6.0,
        velocity: Vec2::new(0.0, -30.0),
        age: 10,
        flight: None,
        source: armor::shell(500, Some(firer.value()), 30.0),
        keys: Default::default(),
    });
    duel.retire_tank(&mut world, 0).unwrap();
    let impacts = duel.duel.impacts;
    for _ in 0..60 {
        step(&mut duel, &mut world);
        if duel.duel.impacts > impacts {
            break;
        }
    }
    let event = duel.impacts()[0];
    assert_eq!(event.source.owner, firer);
    assert!(event.absorbed[1] > 0.0);
    assert!(duel.duel.tanks[1].health < 100.0);
    duel.duel.explode(&mut world, at + Vec2::Y);
    assert_eq!(duel.duel.tanks[0].armor.integrity(), 1.0);
    assert_eq!(duel.duel.tanks[0].health, 0.0);
}
