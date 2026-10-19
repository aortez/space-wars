use super::*;

#[derive(Clone)]
struct Env {
    nutrients: u16,
    water: u16,
    light: u16,
    shade: Option<[i32; 2]>,
    space: bool,
    support: bool,
}
impl Default for Env {
    fn default() -> Self {
        Self {
            nutrients: 1000,
            water: 1000,
            light: 1000,
            shade: None,
            space: true,
            support: true,
        }
    }
}
impl Environment for Env {
    fn exposure(&self, t: &Tissue) -> Exposure {
        Exposure {
            dirt_contact: true,
            surface_contact: true,
            nutrients: self.nutrients,
            water: self.water,
            light: if self.shade == Some(t.cell) {
                0
            } else {
                self.light
            },
        }
    }
    fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
        self.space
    }
    fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
        self.support
    }
}
fn plant() -> Plants {
    Plants::new(Config::default(), 123).unwrap()
}
fn seed(p: &mut Plants, role: Role, cell: [i32; 2], stores: Resources) -> u64 {
    p.seed(42, role, 1, cell, None, stores, &Env::default())
        .unwrap()
}
fn step(p: &mut Plants, env: &Env, intents: &[Intent]) {
    p.tick(p.tick, false, 1, intents, env).unwrap();
}
fn grow(parent: u64, role: Role, cell: [i32; 2]) -> Intent {
    Intent::Grow {
        parent,
        role,
        size: 1,
        cell,
        support: None,
    }
}
fn sum(p: &Plants) -> Resources {
    let mut total = [0; 3];
    for t in p.tissues.values() {
        add(&mut total, t.stores);
    }
    total
}
fn pair() -> Plants {
    let mut p = plant();
    let id = seed(&mut p, Role::Root, [0, 0], [20; 3]);
    step(&mut p, &Env::default(), &[grow(id, Role::Shoot, [1, 0])]);
    assert_eq!(p.ledger.results, vec![Ok(())]);
    p
}

#[test]
fn maintenance_without_growth_and_sustainable_paid_growth() {
    let mut p = pair();
    let env = Env::default();
    for _ in 0..100 {
        step(&mut p, &env, &[]);
        assert!(p.tissues.values().all(|t| t.health == 100));
        assert_eq!(p.ledger.maintenance, [2, 3, 3]);
        assert_eq!(p.ledger.unmet, [0; 3]);
    }
    let before = sum(&p);
    step(&mut p, &env, &[grow(1, Role::Runner, [0, 1])]);
    assert_eq!(p.ledger.growth, [2; 3]);
    assert_eq!(p.ledger.maintenance, [2, 3, 3]); // new runner first pays next tick
    for i in 0..3 {
        assert_eq!(
            sum(&p)[i] + p.ledger.maintenance[i] + p.ledger.growth[i] + p.ledger.loss[i],
            before[i] + p.ledger.intake[i]
        );
    }
    step(&mut p, &env, &[]);
    assert_eq!(p.ledger.maintenance, [3, 4, 4]);
    assert_eq!(p.observe().seed, 123);
    assert_eq!(p.observe().config.identity, 1);
}

#[test]
fn each_resource_independently_limits_and_recovers_before_death() {
    for resource in 0..3 {
        let mut p = pair();
        let mut env = Env::default();
        // Energy shortage without local shade: lower photosynthesis yield.
        match resource {
            0 => env.nutrients = 0,
            1 => env.water = 0,
            _ => p.config.shoot_energy_per_size = 0,
        }
        let mut damaged = false;
        for _ in 0..80 {
            step(&mut p, &env, &[]);
            if p.tissues.values().any(|t| t.health < 100) {
                damaged = true;
                break;
            }
        }
        assert!(damaged);
        assert!(p.ledger.unmet[resource] > 0);
        for i in 0..3 {
            if i != resource {
                assert_eq!(p.ledger.unmet[i], 0);
            }
        }
        assert!(p.tissues.values().all(|t| t.shade_ticks == 0));
        p.config.shoot_energy_per_size = 8;
        for _ in 0..12 {
            step(&mut p, &Env::default(), &[]);
        }
        assert!(p.tissues.values().all(|t| t.health == 100));
    }
}

#[test]
fn underground_roots_survive_but_attached_local_shade_dies_and_can_recover() {
    let mut p = pair();
    step(&mut p, &Env::default(), &[grow(2, Role::Shoot, [2, 0])]);
    let shade = Env {
        shade: Some([2, 0]),
        ..Env::default()
    };
    for _ in 0..4 {
        step(&mut p, &shade, &[]);
    }
    assert_eq!(p.tissues[&3].health, 80);
    assert_eq!(p.tissues[&1].health, 100);
    step(&mut p, &Env::default(), &[]);
    assert_eq!(p.tissues[&3].health, 90);
    for _ in 0..9 {
        step(&mut p, &shade, &[]);
    }
    assert_eq!(p.tissues[&3].health, 0);
    assert_eq!(p.tissues[&2].health, 100);
    // Roots have no local light requirement, despite their own zero exposure.
    let roots_dark = Env {
        shade: Some([0, 0]),
        ..Env::default()
    };
    for _ in 0..30 {
        step(&mut p, &roots_dark, &[]);
    }
    assert_eq!(p.tissues[&1].health, 100);
    assert_eq!(p.tissues[&3].health, 0);
    assert_eq!(p.tissues[&3].stores, [0; 3]);
}

#[test]
fn shade_extension_is_allowed_and_whole_darkness_has_bounded_survival() {
    let mut p = pair();
    let env = Env {
        light: 0,
        ..Env::default()
    };
    step(&mut p, &env, &[grow(2, Role::Shoot, [2, 0])]);
    assert_eq!(p.ledger.results, vec![Ok(())]);
    for _ in 0..100 {
        step(&mut p, &env, &[]);
    }
    assert!(p.tissues.values().all(|t| t.health == 0));
    step(&mut p, &Env::default(), &[grow(1, Role::Root, [0, 1])]);
    assert_eq!(p.ledger.intake, [0; 3]);
    assert_eq!(p.ledger.results, vec![Err(Rejection::Dead)]);
    assert!(p.tissues.values().all(|t| t.health == 0));
    assert_eq!(sum(&p), [0; 3]);
}

#[test]
fn rejection_is_atomic_for_space_support_cost_size_distance_and_dead_parent() {
    let cases = [
        (
            grow(1, Role::Root, [0, 1]),
            Env {
                space: false,
                ..Env::default()
            },
            Rejection::Space,
        ),
        (
            grow(1, Role::Root, [0, 1]),
            Env {
                support: false,
                ..Env::default()
            },
            Rejection::Support,
        ),
        (
            Intent::Grow {
                parent: 1,
                role: Role::Root,
                size: 0,
                cell: [0, 1],
                support: None,
            },
            Env::default(),
            Rejection::Invalid,
        ),
        (
            grow(1, Role::Root, [0, 2]),
            Env::default(),
            Rejection::Invalid,
        ),
        (
            grow(1, Role::Root, [0, 0]),
            Env::default(),
            Rejection::Invalid,
        ),
    ];
    for (intent, env, reason) in cases {
        let mut p = pair();
        let before = p.clone();
        assert_eq!(p.apply(&intent, &env), Err(reason));
        assert_eq!(p, before);
    }
    let mut p = plant();
    seed(&mut p, Role::Root, [0, 0], [0; 3]);
    let before = p.clone();
    assert_eq!(
        p.apply(&grow(1, Role::Root, [0, 1]), &Env::default()),
        Err(Rejection::Unaffordable)
    );
    assert_eq!(p, before);
    p.tissues.get_mut(&1).unwrap().health = 0;
    let before = p.clone();
    assert_eq!(
        p.apply(&grow(1, Role::Root, [0, 1]), &Env::default()),
        Err(Rejection::Dead)
    );
    assert_eq!(p, before);
}

#[test]
fn severing_partitions_by_capacity_and_disconnects_intake_without_identity_change() {
    let mut p = pair();
    p.normalize();
    let before = p.clone();
    let stores = sum(&p);
    p.sever(1, 2).unwrap();
    assert_eq!(sum(&p), stores);
    for id in [1, 2] {
        assert_eq!(p.tissues[&id].stores, before.tissues[&id].stores);
        assert_eq!(p.tissues[&id].policy, 42);
        assert_eq!(p.tissues[&id].organism, 1);
    }
    // Equal capacities: each resource differs by at most one unit.
    for i in 0..3 {
        assert!(p.tissues[&1].stores[i].abs_diff(p.tissues[&2].stores[i]) <= 1);
    }
    for _ in 0..100 {
        step(&mut p, &Env::default(), &[]);
    }
    assert_eq!(p.tissues[&1].health, 0); // no shoot energy
    assert_eq!(p.tissues[&2].health, 0); // no root water/nutrients
}

#[test]
fn unequal_capacity_partition_removal_and_capacity_loss_are_conservative() {
    let mut p = pair();
    step(&mut p, &Env::default(), &[grow(1, Role::Runner, [0, 1])]);
    let group = vec![1, 2, 3];
    p.distribute(&group, [45; 3]);
    assert_eq!(p.tissues[&1].stores, [18; 3]);
    assert_eq!(p.tissues[&2].stores, [18; 3]);
    assert_eq!(p.tissues[&3].stores, [9; 3]);
    let before = sum(&p);
    let lost = p.tissues[&3].stores;
    let previous = p.ledger.loss;
    p.remove(3).unwrap();
    for i in 0..3 {
        assert_eq!(sum(&p)[i] + lost[i], before[i]);
        assert_eq!(p.ledger.loss[i] - previous[i], lost[i]);
    }
    let mut p = plant();
    p.seed(42, Role::Root, 2, [0, 0], None, [40; 3], &Env::default())
        .unwrap();
    p.resize_storage(1, 1).unwrap();
    assert_eq!(sum(&p), [20; 3]);
    assert_eq!(p.ledger.loss, [20; 3]);
    let before = p.clone();
    assert_eq!(p.resize_storage(1, 2), Err(Rejection::Invalid));
    assert_eq!(p, before);
}

#[test]
fn death_of_bridge_loses_its_share_and_stops_resource_transfer() {
    let mut p = pair();
    step(&mut p, &Env::default(), &[grow(2, Role::Root, [2, 0])]);
    let env = Env {
        shade: Some([1, 0]),
        ..Env::default()
    };
    for _ in 0..8 {
        let before = sum(&p);
        step(&mut p, &env, &[]);
        for i in 0..3 {
            assert_eq!(
                before[i] + p.ledger.intake[i],
                sum(&p)[i] + p.ledger.maintenance[i] + p.ledger.loss[i]
            );
        }
    }
    assert_eq!(p.tissues[&2].health, 0);
    assert_eq!(p.components(), vec![vec![1], vec![3]]);
    assert_eq!(p.tissues[&2].stores, [0; 3]);
    for _ in 0..100 {
        step(&mut p, &Env::default(), &[]);
    }
    assert!(p.tissues.values().all(|t| t.health == 0));
}

#[test]
fn work_counts_organisms_allocation_and_invalid_inputs_are_bounded() {
    let mut c = Config::default();
    c.max_tissues = 2;
    c.max_organisms = 1;
    c.max_intents = 1;
    let mut p = Plants::new(c, 0).unwrap();
    seed(&mut p, Role::Root, [0, 0], [20; 3]);
    assert_eq!(
        p.seed(42, Role::Root, 1, [5, 0], None, [0; 3], &Env::default()),
        Err(Rejection::OrganismLimit)
    );
    let before = p.clone();
    assert_eq!(
        p.tick(
            0,
            false,
            1,
            &[grow(1, Role::Root, [0, 1]), grow(1, Role::Root, [0, 1])],
            &Env::default()
        ),
        Err(Rejection::WorkLimit)
    );
    assert_eq!(p, before);
    step(&mut p, &Env::default(), &[grow(1, Role::Shoot, [1, 0])]);
    assert_eq!(
        p.apply(&grow(1, Role::Runner, [0, 1]), &Env::default()),
        Err(Rejection::TissueLimit)
    );
    p.apply(
        &Intent::Allocate {
            tissue: 2,
            priority: 10,
        },
        &Env::default(),
    )
    .unwrap();
    assert_eq!(p.observe().tissues[1].allocation, 10);
    let before = p.clone();
    assert_eq!(
        p.tick(
            p.tick,
            false,
            1,
            &[],
            &Env {
                light: 1001,
                ..Env::default()
            }
        ),
        Err(Rejection::Invalid)
    );
    assert_eq!(p, before);
    let mut c = Config::default();
    c.max_tissues = 1025;
    assert_eq!(Plants::new(c, 0), Err(Rejection::Invalid));
}

#[test]
fn paused_zero_duplicate_and_seeded_replay_ignore_render_schedule() {
    use crate::rng::{random_unit_f32, seeded_rng};
    fn replay(batch: usize) -> Vec<Observation> {
        let mut p = pair();
        let mut rng = seeded_rng(p.seed);
        let mut history = Vec::new();
        for _ in 0..(60 / batch) {
            for _ in 0..batch {
                let env = Env {
                    light: if random_unit_f32(&mut rng) < 0.5 {
                        0
                    } else {
                        1000
                    },
                    ..Env::default()
                };
                let before = p.clone();
                p.tick(p.tick, true, 1, &[grow(1, Role::Root, [0, 1])], &env)
                    .unwrap();
                p.tick(p.tick, false, 0, &[], &env).unwrap();
                assert_eq!(p, before);
                step(&mut p, &env, &[]);
                let before = p.clone();
                assert_eq!(
                    p.tick(p.tick - 1, false, 1, &[], &env),
                    Err(Rejection::Stale)
                );
                assert_eq!(p, before);
                history.push(p.observe());
            }
        }
        history
    }
    assert_eq!(replay(1), replay(10));
}

#[test]
fn tick_rejections_do_not_spend_and_conflicting_growth_commits_only_once() {
    let mut rejected = pair();
    let mut control = rejected.clone();
    step(&mut control, &Env::default(), &[]);
    step(
        &mut rejected,
        &Env::default(),
        &[grow(1, Role::Shoot, [9, 9])],
    );
    assert_eq!(rejected.ledger.results, vec![Err(Rejection::Invalid)]);
    rejected.ledger.results.clear();
    assert_eq!(rejected, control);
    let before = sum(&control);
    step(
        &mut control,
        &Env::default(),
        &[grow(1, Role::Runner, [0, 1]), grow(1, Role::Runner, [0, 1])],
    );
    assert_eq!(control.ledger.results, vec![Ok(()), Err(Rejection::Space)]);
    assert_eq!(control.tissues.len(), 3);
    assert_eq!(control.ledger.growth, [2; 3]);
    for i in 0..3 {
        assert_eq!(
            before[i] + control.ledger.intake[i],
            sum(&control)[i]
                + control.ledger.maintenance[i]
                + control.ledger.growth[i]
                + control.ledger.loss[i]
        );
    }
}

#[test]
fn role_size_upkeep_and_allocation_cannot_waive_maintenance() {
    let mut p = plant();
    p.seed(42, Role::Root, 2, [0, 0], None, [40; 3], &Env::default())
        .unwrap();
    step(&mut p, &Env::default(), &[]);
    assert_eq!(p.ledger.maintenance, [2; 3]);
    let mut p = pair();
    p.apply(
        &Intent::Allocate {
            tissue: 2,
            priority: 100,
        },
        &Env::default(),
    )
    .unwrap();
    // Drain the shared pool. Higher priority is paid first, but every tissue's
    // complete bill remains in maintenance + unmet.
    for t in p.tissues.values_mut() {
        t.stores = [0; 3];
    }
    let env = Env {
        nutrients: 0,
        water: 0,
        light: 0,
        ..Env::default()
    };
    step(&mut p, &env, &[]);
    assert_eq!(p.ledger.unmet, [2, 3, 3]);
    assert_eq!(p.ledger.maintenance, [0; 3]);
}

#[test]
fn brain_observation_is_owned_and_host_queries_are_bounded() {
    use std::cell::Cell;
    struct Counting {
        samples: Cell<usize>,
        sites: Cell<usize>,
        supports: Cell<usize>,
    }
    impl Environment for Counting {
        fn exposure(&self, t: &Tissue) -> Exposure {
            self.samples.set(self.samples.get() + 1);
            Env::default().exposure(t)
        }
        fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
            self.sites.set(self.sites.get() + 1);
            true
        }
        fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
            self.supports.set(self.supports.get() + 1);
            true
        }
    }
    struct Brain;
    impl PlantBrain for Brain {
        fn decide(&mut self, observation: &Observation) -> Vec<Intent> {
            vec![grow(observation.tissues[0].id, Role::Runner, [0, 1])]
        }
    }
    let mut p = pair();
    let before = p.clone();
    let mut observation = p.observe();
    observation.tissues[0].health = 0;
    assert_eq!(p, before);
    let intents = Brain.decide(&p.observe());
    let env = Counting {
        samples: Cell::new(0),
        sites: Cell::new(0),
        supports: Cell::new(0),
    };
    p.tick(p.tick, false, 1, &intents, &env).unwrap();
    assert_eq!(env.samples.get(), 2);
    assert_eq!(env.sites.get(), 1);
    assert_eq!(env.supports.get(), 1);
}

#[test]
fn paid_development_conserves_resources_and_preserves_identity_at_cap() {
    let mut p = pair();
    p.config.max_tissues = 2;
    // Isolate runtime accounting from the separately tuned grass profile.
    p.config.root_intake_per_size = [10, 10, 0];
    let id = 2;
    let original = p.tissues[&id].clone();
    let edges = p.edges.clone();
    for n in 1..=8 {
        let before = sum(&p);
        step(&mut p, &Env::default(), &[Intent::Develop { tissue: id }]);
        assert_eq!(p.ledger.results, vec![Ok(())]);
        for i in 0..3 {
            assert_eq!(
                before[i] + p.ledger.intake[i],
                sum(&p)[i] + p.ledger.maintenance[i] + p.ledger.growth[i] + p.ledger.loss[i]
            );
        }
        let t = &p.tissues[&id];
        assert_eq!(t.size, 1 + n / 4);
        assert_eq!(t.development, [u64::from(n % 4); 3]);
        assert_eq!(t.invested, [u64::from(n); 3]);
        assert_eq!(
            (t.id, t.organism, t.policy, t.cell, t.support),
            (
                original.id,
                original.organism,
                original.policy,
                original.cell,
                original.support
            )
        );
        assert_eq!(p.edges, edges);
        // This tick used the old size even when enlargement committed.
        assert_eq!(
            p.ledger.maintenance,
            [
                1 + u64::from(1 + (n - 1) / 4),
                1 + 2 * u64::from(1 + (n - 1) / 4),
                1 + 2 * u64::from(1 + (n - 1) / 4)
            ]
        );
    }
    let before = p.clone();
    assert_eq!(
        p.apply(&Intent::Develop { tissue: id }, &Env::default()),
        Err(Rejection::Invalid)
    );
    assert_eq!(p, before);
}

#[test]
fn development_refusals_are_atomic_and_shade_retains_paid_work() {
    let mut p = pair();
    let intent = Intent::Develop { tissue: 2 };
    step(&mut p, &Env::default(), &[intent.clone()]);
    let progress = p.tissues[&2].development;
    for (env, expected) in [
        (
            Env {
                support: false,
                ..Env::default()
            },
            Rejection::Support,
        ),
        (
            Env {
                space: false,
                ..Env::default()
            },
            Rejection::Space,
        ),
    ] {
        let before = p.clone();
        assert_eq!(p.apply(&intent, &env), Err(expected));
        assert_eq!(p, before);
    }
    step(
        &mut p,
        &Env {
            shade: Some([1, 0]),
            ..Env::default()
        },
        &[intent.clone()],
    );
    assert_eq!(p.ledger.results, vec![Err(Rejection::Shade)]);
    assert_eq!(p.tissues[&2].development, progress);
    step(&mut p, &Env::default(), &[intent.clone()]);
    assert_eq!(p.tissues[&2].development, [2; 3]);
    p.sever(1, 2).unwrap();
    p.tissues.get_mut(&2).unwrap().stores = [0; 3];
    let before = p.clone();
    assert_eq!(
        p.apply(&intent, &Env::default()),
        Err(Rejection::Unaffordable)
    );
    assert_eq!(p, before);
    p.damage(2, 100).unwrap();
    let before = p.clone();
    assert_eq!(p.apply(&intent, &Env::default()), Err(Rejection::Dead));
    assert_eq!(p, before);
}
