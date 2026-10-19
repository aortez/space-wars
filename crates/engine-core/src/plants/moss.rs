//! Versioned, deterministic moss policy. Physiology remains owned by `Plants`.
use super::*;

/// Moss policy version 1: contiguous paid surface spread from a placed patch.
pub const POLICY: u64 = 0x6d6f_7373_0000_0001;
/// Moss is one thin layer; every element has this physiological size.
pub const TISSUE_SIZE: u16 = 1;
/// Candidate order in the support's local frame: +x, -x, +y, -y.
const NEIGHBORS: [[i32; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Traits {
    pub identity: u64,
    /// One growth opportunity per this many fixed ticks; physiology still runs.
    pub growth_interval_ticks: u16,
    /// Bounded patch size: this policy's living and dead cells on its support.
    pub max_tissues: usize,
    /// Explicit finite reserves for a deliberately placed patch.
    pub starting_reserves: Resources,
}
impl Default for Traits {
    fn default() -> Self {
        Self {
            identity: 1,
            growth_interval_ticks: 4,
            max_tissues: 24,
            starting_reserves: [24; 3],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moss {
    pub traits: Traits,
    /// Initial cell in the support's local frame.
    pub origin: [i32; 2],
    pub support: Support,
}
impl Moss {
    pub fn new(traits: Traits, origin: [i32; 2], support: Support) -> Result<Self, Rejection> {
        if traits.growth_interval_ticks == 0 || traits.max_tissues == 0 || traits.max_tissues > 256
        {
            return Err(Rejection::Invalid);
        }
        Ok(Self {
            traits,
            origin,
            support,
        })
    }
    /// Deliberate placement is a host action, never invoked by the growth policy.
    pub fn place(
        &self,
        plants: &mut Plants,
        env: &impl Environment,
    ) -> Result<TissueId, Rejection> {
        plants.seed(
            POLICY,
            Role::Moss,
            TISSUE_SIZE,
            self.origin,
            Some(self.support),
            self.traits.starting_reserves,
            env,
        )
    }
    /// Expand from healthy, unstressed tissue into one adjacent free surface
    /// cell per growth opportunity, inspecting at most four sites per tissue.
    /// Availability is advisory; runtime admission and post-upkeep costs are final.
    pub fn plan(&self, o: &Observation, env: &impl Environment) -> Plan {
        let mut plan = Plan::default();
        if !o
            .tick
            .is_multiple_of(u64::from(self.traits.growth_interval_ticks.max(1)))
        {
            plan.waiting = Some(Limit::Cadence);
            return plan;
        }
        let pools = component_pools(o);
        let own = |t: &&Tissue| t.policy == POLICY && t.support == Some(self.support);
        let patch = o.tissues.iter().filter(own).count();
        let cost = scaled(o.config.growth_per_size[Role::Moss.index()], TISSUE_SIZE);
        for t in o.tissues.iter().filter(own).filter(|t| t.health > 0) {
            let available = pools[&t.id];
            let decision = |cell, limit| Decision {
                available,
                cost,
                size: TISSUE_SIZE,
                parent: t.id,
                role: Role::Moss,
                cell,
                limit,
            };
            // Stressed or damaged tissue waits for its own recovery rather than
            // spending shared reserves on further expansion.
            let wait = if t.shade_ticks > 0 {
                Some(Limit::Shade)
            } else if t.shortage_ticks > 0 {
                Some(Limit::Shortage)
            } else if t.health < 100 {
                Some(Limit::Damage)
            } else {
                None
            };
            if let Some(limit) = wait {
                plan.decisions.push(decision(t.cell, limit));
                continue;
            }
            for delta in NEIGHBORS {
                let (Some(x), Some(y)) = (
                    t.cell[0].checked_add(delta[0]),
                    t.cell[1].checked_add(delta[1]),
                ) else {
                    continue;
                };
                let cell = [x, y];
                let limit = if patch >= self.traits.max_tissues
                    || o.tissues.len() >= o.config.max_tissues
                {
                    Limit::Population
                } else if TISSUE_SIZE > o.config.max_size {
                    Limit::Size
                } else if o.config.max_intents == 0 {
                    Limit::Work
                } else if o.tissues.iter().any(|other| other.cell == cell) {
                    Limit::Occupied
                } else if !env.support_valid(Some(self.support), cell) {
                    Limit::Support
                } else if !env.space_available(cell, Role::Moss, TISSUE_SIZE) {
                    Limit::Space
                } else if (0..3).any(|i| available[i] < cost[i]) {
                    Limit::Resource
                } else {
                    Limit::Proposed
                };
                plan.decisions.push(decision(cell, limit.clone()));
                if limit == Limit::Proposed {
                    plan.intents.push(Intent::Grow {
                        parent: t.id,
                        role: Role::Moss,
                        size: TISSUE_SIZE,
                        cell,
                        support: Some(self.support),
                    });
                    return plan;
                }
            }
        }
        plan
    }
}

#[cfg(test)]
mod moss_policy_contract {
    use super::*;
    const SUPPORT: Support = Support {
        body: 7,
        generation: 1,
    };
    /// A flat ledge: solid below y = 0 for x in 0..12, air above. Moss cells sit
    /// at y = 0. Listed cells are dark; `contact` toggles surface supply.
    #[derive(Clone)]
    struct Ledge {
        light: u16,
        dark: Vec<[i32; 2]>,
        nutrients: u16,
        water: u16,
        contact: bool,
    }
    impl Default for Ledge {
        fn default() -> Self {
            Self {
                light: 50,
                dark: Vec::new(),
                nutrients: 1000,
                water: 1000,
                contact: true,
            }
        }
    }
    impl Environment for Ledge {
        fn exposure(&self, t: &Tissue) -> Exposure {
            Exposure {
                dirt_contact: false,
                surface_contact: self.contact && t.role == Role::Moss,
                nutrients: self.nutrients,
                water: self.water,
                light: if self.dark.contains(&t.cell) {
                    0
                } else {
                    self.light
                },
            }
        }
        fn space_available(&self, cell: [i32; 2], role: Role, _: u16) -> bool {
            role == Role::Moss && cell[1] == 0 && (0..12).contains(&cell[0])
        }
        fn support_valid(&self, support: Option<Support>, _: [i32; 2]) -> bool {
            support == Some(SUPPORT)
        }
    }
    fn moss() -> Moss {
        Moss::new(Traits::default(), [5, 0], SUPPORT).unwrap()
    }
    fn placed(env: &Ledge) -> Plants {
        let mut p = Plants::new(Config::default(), 7).unwrap();
        moss().place(&mut p, env).unwrap();
        p
    }
    fn run(p: &mut Plants, env: &Ledge, ticks: usize) -> Vec<(Observation, Plan)> {
        let moss = moss();
        (0..ticks)
            .map(|_| {
                let o = p.observe();
                let plan = moss.plan(&o, env);
                assert!(plan.intents.len() <= 1);
                assert!(plan.decisions.len() <= 4 * o.tissues.len());
                p.tick(o.tick, false, 1, &plan.intents, env).unwrap();
                let after = p.observe();
                assert!(after.tissues.len() <= moss.traits.max_tissues);
                for i in 0..3 {
                    assert_eq!(
                        o.tissues.iter().map(|t| t.stores[i]).sum::<u64>() + after.ledger.intake[i],
                        after.tissues.iter().map(|t| t.stores[i]).sum::<u64>()
                            + after.ledger.maintenance[i]
                            + after.ledger.growth[i]
                            + after.ledger.loss[i]
                    );
                }
                (after, plan)
            })
            .collect()
    }

    #[test]
    fn dim_surface_growth_is_contiguous_paid_and_replayable() {
        let env = Ledge::default();
        let mut p = placed(&env);
        let history = run(&mut p, &env, 120);
        assert_eq!(history, run(&mut placed(&env), &env, 120));
        let end = &history.last().unwrap().0;
        // The ledge holds twelve cells; dim light still pays for all of them.
        assert_eq!(end.tissues.len(), 12);
        assert!(
            end.tissues
                .iter()
                .all(|t| t.role == Role::Moss && t.health == 100 && t.policy == POLICY)
        );
        for (a, b) in &end.connections {
            let a = end.tissues.iter().find(|t| t.id == *a).unwrap();
            let b = end.tissues.iter().find(|t| t.id == *b).unwrap();
            assert_eq!(
                (a.cell[0] - b.cell[0]).abs() + (a.cell[1] - b.cell[1]).abs(),
                1
            );
        }
        let growth: u64 = history.iter().map(|(o, _)| o.ledger.growth[0]).sum();
        assert_eq!(growth, 11 * 20);
        // Each growth opportunity is every fourth tick at most.
        for (o, plan) in &history {
            if (o.tick - 1) % 4 != 0 {
                assert_eq!(plan.waiting, Some(Limit::Cadence));
            }
        }
        let surface = &end.ledger.surfaces[&1];
        assert!(surface.eligible);
        assert_eq!(surface.offered, [2, 2, 0]);
        assert!(end.ledger.roots.is_empty());
        assert_eq!(end.ledger.maintenance, [12; 3]);
    }

    #[test]
    fn moss_tolerates_dim_light_that_shades_grass_shoots() {
        let env = Ledge::default();
        let mut p = placed(&env);
        let mut shoot = Plants::new(Config::default(), 7).unwrap();
        struct Dim;
        impl Environment for Dim {
            fn exposure(&self, _: &Tissue) -> Exposure {
                Exposure {
                    light: 50,
                    ..Exposure::default()
                }
            }
            fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
                true
            }
            fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
                true
            }
        }
        shoot
            .seed(1, Role::Shoot, 1, [0, 0], None, [20; 3], &Dim)
            .unwrap();
        for _ in 0..10 {
            p.tick(p.observe().tick, false, 1, &[], &env).unwrap();
            shoot
                .tick(shoot.observe().tick, false, 1, &[], &Dim)
                .unwrap();
        }
        let m = &p.observe().tissues[0];
        let s = &shoot.observe().tissues[0];
        assert_eq!((m.shade_ticks, m.shortage_ticks, m.health), (0, 0, 100));
        assert_eq!(p.observe().ledger.intake, [2, 2, 2]);
        // The same dim exposure offers a size-one shoot no energy and is below
        // its local light threshold, so it is already dying.
        assert!(s.shade_ticks > Config::default().grace_ticks);
        assert!(s.health < 100);
    }

    #[test]
    fn local_darkness_kills_connected_moss_and_restored_light_recovers() {
        let env = Ledge::default();
        let mut p = placed(&env);
        run(&mut p, &env, 120);
        let dark = vec![[0, 0], [1, 0], [2, 0]];
        let shaded = Ledge {
            dark: dark.clone(),
            ..Ledge::default()
        };
        let config = p.observe().config;
        // Grace plus damage at the default rates, then one tick to confirm.
        let deadline = config.grace_ticks as usize + 100 / config.damage_per_tick as usize + 1;
        let mut recovering = p.clone();
        run(&mut recovering, &shaded, config.grace_ticks as usize + 2);
        let stressed = recovering.observe();
        assert!(
            stressed
                .tissues
                .iter()
                .any(|t| dark.contains(&t.cell) && t.health < 100 && t.health > 0)
        );
        run(&mut recovering, &env, 20);
        assert!(recovering.observe().tissues.iter().all(|t| t.health == 100));

        let history = run(&mut p, &shaded, deadline);
        let end = &history.last().unwrap().0;
        for t in &end.tissues {
            if dark.contains(&t.cell) {
                assert_eq!(t.health, 0, "dark moss survived: {t:?}");
            } else {
                assert_eq!(t.health, 100, "lit moss was harmed: {t:?}");
            }
        }
        // Dead cells stay occupied; nothing replaces them for free.
        assert_eq!(end.tissues.len(), 12);
        let total = Ledge {
            light: 0,
            ..Ledge::default()
        };
        run(&mut p, &total, deadline);
        assert!(p.observe().tissues.iter().all(|t| t.health == 0));
        for (_, plan) in run(&mut p, &env, 40) {
            assert!(plan.intents.is_empty());
        }
        assert!(p.observe().tissues.iter().all(|t| t.health == 0));
    }

    #[test]
    fn supply_requires_contact_and_each_resource_limits_independently() {
        let env = Ledge {
            contact: false,
            ..Ledge::default()
        };
        let mut p = placed(&env);
        p.tick(0, false, 1, &[], &env).unwrap();
        let o = p.observe();
        let surface = &o.ledger.surfaces[&1];
        assert!(!surface.eligible);
        assert_eq!(surface.offered, [0; 3]);
        assert_eq!(surface.zero, [Some(UptakeZero::NoContact); 2]);
        assert_eq!(o.ledger.intake, [0, 0, 2]);
        for resource in 0..2 {
            let mut env = Ledge::default();
            if resource == 0 {
                env.nutrients = 0;
            } else {
                env.water = 0;
            }
            let mut p = placed(&env);
            let history = run(&mut p, &env, 200);
            let short = history
                .iter()
                .find(|(o, _)| o.ledger.unmet[resource] > 0)
                .expect("an independent supply cut must eventually cause shortage");
            assert_eq!(short.0.ledger.unmet[1 - resource], 0);
            assert!(p.observe().tissues.iter().all(|t| t.health == 0));
            assert_eq!(p.observe().ledger.surfaces.len(), 0);
        }
    }

    #[test]
    fn stressed_and_damaged_parents_wait_and_decisions_explain_limits() {
        let env = Ledge::default();
        let mut p = placed(&env);
        let moss = moss();
        let o = p.observe();
        assert_eq!(moss.plan(&o, &env).intents.len(), 1);
        p.damage(1, 30).unwrap();
        let o = p.observe();
        let plan = moss.plan(&o, &env);
        assert!(plan.intents.is_empty());
        assert_eq!(plan.decisions[0].limit, Limit::Damage);
        let mut poor = p.observe();
        poor.tissues[0].health = 100;
        poor.tissues[0].stores = [0; 3];
        assert!(
            moss.plan(&poor, &env)
                .decisions
                .iter()
                .all(|d| matches!(d.limit, Limit::Resource | Limit::Space))
        );
        for (field, limit) in [(0, Limit::Shade), (1, Limit::Shortage)] {
            let mut stressed = poor.clone();
            stressed.tissues[0].stores = [24; 3];
            if field == 0 {
                stressed.tissues[0].shade_ticks = 1;
            } else {
                stressed.tissues[0].shortage_ticks = 1;
            }
            let plan = moss.plan(&stressed, &env);
            assert!(plan.intents.is_empty());
            assert_eq!(plan.decisions[0].limit, limit);
        }
        let mut crowded = moss.clone();

        crowded.traits.max_tissues = 1;
        assert!(
            crowded
                .plan(&poor, &env)
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Population)
        );
        let mut delayed = poor.clone();
        delayed.tick = 1;
        assert_eq!(moss.plan(&delayed, &env).waiting, Some(Limit::Cadence));
        let elsewhere = Moss::new(
            Traits::default(),
            [5, 0],
            Support {
                body: 8,
                generation: 1,
            },
        )
        .unwrap();
        assert!(elsewhere.plan(&poor, &env).decisions.is_empty());
        assert_eq!(
            Moss::new(
                Traits {
                    growth_interval_ticks: 0,
                    ..Traits::default()
                },
                [0, 0],
                SUPPORT
            ),
            Err(Rejection::Invalid)
        );
    }

    #[test]
    fn pause_zero_and_removal_never_create_free_moss() {
        let env = Ledge::default();
        let mut p = placed(&env);
        run(&mut p, &env, 40);
        let before = p.clone();
        let o = p.observe();
        let plan = moss().plan(&o, &env);
        p.tick(o.tick, true, 1, &plan.intents, &env).unwrap();
        p.tick(o.tick, false, 0, &plan.intents, &env).unwrap();
        assert_eq!(p, before);
        let ids: Vec<_> = o.tissues.iter().map(|t| t.id).collect();
        let middle = ids[ids.len() / 2];
        let loss_before = o.ledger.loss;
        let removed = o.tissues.iter().find(|t| t.id == middle).unwrap().stores;
        p.remove(middle).unwrap();
        let cut = p.observe();
        for i in 0..3 {
            assert_eq!(cut.ledger.loss[i], loss_before[i] + removed[i]);
        }
        let history = run(&mut p, &env, 80);
        let end = &history.last().unwrap().0;
        assert!(end.tissues.iter().all(|t| t.id != middle));
        assert!(end.tissues.iter().any(|t| t.id > *ids.last().unwrap()));
        assert!(end.tissues.iter().all(|t| t.organism == 1));
    }
}
