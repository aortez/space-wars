use super::*;
use crate::{
    plant_light::{PlantLight, dim_room, full_sun},
    terrain::{TerrainAssembly, TerrainSpec},
    world::*,
};
use engine_core::{
    lighting::LightingPublisher,
    plants::{
        Config, Observation,
        grass::{Grass, Traits},
    },
};
use engine_terrain::{Material, Terrain, TerrainGeometry};

fn run(availability: [u16; 2], dim: bool, angle: f32) -> Vec<Observation> {
    run_case(availability, dim, angle, None, false)
}
fn run_case(
    availability: [u16; 2],
    dim: bool,
    angle: f32,
    roof_until: Option<u64>,
    cut: bool,
) -> Vec<Observation> {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let terrain = Terrain::generate(
        16,
        8,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 10,
        }],
        |c| {
            if c.y <= 3 {
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
        BodySpec {
            kind: BodyKind::Fixed,
            angle,
            position: if angle == 0.0 {
                Vec2::ZERO
            } else {
                Vec2::new(17.0, -3.0)
            },
            ..Default::default()
        },
        &terrain,
        &geometry,
        TerrainSpec::default(),
    )
    .unwrap();
    if roof_until.is_some() {
        let entity = PhysicsId::new(2);
        let collider =
            ColliderSpec::cuboid(ColliderId::new(entity, ColliderRole::PRIMARY, 0), 2.8, 0.25);
        assert!(world.insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position: terrain.cell_center(CellCoord::new(11, 6)),
                ..Default::default()
            },
            &[collider]
        ));
    }
    world.step(1.0 / 60.0);
    let roots = RootUptake {
        world: &world,
        assembly: &assembly,
        terrain: &terrain,
        geometry: &geometry,
        dirt: MaterialId(1),
        rates: [1000; 2],
        availability,
        tick_ms: 1000,
    };
    let gravity = Vec2::new(0.0, -1.0).rotate_radians(angle);
    let up = local_up(&roots, gravity).unwrap();
    assert_eq!(up, [0, 1]);
    let grass = Grass::new(Traits::default(), [8, 3], up, assembly.plant_support()).unwrap();
    let mut plants = Plants::new(Config::default(), 42).unwrap();
    let env = GrassEnvironment {
        roots: &roots,
        physiology: &roots,
    };
    grass.plant(&mut plants, &env).unwrap();
    assert!(!env.space_available([8, 4], Role::Runner, 1));
    assert!(!env.space_available([8, 3], Role::Shoot, 1));
    let mut publisher = LightingPublisher::default();
    let mut history = vec![];
    for tick in 0..40 {
        if roof_until == Some(tick) {
            world.remove_entity(PhysicsId::new(2));
            world.step(1.0 / 60.0);
        }
        if cut && tick == 10 {
            let shoots: Vec<_> = plants
                .observe()
                .tissues
                .iter()
                .filter(|t| t.role == Role::Shoot)
                .map(|t| t.id)
                .collect();
            for id in shoots {
                plants.damage(id, 100).unwrap();
                plants.remove(id).unwrap();
            }
        }
        let roots = RootUptake {
            world: &world,
            assembly: &assembly,
            terrain: &terrain,
            geometry: &geometry,
            dirt: MaterialId(1),
            rates: [1000; 2],
            availability,
            tick_ms: 1000,
        };
        let o = plants.observe();
        let direction = Vec2::Y.rotate_radians(angle);
        let light = PlantLight::publish(
            &mut publisher,
            o.tick,
            o.tick,
            if dim {
                dim_room(direction)
            } else {
                full_sun(direction)
            },
            &roots,
            &o.tissues,
            u64::MAX,
        )
        .unwrap();
        assert!(light.work.rays <= 256);
        assert!(light.work.footprint_tests <= 65536);
        assert!(light.work.collider_records <= 524288);
        let env = GrassEnvironment {
            roots: &roots,
            physiology: light.environment(o.tick).unwrap(),
        };
        let plan = grass.plan(&o, &env);
        plants.tick(o.tick, false, 1, &plan.intents, &env).unwrap();
        let after = plants.observe();
        for i in 0..3 {
            let before: u64 = o.tissues.iter().map(|t| t.stores[i]).sum();
            let stored: u64 = after.tissues.iter().map(|t| t.stores[i]).sum();
            assert_eq!(
                before + after.ledger.intake[i],
                stored
                    + after.ledger.maintenance[i]
                    + after.ledger.growth[i]
                    + after.ledger.loss[i]
            );
        }
        history.push(after);
    }
    history
}
#[test]
fn grass_growth_contract_actual_soil_light_paid_spread_and_replay() {
    let history = run([1000; 2], false, 0.0);
    assert_eq!(history, run([1000; 2], false, 0.0));
    let o = &history[8];
    assert!(o.tissues.iter().any(|t| t.role == Role::Runner));
    assert!(o.tissues.iter().filter(|t| t.role == Role::Shoot).count() >= 2);
    assert!(history.iter().any(|o| o.ledger.growth != [0; 3]));
    assert!(history.iter().all(|o| o.tissues.len() <= 32));
    let rotated = run([1000; 2], false, 0.73);
    assert_eq!(history, rotated);
}
#[test]
fn grass_growth_contract_independent_supply_and_dim_room_limits() {
    let sun = run([1000; 2], false, 0.0);
    for history in [
        run([0, 1000], false, 0.0),
        run([1000, 0], false, 0.0),
        run([1000; 2], true, 0.0),
    ] {
        assert!(history.iter().any(|o| o.ledger.unmet != [0; 3]));
        let growth = |h: &Vec<Observation>| {
            h.iter()
                .map(|o| o.ledger.growth.iter().sum::<u64>())
                .sum::<u64>()
        };
        assert!(growth(&history) < growth(&sun));
    }
}

#[test]
fn grass_growth_contract_shaded_expansion_recovery_and_local_death() {
    let dark = run_case([1000; 2], false, 0.0, Some(100), false);
    let recovering = run_case([1000; 2], false, 0.0, Some(10), false);
    let shaded = dark
        .iter()
        .flat_map(|o| &o.tissues)
        .find(|t| t.role == Role::Shoot && t.shade_ticks > 3 && t.health > 0)
        .unwrap();
    let id = shaded.id;
    let boundary = dark
        .iter()
        .find(|o| {
            o.tissues
                .iter()
                .any(|t| t.id == id && t.shade_ticks > 3 && t.health > 0)
        })
        .unwrap();
    let mut connected = std::collections::BTreeSet::from([id]);
    loop {
        let before = connected.len();
        for (a, b) in &boundary.connections {
            if boundary.tissues.iter().any(|t| t.id == *a && t.health == 0)
                || boundary.tissues.iter().any(|t| t.id == *b && t.health == 0)
            {
                continue;
            }
            if connected.contains(a) || connected.contains(b) {
                connected.insert(*a);
                connected.insert(*b);
            }
        }
        if connected.len() == before {
            break;
        }
    }
    assert!(boundary.tissues.iter().any(|t| connected.contains(&t.id)
        && t.role == Role::Shoot
        && t.health == 100
        && t.shade_ticks == 0));
    assert!(recovering[10..].iter().any(|o| {
        o.tissues
            .iter()
            .any(|t| t.id == id && t.health == 100 && t.shade_ticks == 0)
    }));

    assert!(
        dark.iter()
            .any(|o| o.tissues.iter().any(|t| t.id == id && t.health == 0))
    );
    assert!(
        recovering
            .last()
            .unwrap()
            .tissues
            .iter()
            .any(|t| t.id == id && t.health > 0 && t.shade_ticks == 0)
    );
    assert!(dark.iter().any(|o| {
        o.tissues
            .iter()
            .any(|t| t.id == id && t.shade_ticks > 3 && t.shortage_ticks == 0)
            && o.tissues
                .iter()
                .any(|t| t.role == Role::Shoot && t.shade_ticks == 0 && t.health == 100)
    }));
    assert!(
        dark.iter()
            .flat_map(|o| &o.tissues)
            .filter(|t| t.role != Role::Shoot)
            .all(|t| t.shade_ticks == 0)
    );
}
#[test]
fn grass_growth_contract_cutting_pays_for_regrowth() {
    let history = run_case([1000; 2], false, 0.0, None, true);
    let old_ids: Vec<_> = history[9]
        .tissues
        .iter()
        .filter(|t| t.role == Role::Shoot)
        .map(|t| t.id)
        .collect();
    assert!(history[11..].iter().any(|o| {
        o.ledger.growth != [0; 3]
            && o.tissues
                .iter()
                .any(|t| t.role == Role::Shoot && !old_ids.contains(&t.id))
    }));
}

#[test]
fn grass_growth_contract_support_removal_discards_reserves_without_replanting() {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let terrain = Terrain::generate(
        4,
        4,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 10,
        }],
        |_| MaterialId(1),
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
    let grass = Grass::new(Traits::default(), [1, 1], [0, 1], assembly.plant_support()).unwrap();
    let mut plants = Plants::new(Config::default(), 42).unwrap();
    {
        let roots = RootUptake {
            world: &world,
            assembly: &assembly,
            terrain: &terrain,
            geometry: &geometry,
            dirt: MaterialId(1),
            rates: [1000; 2],
            availability: [1000; 2],
            tick_ms: 1000,
        };
        grass
            .plant(
                &mut plants,
                &GrassEnvironment {
                    roots: &roots,
                    physiology: &roots,
                },
            )
            .unwrap();
    }
    world.remove_entity(PhysicsId::new(1));
    let roots = RootUptake {
        world: &world,
        assembly: &assembly,
        terrain: &terrain,
        geometry: &geometry,
        dirt: MaterialId(1),
        rates: [1000; 2],
        availability: [1000; 2],
        tick_ms: 1000,
    };
    let env = GrassEnvironment {
        roots: &roots,
        physiology: &roots,
    };
    assert_eq!(detach_invalid(&mut plants, &env), Ok(1));
    assert_eq!(plants.observe().ledger.loss, [30; 3]);
    assert!(grass.plan(&plants.observe(), &env).intents.is_empty());
    assert!(plants.observe().tissues.is_empty());
}
