use super::*;
use crate::{
    terrain::TerrainSpec,
    world::{BodySpec, PhysicsId, PhysicsWorldConfig},
};
use engine_core::{
    Vec2,
    plants::{Config, Plants, Rejection},
};
use engine_terrain::{Brush, EditMode, Material, TerrainEdit};

fn fixture() -> (PhysicsWorld, Terrain, TerrainGeometry, TerrainAssembly) {
    let mut w = PhysicsWorld::new(PhysicsWorldConfig::default());
    let t = Terrain::generate(
        8,
        8,
        1.0,
        vec![
            Material {
                id: MaterialId(1),
                hardness: 10,
            },
            Material {
                id: MaterialId(2),
                hardness: 10,
            },
        ],
        |c| {
            if c.x < 4 {
                MaterialId(1)
            } else if c.x == 4 {
                MaterialId(2)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let g = TerrainGeometry::new(&t);
    let a = TerrainAssembly::insert(
        &mut w,
        PhysicsId::new(19),
        BodySpec::default(),
        &t,
        &g,
        TerrainSpec::default(),
    )
    .unwrap();
    (w, t, g, a)
}
fn env<'a>(
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
fn plant(e: &RootUptake<'_>, cell: [i32; 2]) -> Plants {
    let mut p = Plants::new(
        Config {
            capacity_per_size: [[20, 20, 2000], [10; 3], [20; 3]],
            ..Config::default()
        },
        5,
    )
    .unwrap();
    p.seed(
        1,
        Role::Root,
        1,
        cell,
        Some(e.assembly.plant_support()),
        [0, 0, 2000],
        e,
    )
    .unwrap();
    p
}
#[test]
fn contacts_transforms_controls_and_duplicates() {
    let (mut w, t, g, a) = fixture();
    let mut p = plant(&env(&w, &t, &g, &a), [2, 2]);
    for (cell, reason) in [
        ([2, 2], ContactReason::Eligible),
        ([4, 2], ContactReason::Ineligible),
        ([5, 2], ContactReason::Air),
        ([8, 2], ContactReason::Air),
    ] {
        let mut tissue = p.observe().tissues[0].clone();
        tissue.cell = cell;
        assert_eq!(env(&w, &t, &g, &a).contact(&tissue), reason);
        tissue.health = 0;
        assert_eq!(env(&w, &t, &g, &a).contact(&tissue), ContactReason::Dead);
    }
    w.set_pose(a.body(), Vec2::new(19.0, -7.0), 0.73, true);
    let mut e = env(&w, &t, &g, &a);
    for _ in 0..10 {
        assert!(e.exposure(&p.observe().tissues[0]).dirt_contact);
    }
    p.tick(0, false, 1, &[], &e).unwrap();
    assert_eq!(p.observe().ledger.intake, [4, 3, 0]);
    let before = p.clone();
    assert_eq!(p.tick(0, false, 1, &[], &e), Err(Rejection::Stale));
    assert_eq!(p, before);
    p.tick(1, true, 1, &[], &e).unwrap();
    assert_eq!(p, before);
    p.tick(1, false, 0, &[], &e).unwrap();
    assert_eq!(p, before);
    for resource in 0..2 {
        e.availability = [1000; 2];
        e.availability[resource] = 0;
        p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
        assert_eq!(p.observe().ledger.intake[resource], 0);
        e.availability = [1000; 2];
        p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
        assert!(p.observe().ledger.intake[resource] > 0);
    }
    e.tick_ms = 500;
    p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
    assert_eq!(p.observe().ledger.intake, [2, 1, 0]);
}
#[test]
fn terrain_and_support_invalidate_at_boundary() {
    let (mut w, mut t, mut g, mut a) = fixture();
    let mut p = plant(&env(&w, &t, &g, &a), [2, 2]);
    t.apply(TerrainEdit {
        brush: Brush::Circle {
            center: CellCoord::new(2, 2),
            radius: 0,
        },
        mode: EditMode::Remove,
    })
    .unwrap();
    assert_eq!(
        env(&w, &t, &g, &a).contact(&p.observe().tissues[0]),
        ContactReason::StaleGeometry
    );
    g.refresh(&t);
    a.synchronize(&mut w, &t, &g).unwrap();
    let e = env(&w, &t, &g, &a);
    p.tick(0, false, 1, &[], &e).unwrap();
    assert_eq!(p.observe().ledger.intake, [0; 3]);
    w.remove_entity(a.body().entity);
    assert_eq!(
        env(&w, &t, &g, &a).contact(&p.observe().tissues[0]),
        ContactReason::Support
    );
    let replacement = TerrainAssembly::insert(
        &mut w,
        PhysicsId::new(19),
        BodySpec::default(),
        &t,
        &g,
        TerrainSpec::default(),
    )
    .unwrap();
    assert_ne!(replacement.plant_support(), a.plant_support());
    assert_eq!(
        env(&w, &t, &g, &replacement).contact(&p.observe().tissues[0]),
        ContactReason::Support
    );
}
#[test]
fn renewable_bounded_equal_time_schedules() {
    let (w, t, g, a) = fixture();
    let hash = t.hash();
    let e = env(&w, &t, &g, &a);
    let initial = plant(&e, [2, 2]);
    let run = |batch| {
        let mut p = initial.clone();
        for _ in 0..1000 / batch {
            for _ in 0..batch {
                let before = p.observe();
                p.tick(before.tick, false, 1, &[], &e).unwrap();
                let after = p.observe();
                assert!(
                    after.tissues[0]
                        .stores
                        .iter()
                        .zip([20, 20, 2000])
                        .all(|(x, cap)| *x <= cap)
                );
                for i in 0..3 {
                    assert_eq!(
                        before.tissues[0].stores[i] + after.ledger.intake[i],
                        after.tissues[0].stores[i]
                            + after.ledger.maintenance[i]
                            + after.ledger.loss[i]
                    );
                }
                let r = &after.ledger.roots[&1];
                for i in 0..3 {
                    assert_eq!(r.offered[i], r.accepted[i] + r.overflow[i]);
                }
            }
        }
        p
    };
    assert_eq!(run(1), run(10));
    assert_eq!(t.hash(), hash);
}

#[test]
fn independent_shortage_damage_recovery_and_death() {
    let (w, t, g, a) = fixture();
    for resource in 0..2 {
        let mut e = env(&w, &t, &g, &a);
        let mut p = plant(&e, [2, 2]);
        e.availability[resource] = 0;
        for _ in 0..6 {
            p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
        }
        let damaged = p.observe().tissues[0].health;
        assert!(damaged > 0 && damaged < 100);
        assert!(p.observe().ledger.unmet[resource] > 0);
        assert_eq!(p.observe().ledger.unmet[1 - resource], 0);
        e.availability[resource] = 1000;
        for _ in 0..4 {
            p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
        }
        assert!(p.observe().tissues[0].health > damaged);
        e.availability[resource] = 0;
        for _ in 0..40 {
            p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
        }
        assert_eq!(p.observe().tissues[0].health, 0);
        e.availability[resource] = 1000;
        p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
        assert_eq!(p.observe().ledger.intake, [0; 3]);
        assert_eq!(e.contact(&p.observe().tissues[0]), ContactReason::Dead);
        p.remove(1).unwrap();
        p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
        assert!(p.observe().ledger.roots.is_empty());
    }
}

#[test]
fn growth_first_samples_next_tick_and_invalid_controls_are_atomic() {
    use engine_core::plants::Intent;
    let (w, t, g, a) = fixture();
    let mut e = env(&w, &t, &g, &a);
    let mut p = plant(&e, [2, 2]);
    for _ in 0..3 {
        p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
    }
    p.tick(
        p.observe().tick,
        false,
        1,
        &[Intent::Grow {
            parent: 1,
            role: Role::Root,
            size: 1,
            cell: [3, 2],
            support: Some(a.plant_support()),
        }],
        &e,
    )
    .unwrap();
    assert_eq!(p.observe().ledger.results, vec![Ok(())]);
    assert_eq!(p.observe().ledger.roots.len(), 1);
    p.tick(p.observe().tick, false, 1, &[], &e).unwrap();
    assert_eq!(p.observe().ledger.intake, [8, 6, 0]);
    assert_eq!(p.observe().ledger.roots.len(), 2);
    e.rates[0] = 1001;
    let before = p.clone();
    assert_eq!(
        p.tick(p.observe().tick, false, 1, &[], &e),
        Err(Rejection::Invalid)
    );
    assert_eq!(p, before);
}
