//! Shared, headless plant physiology. See `docs/plants.md` for units and order.
use std::collections::{BTreeMap, BTreeSet};

pub mod grass;

pub type TissueId = u64;
pub type OrganismId = u64;
/// Nutrients, water, chemical energy, in abstract integer resource units.
pub type Resources = [u64; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Root,
    Runner,
    Shoot,
}
impl Role {
    fn index(self) -> usize {
        match self {
            Self::Root => 0,
            Self::Runner => 1,
            Self::Shoot => 2,
        }
    }
}

/// Opaque reference into the host's canonical physics world. The host validates
/// generation/liveness and attachment; this module creates no physics bodies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Support {
    pub body: u64,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tissue {
    pub id: TissueId,
    pub organism: OrganismId,
    pub policy: u64,
    pub role: Role,
    pub cell: [i32; 2],
    pub size: u16,
    pub support: Option<Support>,
    pub health: u16,
    pub shortage_ticks: u32,
    pub shade_ticks: u32,
    /// Canonical capacity-weighted share of its connected component's stores.
    pub stores: Resources,
    pub allocation: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub identity: u64,
    pub max_tissues: usize,
    pub max_organisms: usize,
    pub max_intents: usize,
    pub max_size: u16,
    pub capacity_per_size: [Resources; 3],
    pub maintenance_per_size: [Resources; 3],
    pub growth_per_size: [Resources; 3],
    pub root_intake_per_size: Resources,
    pub shoot_energy_per_size: u64,
    pub local_light_threshold: u16,
    pub grace_ticks: u32,
    pub damage_per_tick: u16,
    pub recovery_per_tick: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            identity: 1,
            max_tissues: 256,
            max_organisms: 64,
            max_intents: 16,
            max_size: 16,
            capacity_per_size: [[20; 3], [10; 3], [20; 3]],
            maintenance_per_size: [[1, 1, 1], [1, 1, 1], [1, 2, 2]],
            growth_per_size: [[3; 3], [2; 3], [4; 3]],
            root_intake_per_size: [4, 3, 0],
            shoot_energy_per_size: 8,
            local_light_threshold: 250,
            grace_ticks: 3,
            damage_per_tick: 20,
            recovery_per_tick: 10,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Invalid,
    Dead,
    Space,
    Support,
    Unaffordable,
    TissueLimit,
    OrganismLimit,
    WorkLimit,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UptakeZero {
    NoContact,
    SupplyOrRounding,
    Capacity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootIntake {
    pub eligible: bool,
    pub offered: Resources,
    pub accepted: Resources,
    pub overflow: Resources,
    pub zero: [Option<UptakeZero>; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub tick: u64,
    pub intake: Resources,
    pub roots: BTreeMap<TissueId, RootIntake>,
    pub maintenance: Resources,
    pub unmet: Resources,
    pub growth: Resources,
    pub loss: Resources,
    pub results: Vec<Result<(), Rejection>>,
    pub waiting: BTreeMap<TissueId, WaitReason>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitReason {
    Idle,
    Shortage,
    Shade,
    Dead,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Grow {
        parent: TissueId,
        role: Role,
        size: u16,
        cell: [i32; 2],
        support: Option<Support>,
    },
    /// Relative maintenance priority inside a living component; does not waive costs.
    Allocate { tissue: TissueId, priority: u16 },
}

/// Inputs come from bounded host adapters, never from the plant brain. Light is
/// 0..=1000; soil supply fractions bound passive uptake independently per resource.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Exposure {
    pub dirt_contact: bool,
    pub nutrients: u16,
    pub water: u16,
    pub light: u16,
}
pub trait Environment {
    fn exposure(&self, tissue: &Tissue) -> Exposure;
    fn space_available(&self, cell: [i32; 2], role: Role, size: u16) -> bool;
    fn support_valid(&self, support: Option<Support>, cell: [i32; 2]) -> bool;
}
impl<E: Environment + ?Sized> Environment for &E {
    fn exposure(&self, tissue: &Tissue) -> Exposure {
        (**self).exposure(tissue)
    }
    fn space_available(&self, cell: [i32; 2], role: Role, size: u16) -> bool {
        (**self).space_available(cell, role, size)
    }
    fn support_valid(&self, support: Option<Support>, cell: [i32; 2]) -> bool {
        (**self).support_valid(support, cell)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub tick: u64,
    pub seed: u64,
    pub config: Config,
    pub tissues: Vec<Tissue>,
    pub connections: Vec<(TissueId, TissueId)>,
    pub ledger: Ledger,
}
pub trait PlantBrain {
    fn decide(&mut self, observation: &Observation) -> Vec<Intent>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plants {
    config: Config,
    seed: u64,
    tick: u64,
    next_tissue: u64,
    next_organism: u64,
    tissues: BTreeMap<TissueId, Tissue>,
    edges: BTreeSet<(TissueId, TissueId)>,
    ledger: Ledger,
}
fn add(a: &mut Resources, b: Resources) {
    for i in 0..3 {
        a[i] += b[i];
    }
}
fn scaled(a: Resources, size: u16) -> Resources {
    a.map(|v| v * u64::from(size))
}
fn edge(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}
impl Plants {
    pub fn new(config: Config, seed: u64) -> Result<Self, Rejection> {
        // Hard safety ceilings keep arithmetic and all graph walks bounded.
        if config.max_tissues == 0
            || config.max_tissues > 1024
            || config.max_organisms == 0
            || config.max_organisms > config.max_tissues
            || config.max_intents > 1024
            || config.max_size == 0
            || config.local_light_threshold > 1000
            || config.grace_ticks == u32::MAX
            || config.damage_per_tick == 0
            || config.damage_per_tick > 100
            || config.recovery_per_tick > 100
            || config
                .capacity_per_size
                .iter()
                .chain(config.maintenance_per_size.iter())
                .chain(config.growth_per_size.iter())
                .flatten()
                .any(|v| *v > 1_000_000)
            || config.root_intake_per_size.iter().any(|v| *v > 1_000_000)
            || config.shoot_energy_per_size > 1_000_000
            || config.root_intake_per_size[2] != 0
            || config
                .growth_per_size
                .iter()
                .any(|v| v.iter().any(|x| *x == 0))
            || config
                .maintenance_per_size
                .iter()
                .any(|v| v.iter().any(|x| *x == 0))
        {
            return Err(Rejection::Invalid);
        }
        Ok(Self {
            config,
            seed,
            tick: 0,
            next_tissue: 1,
            next_organism: 1,
            tissues: BTreeMap::new(),
            edges: BTreeSet::new(),
            ledger: Ledger::default(),
        })
    }
    pub fn observe(&self) -> Observation {
        Observation {
            tick: self.tick,
            seed: self.seed,
            config: self.config.clone(),
            tissues: self.tissues.values().cloned().collect(),
            connections: self.edges.iter().copied().collect(),
            ledger: self.ledger.clone(),
        }
    }
    fn capacity(&self, id: u64) -> Resources {
        let t = &self.tissues[&id];
        if t.health == 0 {
            [0; 3]
        } else {
            scaled(self.config.capacity_per_size[t.role.index()], t.size)
        }
    }
    fn components(&self) -> Vec<Vec<u64>> {
        let mut remaining: BTreeSet<_> = self
            .tissues
            .values()
            .filter(|t| t.health > 0)
            .map(|t| t.id)
            .collect();
        let mut result = Vec::new();
        while let Some(first) = remaining.pop_first() {
            let mut group = vec![first];
            let mut cursor = 0;
            while cursor < group.len() {
                let id = group[cursor];
                cursor += 1;
                for &(a, b) in &self.edges {
                    let neighbor = if a == id {
                        b
                    } else if b == id {
                        a
                    } else {
                        continue;
                    };
                    if remaining.remove(&neighbor) {
                        group.push(neighbor);
                    }
                }
            }
            group.sort_unstable();
            result.push(group);
        }
        result
    }
    fn total(&self, group: &[u64]) -> Resources {
        let mut total = [0; 3];
        for id in group {
            add(&mut total, self.tissues[id].stores);
        }
        total
    }
    /// Deterministic largest-prefix rounding: each resource remainder goes in ID
    /// order, never past capacity. Called before any topology edit, so severing
    /// retains exactly the former capacity-weighted shares in each new component.
    fn distribute(&mut self, group: &[u64], total: Resources) {
        let mut capacity = [0; 3];
        for id in group {
            add(&mut capacity, self.capacity(*id));
        }
        let kept = std::array::from_fn::<_, 3, _>(|i| total[i].min(capacity[i]));
        for i in 0..3 {
            self.ledger.loss[i] += total[i] - kept[i];
        }
        let mut assigned = [0; 3];
        for id in group {
            let cap = self.capacity(*id);
            let share = std::array::from_fn(|i| {
                if capacity[i] == 0 {
                    0
                } else {
                    ((u128::from(kept[i]) * u128::from(cap[i])) / u128::from(capacity[i])) as u64
                }
            });
            self.tissues.get_mut(id).unwrap().stores = share;
            add(&mut assigned, share);
        }
        for id in group {
            let cap = self.capacity(*id);
            let t = self.tissues.get_mut(id).unwrap();
            for i in 0..3 {
                let extra = (kept[i] - assigned[i]).min(cap[i] - t.stores[i]);
                t.stores[i] += extra;
                assigned[i] += extra;
            }
        }
    }
    fn normalize(&mut self) {
        for group in self.components() {
            let total = self.total(&group);
            self.distribute(&group, total);
        }
    }
    fn organism_count(&self) -> usize {
        self.tissues
            .values()
            .map(|t| t.organism)
            .collect::<BTreeSet<_>>()
            .len()
    }
    /// Explicit finite initial endowment, only for a NEW organism. Dead organisms
    /// cannot be revived through this API; IDs are never reused.
    pub fn seed(
        &mut self,
        policy: u64,
        role: Role,
        size: u16,
        cell: [i32; 2],
        support: Option<Support>,
        stores: Resources,
        env: &impl Environment,
    ) -> Result<TissueId, Rejection> {
        self.validate_site(role, size, cell, support, env)?;
        if self.organism_count() >= self.config.max_organisms {
            return Err(Rejection::OrganismLimit);
        }
        if self.tissues.len() >= self.config.max_tissues {
            return Err(Rejection::TissueLimit);
        }
        let cap = scaled(self.config.capacity_per_size[role.index()], size);
        if (0..3).any(|i| stores[i] > cap[i]) {
            return Err(Rejection::Invalid);
        }
        let id = self.next_tissue;
        let organism = self.next_organism;
        self.next_tissue = self.next_tissue.checked_add(1).ok_or(Rejection::Invalid)?;
        self.next_organism = self
            .next_organism
            .checked_add(1)
            .ok_or(Rejection::Invalid)?;
        self.tissues.insert(
            id,
            Tissue {
                id,
                organism,
                policy,
                role,
                cell,
                size,
                support,
                health: 100,
                shortage_ticks: 0,
                shade_ticks: 0,
                stores,
                allocation: 0,
            },
        );
        Ok(id)
    }
    fn validate_site(
        &self,
        role: Role,
        size: u16,
        cell: [i32; 2],
        support: Option<Support>,
        env: &impl Environment,
    ) -> Result<(), Rejection> {
        if size == 0 || size > self.config.max_size {
            return Err(Rejection::Invalid);
        }
        if self.tissues.values().any(|t| t.cell == cell) || !env.space_available(cell, role, size) {
            return Err(Rejection::Space);
        }
        if !env.support_valid(support, cell) {
            return Err(Rejection::Support);
        }
        Ok(())
    }
    fn apply(&mut self, intent: &Intent, env: &impl Environment) -> Result<(), Rejection> {
        match *intent {
            Intent::Allocate { tissue, priority } => {
                let t = self.tissues.get_mut(&tissue).ok_or(Rejection::Invalid)?;
                if t.health == 0 {
                    return Err(Rejection::Dead);
                }
                t.allocation = priority;
                Ok(())
            }
            Intent::Grow {
                parent,
                role,
                size,
                cell,
                support,
            } => {
                let p = self.tissues.get(&parent).ok_or(Rejection::Invalid)?.clone();
                if p.health == 0 {
                    return Err(Rejection::Dead);
                }
                let distance = (i64::from(p.cell[0]) - i64::from(cell[0])).abs()
                    + (i64::from(p.cell[1]) - i64::from(cell[1])).abs();
                if distance != 1 {
                    return Err(Rejection::Invalid);
                }
                self.validate_site(role, size, cell, support, env)?;
                if self.tissues.len() >= self.config.max_tissues {
                    return Err(Rejection::TissueLimit);
                }
                let group = self
                    .components()
                    .into_iter()
                    .find(|g| g.contains(&parent))
                    .unwrap();
                let total = self.total(&group);
                let cost = scaled(self.config.growth_per_size[role.index()], size);
                if (0..3).any(|i| total[i] < cost[i]) {
                    return Err(Rejection::Unaffordable);
                }
                let id = self.next_tissue;
                let next = id.checked_add(1).ok_or(Rejection::Invalid)?;
                let left = std::array::from_fn(|i| total[i] - cost[i]);
                self.next_tissue = next;
                self.tissues.insert(
                    id,
                    Tissue {
                        id,
                        organism: p.organism,
                        policy: p.policy,
                        role,
                        cell,
                        size,
                        support,
                        health: 100,
                        shortage_ticks: 0,
                        shade_ticks: 0,
                        stores: [0; 3],
                        allocation: 0,
                    },
                );
                self.edges.insert(edge(parent, id));
                add(&mut self.ledger.growth, cost);
                let mut grown = group;
                grown.push(id);
                grown.sort_unstable();
                self.distribute(&grown, left);
                Ok(())
            }
        }
    }
    /// Growth is only committed during tick(), after maintenance, never directly
    /// by a brain. Requests beyond max_intents reject the whole input batch.
    pub fn tick(
        &mut self,
        expected_tick: u64,
        paused: bool,
        elapsed_ticks: u8,
        intents: &[Intent],
        env: &impl Environment,
    ) -> Result<(), Rejection> {
        if paused || elapsed_ticks == 0 {
            return Ok(());
        }
        if elapsed_ticks != 1 {
            return Err(Rejection::Invalid);
        }
        if expected_tick != self.tick || self.tick == u64::MAX {
            return Err(Rejection::Stale);
        }
        if intents.len() > self.config.max_intents {
            return Err(Rejection::WorkLimit);
        }
        // Validate all host samples before mutating any state.
        let mut samples = BTreeMap::new();
        for t in self.tissues.values().filter(|t| t.health > 0) {
            let e = env.exposure(t);
            if e.nutrients > 1000 || e.water > 1000 || e.light > 1000 {
                return Err(Rejection::Invalid);
            }
            samples.insert(t.id, e);
        }
        self.tick += 1;
        self.ledger = Ledger {
            tick: self.tick,
            ..Ledger::default()
        };
        self.normalize();
        for group in self.components() {
            let mut pool = self.total(&group);
            let mut capacity = [0; 3];
            for id in &group {
                add(&mut capacity, self.capacity(*id));
            }
            for id in &group {
                let t = &self.tissues[id];
                let e = samples[id];
                let mut intake = [0; 3];
                if t.role == Role::Root && e.dirt_contact {
                    for (i, fraction) in [e.nutrients, e.water].into_iter().enumerate() {
                        intake[i] = self.config.root_intake_per_size[i]
                            * u64::from(t.size)
                            * u64::from(fraction)
                            / 1000;
                    }
                }
                if t.role == Role::Shoot {
                    intake[2] =
                        self.config.shoot_energy_per_size * u64::from(t.size) * u64::from(e.light)
                            / 1000;
                }
                if t.role == Role::Root {
                    let accepted =
                        std::array::from_fn(|i| intake[i].min(capacity[i].saturating_sub(pool[i])));
                    self.ledger.roots.insert(
                        *id,
                        RootIntake {
                            zero: std::array::from_fn(|i| {
                                if !e.dirt_contact {
                                    Some(UptakeZero::NoContact)
                                } else if intake[i] == 0 {
                                    Some(UptakeZero::SupplyOrRounding)
                                } else if accepted[i] == 0 {
                                    Some(UptakeZero::Capacity)
                                } else {
                                    None
                                }
                            }),
                            eligible: e.dirt_contact,
                            offered: intake,
                            accepted,
                            overflow: std::array::from_fn(|i| intake[i] - accepted[i]),
                        },
                    );
                }
                add(&mut pool, intake);
                add(&mut self.ledger.intake, intake);
            }
            // Intake is bounded by current capacity BEFORE paying maintenance.
            self.distribute(&group, pool);
            pool = self.total(&group);
            let mut order = group.clone();
            order.sort_by_key(|id| (std::cmp::Reverse(self.tissues[id].allocation), *id));
            for id in order {
                let t = self.tissues.get_mut(&id).unwrap();
                let cost = scaled(self.config.maintenance_per_size[t.role.index()], t.size);
                let mut shortage = false;
                for i in 0..3 {
                    let paid = cost[i].min(pool[i]);
                    pool[i] -= paid;
                    self.ledger.maintenance[i] += paid;
                    self.ledger.unmet[i] += cost[i] - paid;
                    shortage |= paid < cost[i];
                }
                t.shortage_ticks = if shortage {
                    t.shortage_ticks.saturating_add(1)
                } else {
                    0
                };
                let shade =
                    t.role == Role::Shoot && samples[&id].light < self.config.local_light_threshold;
                t.shade_ticks = if shade {
                    t.shade_ticks.saturating_add(1)
                } else {
                    0
                };
                let old_health = t.health;
                if t.shortage_ticks > self.config.grace_ticks
                    || t.shade_ticks > self.config.grace_ticks
                {
                    t.health = t.health.saturating_sub(self.config.damage_per_tick);
                } else if !shortage && !shade {
                    t.health = (t.health + self.config.recovery_per_tick).min(100);
                }
                let dying = t.health == 0;
                if dying {
                    t.health = old_health;
                }
                self.ledger.waiting.insert(
                    id,
                    if dying {
                        WaitReason::Dead
                    } else if shade {
                        WaitReason::Shade
                    } else if shortage {
                        WaitReason::Shortage
                    } else {
                        WaitReason::Idle
                    },
                );
            }
            // Shares at the moment of death are lost; surviving shares are
            // retained, then re-pooled only over the remaining live connections.
            // Use pre-death capacities to avoid moving a dying bridge's reserves.
            self.distribute(&group, pool);
            for id in &group {
                if self.ledger.waiting.get(id) == Some(&WaitReason::Dead) {
                    let t = self.tissues.get_mut(id).unwrap();
                    t.health = 0;
                    add(&mut self.ledger.loss, t.stores);
                    t.stores = [0; 3];
                }
            }
        }
        self.normalize();
        for intent in intents {
            let result = self.apply(intent, env);
            self.ledger.results.push(result);
        }
        for t in self.tissues.values().filter(|t| t.health == 0) {
            self.ledger.waiting.insert(t.id, WaitReason::Dead);
        }
        Ok(())
    }
    /// External damage/topology events occur between ticks. Their loss is
    /// inspectable immediately in the current ledger, before its next reset.
    /// Cutting/crushing reduces health; a killed tissue loses its canonical share.
    /// Dead cells remain occupied until the host explicitly removes them.
    pub fn damage(&mut self, id: TissueId, amount: u16) -> Result<(), Rejection> {
        if amount > 100 || !self.tissues.contains_key(&id) {
            return Err(Rejection::Invalid);
        }
        if amount == 0 {
            return Ok(());
        }
        self.normalize();
        let t = self.tissues.get_mut(&id).unwrap();
        t.health = t.health.saturating_sub(amount);
        if t.health == 0 {
            add(&mut self.ledger.loss, t.stores);
            t.stores = [0; 3];
        }
        self.normalize();
        Ok(())
    }
    pub fn sever(&mut self, a: u64, b: u64) -> Result<(), Rejection> {
        if !self.edges.contains(&edge(a, b)) {
            return Err(Rejection::Invalid);
        }
        self.normalize();
        self.edges.remove(&edge(a, b));
        self.normalize();
        Ok(())
    }
    pub fn remove(&mut self, id: u64) -> Result<(), Rejection> {
        if !self.tissues.contains_key(&id) {
            return Err(Rejection::Invalid);
        }
        self.normalize();
        let t = self.tissues.remove(&id).unwrap();
        add(&mut self.ledger.loss, t.stores);
        self.edges.retain(|(a, b)| *a != id && *b != id);
        self.normalize();
        Ok(())
    }
    pub fn resize_storage(&mut self, id: u64, size: u16) -> Result<(), Rejection> {
        if size == 0 || size > self.config.max_size {
            return Err(Rejection::Invalid);
        }
        let t = self.tissues.get(&id).ok_or(Rejection::Invalid)?;
        // Shrink only: growing biomass must use a paid growth intent.
        if size > t.size || t.health == 0 {
            return Err(Rejection::Invalid);
        }
        self.normalize();
        self.tissues.get_mut(&id).unwrap().size = size;
        self.normalize();
        Ok(())
    }
}
#[cfg(test)]
mod tests;
