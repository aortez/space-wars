use super::*;
use crate::{
    terrain::{TerrainAssembly, TerrainSpec},
    world::*,
};
use engine_core::{
    Vec2,
    plants::{Config, Plants},
};
use engine_terrain::{Material, MaterialId, Terrain, TerrainGeometry};

fn fixture() -> (PhysicsWorld, Terrain, TerrainGeometry, TerrainAssembly) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let terrain = Terrain::generate(
        8,
        8,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 10,
        }],
        |c| {
            if c.x < 2 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let geometry = TerrainGeometry::new(&terrain);
    let assembly = TerrainAssembly::insert(
        &mut world,
        PhysicsId::new(1),
        BodySpec::default(),
        &terrain,
        &geometry,
        TerrainSpec::default(),
    )
    .unwrap();
    (world, terrain, geometry, assembly)
}
fn roots<'a>(
    w: &'a PhysicsWorld,
    t: &'a Terrain,
    g: &'a TerrainGeometry,
    a: &'a TerrainAssembly,
) -> RootUptake<'a> {
    RootUptake {
        world: w,
        terrain: t,
        geometry: g,
        assembly: a,
        dirt: MaterialId(1),
        rates: [1000; 2],
        availability: [1000; 2],
        tick_ms: 1000,
    }
}
fn shoot(a: &TerrainAssembly, id: u64, cell: [i32; 2]) -> Tissue {
    Tissue {
        id,
        organism: 1,
        policy: 1,
        role: Role::Shoot,
        cell,
        size: 1,
        support: Some(a.plant_support()),
        health: 100,
        shortage_ticks: 0,
        shade_ticks: 0,
        stores: [0; 3],
        allocation: 1000,
    }
}
fn solid(w: &mut PhysicsWorld, position: Vec2, sensor: bool) {
    let id = PhysicsId::new(2);
    let mut c = ColliderSpec::cuboid(ColliderId::new(id, ColliderRole::PRIMARY, 0), 0.5, 0.5);
    c.sensor = sensor;
    assert!(w.insert_body(
        BodyId::new(id, BodyRole::PRIMARY),
        BodySpec {
            position,
            kind: BodyKind::Fixed,
            ..Default::default()
        },
        &[c]
    ));
    w.step(1.0 / 60.0);
}
#[test]
fn plant_light_contract_solids_sensors_removal_and_same_organism_shade() {
    let (mut w, t, g, a) = fixture();
    let tissue = shoot(&a, 1, [5, 3]);
    let center = t.cell_center(engine_terrain::CellCoord::new(5, 3));
    let mut publisher = LightingPublisher::default();
    solid(&mut w, center + Vec2::Y * 2.0, true);
    {
        let r = roots(&w, &t, &g, &a);
        let light = PlantLight::publish(
            &mut publisher,
            0,
            0,
            full_sun(Vec2::Y),
            &r,
            std::slice::from_ref(&tissue),
            u64::MAX,
        )
        .unwrap();
        assert_eq!(light.diagnostic(&tissue, 0).unwrap().intensity, 1000.0);
    }
    w.remove_entity(PhysicsId::new(2));
    solid(&mut w, center + Vec2::Y * 2.0, false);
    {
        let r = roots(&w, &t, &g, &a);
        let light = PlantLight::publish(
            &mut publisher,
            1,
            1,
            full_sun(Vec2::Y),
            &r,
            std::slice::from_ref(&tissue),
            u64::MAX,
        )
        .unwrap();
        assert_eq!(light.diagnostic(&tissue, 1).unwrap().intensity, 0.0);
        assert!(
            light
                .diagnostic(&tissue, 1)
                .unwrap()
                .first_blocker
                .is_some()
        );
    }
    w.remove_entity(PhysicsId::new(2));
    w.step(1.0 / 60.0);
    let r = roots(&w, &t, &g, &a);
    let light = PlantLight::publish(
        &mut publisher,
        2,
        2,
        full_sun(Vec2::Y),
        &r,
        std::slice::from_ref(&tissue),
        u64::MAX,
    )
    .unwrap();
    assert_eq!(light.diagnostic(&tissue, 2).unwrap().intensity, 1000.0);
    let other = shoot(&a, 2, [5, 4]);
    let light = PlantLight::publish(
        &mut publisher,
        3,
        3,
        full_sun(Vec2::Y),
        &r,
        &[tissue.clone(), other],
        u64::MAX,
    )
    .unwrap();
    assert_eq!(
        light.diagnostic(&tissue, 3).unwrap().first_blocker,
        Some(LightId(u64::MAX, 2))
    );
    let overlap = shoot(&a, 3, tissue.cell);
    let light = PlantLight::publish(
        &mut publisher,
        4,
        4,
        full_sun(Vec2::Y),
        &r,
        &[tissue.clone(), overlap],
        u64::MAX,
    )
    .unwrap();
    assert_eq!(light.diagnostic(&tissue, 4).unwrap().intensity, 0.0);
    // Ordinary canonical terrain blocks a ray aimed back into the substrate.
    let light = PlantLight::publish(
        &mut publisher,
        5,
        5,
        full_sun(-Vec2::X),
        &r,
        std::slice::from_ref(&tissue),
        u64::MAX,
    )
    .unwrap();
    assert_eq!(light.diagnostic(&tissue, 5).unwrap().intensity, 0.0);
}
#[test]
fn plant_light_contract_presets_actual_intake_and_root_composition() {
    let (w, t, g, a) = fixture();
    let r = roots(&w, &t, &g, &a);
    let mut intakes = Vec::new();
    for source in [dim_room(Vec2::Y), full_sun(Vec2::Y)] {
        let mut plants = Plants::new(Config::default(), 1).unwrap();
        plants
            .seed(
                1,
                Role::Shoot,
                1,
                [5, 3],
                Some(a.plant_support()),
                [0; 3],
                &r,
            )
            .unwrap();
        let tissues = plants.observe().tissues;
        let mut publisher = LightingPublisher::default();
        let light =
            PlantLight::publish(&mut publisher, 0, 0, source, &r, &tissues, u64::MAX).unwrap();
        let env = light.environment(0).unwrap();
        let mut root = tissues[0].clone();
        root.role = Role::Root;
        root.cell = [0, 3];
        assert_eq!(env.exposure(&root), r.exposure(&root));
        plants.tick(0, false, 1, &[], &env).unwrap();
        intakes.push(plants.observe().ledger.intake[2]);
    }
    assert!(intakes[1] > intakes[0], "{intakes:?}");
}
#[test]
fn plant_light_contract_caps_freshness_invalid_and_replay() {
    let (w, t, g, a) = fixture();
    let r = roots(&w, &t, &g, &a);
    let tissues: Vec<_> = (0..MAX_TISSUES)
        .map(|id| shoot(&a, id as u64, [5, 3]))
        .collect();
    let mut publisher = LightingPublisher::default();
    let light = PlantLight::publish(
        &mut publisher,
        0,
        0,
        full_sun(Vec2::Y),
        &r,
        &tissues,
        u64::MAX,
    )
    .unwrap();
    assert_eq!(light.work.rays, MAX_TISSUES as u64);
    assert_eq!(
        light.work.footprint_tests,
        (MAX_TISSUES * MAX_TISSUES) as u64
    );
    assert!(matches!(light.environment(1), Err(LightingError::Stale)));
    let mut changed = tissues[0].clone();
    changed.size = 2;
    assert_eq!(
        light.diagnostic(&changed, 0),
        Err(LightingError::GeometryGeneration)
    );
    let mut overflow = tissues.clone();
    overflow.push(shoot(&a, 999, [5, 3]));
    assert!(matches!(
        PlantLight::publish(
            &mut publisher,
            1,
            1,
            full_sun(Vec2::Y),
            &r,
            &overflow,
            u64::MAX
        ),
        Err(LightingError::Capacity)
    ));
    assert!(matches!(
        PlantLight::publish(
            &mut publisher,
            1,
            1,
            full_sun(Vec2::ZERO),
            &r,
            &tissues,
            u64::MAX
        ),
        Err(LightingError::InvalidInput)
    ));
    assert!(matches!(
        PlantLight::publish(
            &mut publisher,
            1,
            1,
            full_sun(Vec2::Y),
            &r,
            &[tissues[0].clone(), tissues[0].clone()],
            u64::MAX
        ),
        Err(LightingError::ConflictingIdentity)
    ));
    let mut replay = LightingPublisher::default();
    let repeated =
        PlantLight::publish(&mut replay, 0, 0, full_sun(Vec2::Y), &r, &tissues, u64::MAX).unwrap();
    assert_eq!(light.work, repeated.work);
    for tissue in &tissues {
        assert_eq!(light.diagnostic(tissue, 0), repeated.diagnostic(tissue, 0));
    }
}
#[test]
fn plant_light_contract_support_and_source_frame_transform() {
    let (mut w, t, g, a) = fixture();
    let tissue = shoot(&a, 1, [5, 3]);
    let mut publisher = LightingPublisher::default();
    for (tick, angle) in [0.0, 0.5, 1.5].into_iter().enumerate() {
        w.set_pose(a.body(), Vec2::new(12.0, -4.0), angle, true);
        w.step(1.0 / 60.0);
        let r = roots(&w, &t, &g, &a);
        let light = PlantLight::publish(
            &mut publisher,
            tick as u64,
            tick as u64,
            full_sun(Vec2::Y.rotate_radians(angle)),
            &r,
            std::slice::from_ref(&tissue),
            u64::MAX,
        )
        .unwrap();
        assert_eq!(
            light.diagnostic(&tissue, tick as u64).unwrap().intensity,
            1000.0
        );
    }
}

#[test]
fn plant_light_contract_moving_blocker_and_embedded_receiver() {
    let (mut w, t, g, a) = fixture();
    let tissue = shoot(&a, 1, [5, 3]);
    let center = t.cell_center(CellCoord::new(5, 3));
    solid(&mut w, center + Vec2::Y * 2.0, false);
    let mut publisher = LightingPublisher::default();
    for (tick, position, expected) in [
        (0, center + Vec2::Y * 2.0, 0.0),
        (1, center + Vec2::X * 2.0, 1000.0),
        (2, center, 0.0),
    ] {
        w.set_pose(
            BodyId::new(PhysicsId::new(2), BodyRole::PRIMARY),
            position,
            0.0,
            true,
        );
        w.step(1.0 / 60.0);
        let r = roots(&w, &t, &g, &a);
        let light = PlantLight::publish(
            &mut publisher,
            tick,
            tick,
            full_sun(Vec2::Y),
            &r,
            std::slice::from_ref(&tissue),
            u64::MAX,
        )
        .unwrap();
        assert_eq!(light.diagnostic(&tissue, tick).unwrap().intensity, expected);
        assert!(matches!(
            PlantLight::publish(
                &mut publisher,
                tick,
                tick,
                full_sun(Vec2::Y),
                &r,
                std::slice::from_ref(&tissue),
                u64::MAX
            ),
            Err(LightingError::Stale)
        ));
    }
}

#[test]
fn plant_light_contract_two_support_tissue_shade_motion_removal_and_replay() {
    fn replay() -> Vec<(lighting::Exposure, VisibilityCounters)> {
        let (mut w, t, g, a) = fixture();
        let b = TerrainAssembly::insert(
            &mut w,
            PhysicsId::new(3),
            BodySpec::default(),
            &t,
            &g,
            TerrainSpec::default(),
        )
        .unwrap();
        let receiver = shoot(&a, 1, [5, 3]);
        let blocker = shoot(&b, 1, [5, 3]);
        assert_ne!(receiver.support, blocker.support);
        let local = t.cell_center(CellCoord::new(5, 3));
        let mut publisher = LightingPublisher::default();
        let mut results = Vec::new();
        for (tick, offset, angle, removed, expected) in [
            (0, Vec2::Y * 2.0, 0.0, false, 0.0),
            (1, Vec2::new(3.0, 2.0), 0.4, false, 1000.0),
            (2, Vec2::Y * 2.0, 0.2, false, 0.0),
            (3, Vec2::Y * 2.0, 0.2, true, 1000.0),
        ] {
            let target = local + offset;
            w.set_pose(b.body(), target - local.rotate_radians(angle), angle, true);
            w.step(1.0 / 60.0);
            let motion = w.motion(b.body()).unwrap();
            let footprint = OpaqueFootprint {
                id: LightId(u64::MAX - 1, blocker.id),
                center: motion.position + local.rotate_radians(motion.angle),
                radius: t.cell_size() * 0.25 * f32::from(blocker.size).sqrt(),
            };
            let disks = if removed {
                &[][..]
            } else {
                std::slice::from_ref(&footprint)
            };
            let r = roots(&w, &t, &g, &a);
            // Canonical support colliders alone never shadow this receiver.
            let clear = PlantLight::publish(
                &mut LightingPublisher::default(),
                tick,
                tick,
                full_sun(Vec2::Y),
                &r,
                std::slice::from_ref(&receiver),
                u64::MAX,
            )
            .unwrap();
            assert_eq!(clear.diagnostic(&receiver, tick).unwrap().intensity, 1000.0);
            let before = w.snapshot_bytes().unwrap();
            let light = PlantLight::publish_with_occluders(
                &mut publisher,
                tick,
                tick,
                full_sun(Vec2::Y),
                &r,
                std::slice::from_ref(&receiver),
                u64::MAX,
                WorldOccluders {
                    tick,
                    geometry_generation: tick,
                    footprints: disks,
                },
            )
            .unwrap();
            let exposure = light.diagnostic(&receiver, tick).unwrap();
            assert_eq!(exposure.intensity, expected);
            assert_eq!(
                exposure.first_blocker,
                (expected == 0.0).then_some(footprint.id)
            );
            assert_eq!(
                light.environment(tick).unwrap().exposure(&receiver).light,
                expected as u16
            );
            assert_eq!(w.snapshot_bytes().unwrap(), before);
            results.push((exposure.clone(), light.work));
        }
        results
    }
    assert_eq!(replay(), replay());
}

#[test]
fn plant_light_contract_external_aggregate_caps_identity_and_freshness() {
    let (w, t, g, a) = fixture();
    let r = roots(&w, &t, &g, &a);
    // Cross the 64-receiver boundary while sharing the total geometry cap.
    let tissues: Vec<_> = (0..65).map(|id| shoot(&a, id, [5, 3])).collect();
    let mut disks: Vec<_> = (0..MAX_TISSUES - tissues.len())
        .map(|id| OpaqueFootprint {
            id: LightId(u64::MAX - 1, id as u64),
            center: Vec2::new(20.0, 20.0),
            radius: 0.25,
        })
        .collect();
    let mut publisher = LightingPublisher::default();
    let publish = |publisher: &mut LightingPublisher, tick, revision, disks: &[OpaqueFootprint]| {
        PlantLight::publish_with_occluders(
            publisher,
            0,
            0,
            full_sun(Vec2::Y),
            &r,
            &tissues,
            u64::MAX,
            WorldOccluders {
                tick,
                geometry_generation: revision,
                footprints: disks,
            },
        )
    };
    let light = publish(&mut publisher, 0, 0, &disks).unwrap();
    assert_eq!(light.work.rays, 65);
    assert_eq!(light.work.footprint_tests, 65 * MAX_TISSUES as u64);
    let mut fresh = LightingPublisher::default();
    disks.push(OpaqueFootprint {
        id: LightId(99, 1),
        center: Vec2::ZERO,
        radius: 1.0,
    });
    assert!(matches!(
        publish(&mut fresh, 0, 0, &disks),
        Err(LightingError::Capacity)
    ));
    disks.pop();
    assert!(matches!(
        publish(&mut fresh, 1, 0, &disks),
        Err(LightingError::Stale)
    ));
    assert!(matches!(
        publish(&mut fresh, 0, 1, &disks),
        Err(LightingError::GeometryGeneration)
    ));
    disks[0].id = LightId(u64::MAX, tissues[0].id);
    assert!(matches!(
        publish(&mut fresh, 0, 0, &disks),
        Err(LightingError::ConflictingIdentity)
    ));
    disks[0].id = disks[1].id;
    assert!(matches!(
        publish(&mut fresh, 0, 0, &disks),
        Err(LightingError::ConflictingIdentity)
    ));
    disks[0].id = LightId(u64::MAX - 1, 0);
    disks[0].center.x = f32::NAN;
    assert!(matches!(
        publish(&mut fresh, 0, 0, &disks),
        Err(LightingError::InvalidInput)
    ));
    assert!(fresh.snapshot().is_none());
}

#[test]
fn plant_light_contract_subsecond_intensity_shade_and_per_tick_energy() {
    let mut duration_results = Vec::new();
    for tick_ms in [100, 250, 1000] {
        let mut results = Vec::new();
        for (source, blocked, intensity, shaded) in [
            (full_sun(Vec2::Y), false, 1000, false),
            (dim_room(Vec2::Y), false, 50, true),
            (full_sun(Vec2::Y), true, 0, true),
        ] {
            let (mut w, t, g, a) = fixture();
            if blocked {
                let center = t.cell_center(CellCoord::new(5, 3));
                solid(&mut w, center + Vec2::Y * 2.0, false);
            }
            let mut r = roots(&w, &t, &g, &a);
            r.tick_ms = tick_ms;
            let config = Config::default();
            assert_eq!(config.local_light_threshold, 250);
            let expected_energy = config.shoot_energy_per_size * u64::from(intensity) / 1000;
            let mut plants = Plants::new(config.clone(), 1).unwrap();
            for (role, cell) in [(Role::Shoot, [5, 3]), (Role::Root, [0, 3])] {
                plants
                    .seed(1, role, 1, cell, Some(a.plant_support()), [10; 3], &r)
                    .unwrap();
            }
            let mut publisher = LightingPublisher::default();
            for tick in 0..4 {
                let before = plants.observe();
                let root = before
                    .tissues
                    .iter()
                    .find(|t| t.role == Role::Root)
                    .unwrap();
                let root_supply = r.exposure(root);
                let expected_root = [
                    config.root_intake_per_size[0] * u64::from(root_supply.nutrients) / 1000,
                    config.root_intake_per_size[1] * u64::from(root_supply.water) / 1000,
                    0,
                ];
                let light = PlantLight::publish(
                    &mut publisher,
                    tick,
                    tick,
                    source,
                    &r,
                    &before.tissues,
                    u64::MAX,
                )
                .unwrap();
                let env = light.environment(tick).unwrap();
                let shoot = before
                    .tissues
                    .iter()
                    .find(|t| t.role == Role::Shoot)
                    .unwrap();
                assert_eq!(env.exposure(shoot).light, intensity, "tick_ms={tick_ms}");
                assert_eq!(env.exposure(root), root_supply);
                plants.tick(tick, false, 1, &[], &env).unwrap();
                let after = plants.observe();
                assert_eq!(after.ledger.roots[&root.id].offered, expected_root);
                assert_eq!(
                    after.ledger.intake,
                    [expected_root[0], expected_root[1], expected_energy]
                );
                let shoot = after
                    .tissues
                    .iter()
                    .find(|t| t.role == Role::Shoot)
                    .unwrap();
                assert_eq!(shoot.shade_ticks, if shaded { tick as u32 + 1 } else { 0 });
                assert_eq!(shoot.shortage_ticks, 0);
                // Four shaded ticks exceed the default grace; clear full sun
                // remains healthy even at 100 ms, with no threshold retuning.
                assert_eq!(shoot.health, if shaded && tick == 3 { 80 } else { 100 });
                results.push((intensity, shoot.shade_ticks, shoot.health, expected_energy));
            }
        }
        duration_results.push(results);
    }
    assert_eq!(duration_results[0], duration_results[1]);
    assert_eq!(duration_results[1], duration_results[2]);
}
