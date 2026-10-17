//! Versioned, deterministic grass policy. Physiology remains owned by `Plants`.
use super::*;

pub const POLICY: u64 = 0x6772_6173_7300_0001;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Traits {
    pub identity: u64,
    pub growth_interval_ticks: u16,
    pub radius: u16,
    pub max_tissues: usize,
    pub root_size: u16,
    pub shoot_size: u16,
    pub runner_size: u16,
    pub starting_reserves: Resources,
}
impl Default for Traits {
    fn default() -> Self {
        Self {
            identity: 1,
            growth_interval_ticks: 1,
            radius: 4,
            max_tissues: 32,
            root_size: 2,
            shoot_size: 1,
            runner_size: 1,
            starting_reserves: [30; 3],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grass {
    pub traits: Traits,
    pub origin: [i32; 2],
    /// Cardinal biological up in the support's local cell frame, from host gravity.
    pub up: [i32; 2],
    pub support: Support,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Limit {
    Cadence,
    Occupied,
    Size,
    Spread,
    Space,
    Support,
    Population,
    Work,
    Resource,
    Proposed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub available: Resources,
    pub cost: Resources,
    pub size: u16,
    pub parent: TissueId,
    pub role: Role,
    pub cell: [i32; 2],
    pub limit: Limit,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub waiting: Option<Limit>,
    pub intents: Vec<Intent>,
    pub decisions: Vec<Decision>,
}
impl Grass {
    pub fn new(
        traits: Traits,
        origin: [i32; 2],
        up: [i32; 2],
        support: Support,
    ) -> Result<Self, Rejection> {
        if ![[1, 0], [-1, 0], [0, 1], [0, -1]].contains(&up)
            || traits.growth_interval_ticks == 0
            || traits.radius > 128
            || traits.max_tissues == 0
            || traits.max_tissues > 256
            || [traits.root_size, traits.shoot_size, traits.runner_size].contains(&0)
        {
            return Err(Rejection::Invalid);
        }
        Ok(Self {
            traits,
            origin,
            up,
            support,
        })
    }
    /// Deliberate planting is a host action, never invoked by the growth policy.
    pub fn plant(
        &self,
        plants: &mut Plants,
        env: &impl Environment,
    ) -> Result<TissueId, Rejection> {
        plants.seed(
            POLICY,
            Role::Root,
            self.traits.root_size,
            self.origin,
            Some(self.support),
            self.traits.starting_reserves,
            env,
        )
    }
    /// At most four candidate sites per tissue, and one growth intent per tick.
    /// Availability is advisory; runtime admission and post-upkeep costs are final.
    pub fn plan(&self, o: &Observation, env: &impl Environment) -> Plan {
        let mut plan = Plan::default();
        if o.tick % u64::from(self.traits.growth_interval_ticks.max(1)) != 0 {
            plan.waiting = Some(Limit::Cadence);
            return plan;
        }
        let side = [self.up[1], -self.up[0]];
        // Build living component pools once, rather than walking the graph for
        // every candidate. Severed components never share purchasing power.
        let mut neighbors: BTreeMap<TissueId, Vec<TissueId>> = o
            .tissues
            .iter()
            .filter(|t| t.health > 0)
            .map(|t| (t.id, Vec::new()))
            .collect();
        for (a, b) in &o.connections {
            if neighbors.contains_key(a) && neighbors.contains_key(b) {
                neighbors.get_mut(a).unwrap().push(*b);
                neighbors.get_mut(b).unwrap().push(*a);
            }
        }
        let mut pools = BTreeMap::new();
        for id in neighbors.keys() {
            if pools.contains_key(id) {
                continue;
            }
            let mut group = BTreeSet::new();
            let mut pending = vec![*id];
            while let Some(current) = pending.pop() {
                if group.insert(current) {
                    pending.extend(&neighbors[&current]);
                }
            }
            let mut pool = [0; 3];
            for t in o.tissues.iter().filter(|t| group.contains(&t.id)) {
                add(&mut pool, t.stores);
            }
            for member in group {
                pools.insert(member, pool);
            }
        }

        for preferred in [Role::Shoot, Role::Root, Role::Runner] {
            for t in o
                .tissues
                .iter()
                .filter(|t| t.health > 0 && t.policy == POLICY && t.support == Some(self.support))
            {
                let candidates = match t.role {
                    Role::Root => vec![
                        (self.up, Role::Shoot, self.traits.shoot_size),
                        (side, Role::Runner, self.traits.runner_size),
                        (side.map(|v| -v), Role::Runner, self.traits.runner_size),
                    ],
                    Role::Runner => vec![
                        (self.up.map(|v| -v), Role::Root, self.traits.root_size),
                        (self.up, Role::Shoot, self.traits.shoot_size),
                        (side, Role::Runner, self.traits.runner_size),
                        (side.map(|v| -v), Role::Runner, self.traits.runner_size),
                    ],
                    Role::Shoot => vec![],
                };
                for (delta, role, size) in candidates
                    .into_iter()
                    .filter(|(_, role, _)| *role == preferred)
                {
                    let Some(x) = t.cell[0].checked_add(delta[0]) else {
                        continue;
                    };
                    let Some(y) = t.cell[1].checked_add(delta[1]) else {
                        continue;
                    };
                    let cell = [x, y];
                    let pool = pools[&t.id];
                    let cost = scaled(o.config.growth_per_size[role.index()], size);
                    let limit =
                        if o.tissues.len() >= self.traits.max_tissues.min(o.config.max_tissues) {
                            Limit::Population
                        } else if size > o.config.max_size {
                            Limit::Size
                        } else if o.config.max_intents == 0 {
                            Limit::Work
                        } else if (0..2).any(|i| {
                            (i64::from(cell[i]) - i64::from(self.origin[i])).abs()
                                > i64::from(self.traits.radius)
                        }) {
                            Limit::Spread
                        } else if o.tissues.iter().any(|t| t.cell == cell) {
                            Limit::Occupied
                        } else if !env.support_valid(Some(self.support), cell) {
                            Limit::Support
                        } else if !env.space_available(cell, role, size) {
                            Limit::Space
                        } else {
                            if (0..3).any(|i| pool[i] < cost[i]) {
                                Limit::Resource
                            } else {
                                Limit::Proposed
                            }
                        };
                    plan.decisions.push(Decision {
                        available: pool,
                        cost,
                        size,
                        parent: t.id,
                        role,
                        cell,
                        limit: limit.clone(),
                    });
                    if limit == Limit::Proposed {
                        plan.intents.push(Intent::Grow {
                            parent: t.id,
                            role,
                            size,
                            cell,
                            support: Some(self.support),
                        });
                        return plan;
                    }
                }
            }
        }
        plan
    }
}

#[cfg(test)]
mod grass_policy_contract {
    use super::*;
    struct Soil;
    impl Environment for Soil {
        fn exposure(&self, t: &Tissue) -> Exposure {
            Exposure {
                dirt_contact: t.role == Role::Root,
                nutrients: 1000,
                water: 1000,
                light: 1000,
            }
        }
        fn space_available(&self, cell: [i32; 2], role: Role, _: u16) -> bool {
            if role == Role::Shoot {
                cell[1] == 1
            } else {
                cell[1] <= 0
            }
        }
        fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
            true
        }
    }
    fn run() -> Vec<(Observation, Plan)> {
        let grass = Grass::new(
            Traits::default(),
            [0, 0],
            [0, 1],
            Support {
                body: 1,
                generation: 1,
            },
        )
        .unwrap();
        let mut p = Plants::new(Config::default(), 42).unwrap();
        grass.plant(&mut p, &Soil).unwrap();
        let mut history = vec![];
        for _ in 0..60 {
            let o = p.observe();
            let plan = grass.plan(&o, &Soil);
            assert!(plan.intents.len() <= 1);
            assert!(plan.decisions.len() <= 4 * o.tissues.len());
            p.tick(o.tick, false, 1, &plan.intents, &Soil).unwrap();
            assert!(p.observe().tissues.len() <= grass.traits.max_tissues);
            history.push((p.observe(), plan));
        }
        history
    }
    #[test]
    fn grass_policy_contract_paid_deterministic_bounded_network() {
        let a = run();
        assert_eq!(a, run());
        let o = &a[10].0;
        for role in [Role::Root, Role::Runner, Role::Shoot] {
            assert!(o.tissues.iter().any(|t| t.role == role));
        }
        assert!(a.iter().any(|(o, _)| o.ledger.growth != [0; 3]));
        assert!(o.tissues.iter().all(|t| t.policy == POLICY));
    }
    #[test]
    fn grass_policy_contract_damage_sever_and_total_death() {
        let grass = Grass::new(
            Traits::default(),
            [0, 0],
            [0, 1],
            Support {
                body: 1,
                generation: 1,
            },
        )
        .unwrap();
        let mut p = Plants::new(Config::default(), 42).unwrap();
        grass.plant(&mut p, &Soil).unwrap();
        for _ in 0..4 {
            let o = p.observe();
            p.tick(o.tick, false, 1, &grass.plan(&o, &Soil).intents, &Soil)
                .unwrap();
        }
        let before = p.observe();
        let edge = before.connections[0];
        p.sever(edge.0, edge.1).unwrap();
        let total = |o: &Observation| -> Resources {
            std::array::from_fn(|i| o.tissues.iter().map(|t| t.stores[i]).sum())
        };
        assert_eq!(total(&before), total(&p.observe()));
        assert_eq!(before.tissues[1].stores, p.observe().tissues[1].stores);
        let shoot = p
            .observe()
            .tissues
            .iter()
            .find(|t| t.role == Role::Shoot)
            .unwrap()
            .id;
        p.damage(shoot, 100).unwrap();
        assert_eq!(
            p.observe()
                .tissues
                .iter()
                .find(|t| t.id == shoot)
                .unwrap()
                .health,
            0
        );
        let snapshot = p.clone();
        assert_eq!(p.damage(shoot, 101), Err(Rejection::Invalid));
        assert_eq!(p, snapshot);
        for t in p.observe().tissues {
            p.damage(t.id, 100).unwrap();
        }
        for _ in 0..20 {
            let o = p.observe();
            let plan = grass.plan(&o, &Soil);
            assert!(plan.intents.is_empty());
            p.tick(o.tick, false, 1, &[], &Soil).unwrap();
        }
        assert!(p.observe().tissues.iter().all(|t| t.health == 0));
    }
}

#[cfg(test)]
mod grass_policy_contract_limits {
    use super::*;
    struct Site;
    impl Environment for Site {
        fn exposure(&self, _: &Tissue) -> Exposure {
            Exposure::default()
        }
        fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
            true
        }
        fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
            true
        }
    }
    #[test]
    fn grass_policy_contract_limits_and_pause_are_inspectable() {
        let support = Support {
            body: 1,
            generation: 1,
        };
        assert_eq!(
            Grass::new(Traits::default(), [0, 0], [1, 1], support),
            Err(Rejection::Invalid)
        );
        let mut grass = Grass::new(Traits::default(), [0, 0], [0, 1], support).unwrap();
        let mut plants = Plants::new(Config::default(), 1).unwrap();
        grass.plant(&mut plants, &Site).unwrap();
        let o = plants.observe();
        let plan = grass.plan(&o, &Site);
        let before = plants.clone();
        plants.tick(o.tick, true, 1, &plan.intents, &Site).unwrap();
        assert_eq!(plants, before);
        plants.tick(o.tick, false, 0, &plan.intents, &Site).unwrap();
        assert_eq!(plants, before);
        grass.traits.radius = 0;
        assert!(
            grass
                .plan(&o, &Site)
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Spread)
        );
        grass.traits.max_tissues = 1;
        assert!(
            grass
                .plan(&o, &Site)
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Population)
        );
        grass.traits.growth_interval_ticks = 2;
        let mut delayed = o.clone();
        delayed.tick = 1;
        assert_eq!(grass.plan(&delayed, &Site).waiting, Some(Limit::Cadence));
        let mut poor = o;
        poor.tissues[0].stores = [0; 3];
        grass.traits = Traits::default();
        assert!(
            grass
                .plan(&poor, &Site)
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Resource)
        );
    }
}

#[cfg(test)]
mod grass_policy_contract_ordered_refusals {
    use super::*;
    struct Site {
        support: bool,
        space: bool,
    }
    impl Environment for Site {
        fn exposure(&self, _: &Tissue) -> Exposure {
            Exposure::default()
        }
        fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
            self.support
        }
        fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
            self.space
        }
    }
    #[test]
    fn actual_candidates_preserve_masking_and_partial_enumeration() {
        let support = Support {
            body: 1,
            generation: 1,
        };
        let mut grass = Grass::new(Traits::default(), [0, 0], [0, 1], support).unwrap();
        let mut plants = Plants::new(Config::default(), 42).unwrap();
        grass
            .plant(
                &mut plants,
                &Site {
                    support: true,
                    space: true,
                },
            )
            .unwrap();
        let mut o = plants.observe();
        let bad = Site {
            support: false,
            space: false,
        };
        assert!(
            grass
                .plan(&o, &bad)
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Support)
        );
        assert!(
            grass
                .plan(
                    &o,
                    &Site {
                        support: true,
                        space: false
                    }
                )
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Space)
        );
        o.tissues[0].stores = [0; 3];
        let good = Site {
            support: true,
            space: true,
        };
        assert!(
            grass
                .plan(&o, &good)
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Resource)
        );
        grass.traits.radius = 0;
        assert!(
            grass
                .plan(&o, &bad)
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Spread)
        );
        grass.traits.max_tissues = 1;
        assert!(
            grass
                .plan(&o, &bad)
                .decisions
                .iter()
                .all(|d| d.limit == Limit::Population)
        );
        grass.traits = Traits::default();
        o.tissues[0].stores = [30; 3];
        let mut occupied = o.tissues[0].clone();
        occupied.id = 99;
        occupied.cell = [0, 1];
        occupied.health = 0;
        o.tissues.push(occupied);
        let mixed = grass.plan(&o, &bad);
        assert_eq!(mixed.decisions[0].limit, Limit::Occupied);
        assert!(
            mixed
                .decisions
                .iter()
                .skip(1)
                .all(|d| d.limit == Limit::Support)
        );
        let partial = grass.plan(&o, &good);
        assert_eq!(partial.decisions.len(), 2);
        assert_eq!(partial.decisions[1].limit, Limit::Proposed);
        assert_eq!(partial.intents.len(), 1);
        o.tissues[0].health = 0;
        assert!(grass.plan(&o, &good).decisions.is_empty());
    }
}
