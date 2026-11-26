//! A tree's growth policy. Every growth point of the tree bids at every
//! decision, whether or not the tree can afford it. The world measures each
//! point's neighboring cells first, checks the winning bids and turns them
//! into ordinary runtime intents. A brain sees one point's `View` at a time,
//! so a trained model can stand in for the hand-written `Conifer`. See
//! `docs/plants.md`.
use super::*;
use std::cmp::Reverse;

/// Tree policy version 1.
pub const POLICY: u64 = 0x7472_6565_0000_0001;
/// The most empty cells a host measures light at per decision: one lighting
/// query batch.
pub const MAX_PROBES: usize = crate::lighting::MAX_RECEIVERS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Traits {
    pub identity: u64,
    /// One decision per this many fixed ticks; physiology still runs.
    pub decision_interval_ticks: u16,
    /// Winning bids that become intents at each decision.
    pub bids_per_decision: usize,
    /// The tree's own living and dead tissue on its support.
    pub max_tissues: usize,
    pub root_size: u16,
    pub wood_size: u16,
    pub needle_size: u16,
    /// Foliage, the summed sizes of living needles, that each size of wood
    /// supplies beyond it. As in the pipe model, a stem's cross-section
    /// follows the foliage it feeds, so wood must thicken before needles
    /// beyond it grow or mature further.
    pub foliage_per_wood_size: u16,
    /// Thousandths of the light crossing a cell of wood, or of needles, that
    /// gets through. By default a branch takes a quarter and a cluster of
    /// needles two fifths.
    pub wood_transmits: u16,
    pub needle_transmits: u16,
    /// The planted seed's finite reserves.
    pub starting_reserves: Resources,
}
impl Default for Traits {
    fn default() -> Self {
        Self {
            identity: 1,
            decision_interval_ticks: 1,
            bids_per_decision: 4,
            max_tissues: 160,
            root_size: 2,
            wood_size: 1,
            needle_size: 1,
            foliage_per_wood_size: 6,
            wood_transmits: 750,
            needle_transmits: 600,
            starting_reserves: [40; 3],
        }
    }
}
impl Traits {
    /// The size the tree grows a new tissue of this role at.
    fn size(&self, role: Role) -> u16 {
        match role {
            Role::Root => self.root_size,
            Role::Wood => self.wood_size,
            _ => self.needle_size,
        }
    }
}

/// What a growth point is, from where it sits in the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// The trunk's top.
    Leader,
    Trunk,
    BranchTip,
    Branch,
    RootTip,
    Root,
    Needle,
}
const KINDS: [Kind; 7] = [
    Kind::Leader,
    Kind::Trunk,
    Kind::BranchTip,
    Kind::Branch,
    Kind::RootTip,
    Kind::Root,
    Kind::Needle,
];

/// What already holds a cell: this tree's tissue by role, or another plant's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    Wood,
    Needle,
    Root,
    Other,
}

/// The whole tree, as every point's view shares it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Whole {
    /// Living tissue's stores and their capacity.
    pub stores: Resources,
    pub capacity: Resources,
    /// Intake this tick's exposures offer, and the upkeep its living tissue owes.
    pub intake: Resources,
    pub upkeep: Resources,
    /// Living tissue short of resources, and living needles in shade.
    pub short: u16,
    pub shaded: u16,
    /// Living needles and their summed sizes, living wood and roots, and dead
    /// tissue of every role.
    pub needles: u16,
    pub foliage: u16,
    pub wood: u16,
    pub roots: u16,
    pub dead: u16,
    pub max_tissues: u16,
    /// The highest wood above the origin, and the deepest root below it.
    pub height: i32,
    pub depth: i32,
}

/// One growth point: a tissue of the tree, living or dead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Point {
    pub tissue: TissueId,
    pub kind: Kind,
    pub alive: bool,
    /// Whether it grew from another tissue, and so can be shed.
    pub attached: bool,
    /// Cells along the tree's up from its origin, and across from its axis.
    pub height: i32,
    pub across: i32,
    pub size: u16,
    pub health: u16,
    pub shade_ticks: u32,
    pub shortage_ticks: u32,
    /// A living needle's light this tick; `None` for every other point.
    pub light: Option<u16>,
    /// Living needles, their foliage and dead tissue from this point outward,
    /// itself included.
    pub needles_beyond: u16,
    pub foliage_beyond: u16,
    pub dead_beyond: u16,
    /// The foliage wood supplies at its size; `None` unless wood.
    pub carry: Option<u16>,
    /// The least more foliage any wood from the seed out to this point can
    /// supply; `None` when no wood lies on the way.
    pub headroom: Option<u16>,
    /// Paid progress toward the next size, and that size's target if it can develop.
    pub development: Resources,
    pub target: Option<Resources>,
}

/// One neighboring cell of a point, measured by the world before it bids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub cell: [i32; 2],
    /// The step in the tree's frame: along its up, and away from (1) or
    /// toward (-1) its axis.
    pub along: i8,
    pub outward: i8,
    pub holder: Option<Holder>,
    /// Whether the world would place wood, a needle or a root here now,
    /// before cost.
    pub open: [bool; 3],
    /// Light measured at the empty cell, if the host probed it.
    pub light: Option<u16>,
    /// Nutrient and water supply fractions a root would take here, if dirt.
    pub soil: Option<[u16; 2]>,
}

/// Everything a brain sees when one point bids. Candidates are the point's
/// four neighbors in the tree's frame: up, down, and either side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub whole: Whole,
    pub point: Point,
    pub candidates: [Candidate; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Wait,
    /// Grow wood, a needle or a root into the best candidate the world accepts.
    Grow(Role),
    /// Pay toward the point's next size: wood thickens, needles mature.
    Thicken,
    /// Drop the point and everything beyond it.
    Shed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bid {
    pub action: Action,
    /// Higher bids are taken first; ties go to the lower tissue ID.
    pub priority: i16,
    /// Candidate indices in the order to try; the world takes the first it accepts.
    pub ranking: Vec<usize>,
}
impl Bid {
    pub fn wait() -> Self {
        Self {
            action: Action::Wait,
            priority: 0,
            ranking: Vec::new(),
        }
    }
}

/// Why the world turned a bid down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Dead,
    /// Needles grow nothing, roots grow roots, only the seed starts a trunk,
    /// and wood grows wood and needles.
    Role,
    Population,
    /// No ranked candidate is open, or another bid claimed it first.
    Space,
    /// Wood on the way already supplies all the foliage its size allows.
    Load,
    Size,
    Shade,
    Shortage,
    /// A tissue that grew from nothing cannot be shed.
    Seed,
    /// Shed earlier in this decision.
    Shed,
    /// The tree's connected stores cannot pay for it after the bids before it.
    Resource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Waited,
    /// The bid became an intent, into the given candidate when it grows.
    Won(Option<usize>),
    /// Higher bids spent the decision's budget first.
    Outbid,
    Refused(Refusal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub view: View,
    pub bid: Bid,
    pub outcome: Outcome,
}

/// One decision: its intents, and every point's view, bid and outcome in
/// tissue order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Round {
    pub waiting: Option<Limit>,
    pub intents: Vec<Intent>,
    pub records: Vec<Record>,
}

/// What the host measures at empty cells for the tree's candidates.
pub trait Senses {
    /// Light at an empty cell this tick, if the host probed it.
    fn light(&self, cell: [i32; 2]) -> Option<u16>;
    /// Nutrient and water supply fractions a root would take at this cell,
    /// if it is eligible dirt.
    fn soil(&self, cell: [i32; 2]) -> Option<[u16; 2]>;
}

/// Decides one point's bid from its view alone.
pub trait TreeBrain {
    fn bid(&mut self, view: &View) -> Bid;
}

fn slot(role: Role) -> Option<usize> {
    match role {
        Role::Wood => Some(0),
        Role::Shoot => Some(1),
        Role::Root => Some(2),
        Role::Runner | Role::Moss => None,
    }
}

fn step(cell: [i32; 2], delta: [i32; 2]) -> Option<[i32; 2]> {
    Some([
        cell[0].checked_add(delta[0])?,
        cell[1].checked_add(delta[1])?,
    ])
}

fn count(n: usize) -> u16 {
    n.min(usize::from(u16::MAX)) as u16
}

/// The tree's own tissue as it grew. Growth joins each new, higher ID to its
/// parent, so a pass in descending ID order meets every child before its
/// parent.
struct Shape<'a> {
    tissues: BTreeMap<TissueId, &'a Tissue>,
    parent: BTreeMap<TissueId, TissueId>,
    children: BTreeMap<TissueId, Vec<TissueId>>,
    /// Living needles, their foliage and dead tissue from each tissue
    /// outward, itself included.
    beyond: BTreeMap<TissueId, [u16; 3]>,
}
impl Shape<'_> {
    /// A tissue and the tissues it grew from, back to the seed.
    fn path(&self, id: TissueId) -> impl Iterator<Item = TissueId> + '_ {
        std::iter::successors(Some(id), |id| self.parent.get(id).copied())
    }

    /// A tissue and everything that grew from it.
    fn subtree(&self, id: TissueId) -> Vec<TissueId> {
        let mut all = vec![id];
        let mut cursor = 0;
        while cursor < all.len() {
            all.extend(self.children.get(&all[cursor]).into_iter().flatten());
            cursor += 1;
        }
        all
    }

    /// The living tissue a living tissue's pooled stores are keyed by: the
    /// first of its component, where living connections end at dead tissue.
    fn component(&self, id: TissueId) -> TissueId {
        self.path(id)
            .take_while(|id| self.tissues[id].health > 0)
            .last()
            .unwrap_or(id)
    }
}

/// What one decision has committed so far.
struct Decision {
    budget: usize,
    population: usize,
    total: usize,
    claimed: BTreeSet<[i32; 2]>,
    gone: BTreeSet<TissueId>,
    pools: BTreeMap<TissueId, Resources>,
    spent: BTreeMap<TissueId, Resources>,
    /// Foliage this decision grows beyond each wood tissue.
    added: BTreeMap<TissueId, u16>,
    intents: Vec<Intent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    pub traits: Traits,
    /// The seed's cell in the support's local frame.
    pub origin: [i32; 2],
    /// Cardinal biological up in the support's local cell frame, from host gravity.
    pub up: [i32; 2],
    pub support: Support,
}
impl Tree {
    pub fn new(
        traits: Traits,
        origin: [i32; 2],
        up: [i32; 2],
        support: Support,
    ) -> Result<Self, Rejection> {
        if ![[1, 0], [-1, 0], [0, 1], [0, -1]].contains(&up)
            || traits.decision_interval_ticks == 0
            || traits.bids_per_decision == 0
            || traits.max_tissues == 0
            || traits.max_tissues > 1024
            || traits.foliage_per_wood_size == 0
            || [traits.root_size, traits.wood_size, traits.needle_size].contains(&0)
            || traits.wood_transmits >= 1000
            || traits.needle_transmits >= 1000
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

    /// Deliberate planting is a host action, never invoked by the growth
    /// policy: one root at the origin holding the seed's reserves.
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

    fn side(&self) -> [i32; 2] {
        [self.up[1], -self.up[0]]
    }

    /// Up, down, and the two sides, in the support's local frame.
    fn steps(&self) -> [[i32; 2]; 4] {
        let side = self.side();
        [self.up, self.up.map(|v| -v), side, side.map(|v| -v)]
    }

    /// A cell's height along up from the origin, and its offset across.
    fn frame(&self, cell: [i32; 2]) -> [i32; 2] {
        let d = [0, 1].map(|i| i64::from(cell[i]) - i64::from(self.origin[i]));
        let side = self.side();
        let dot = |axis: [i32; 2]| {
            (d[0] * i64::from(axis[0]) + d[1] * i64::from(axis[1]))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
        };
        [dot(self.up), dot(side)]
    }

    /// The share of the light crossing a tissue that gets through, for the
    /// host's light publication. The tree's wood and needles let some
    /// through; anything else, another plant's tissue included, none.
    pub fn transmits(&self, t: &Tissue) -> f32 {
        if t.policy != POLICY || t.support != Some(self.support) {
            return 0.0;
        }
        let thousandths = match t.role {
            Role::Wood => self.traits.wood_transmits,
            Role::Shoot => self.traits.needle_transmits,
            _ => 0,
        };
        f32::from(thousandths) / 1000.0
    }

    fn own<'a>(&self, o: &'a Observation) -> impl Iterator<Item = &'a Tissue> {
        let support = Some(self.support);
        o.tissues.iter().filter(move |t| {
            t.policy == POLICY
                && t.support == support
                && matches!(t.role, Role::Root | Role::Wood | Role::Shoot)
        })
    }

    fn shape<'a>(&self, o: &'a Observation) -> Shape<'a> {
        let tissues: BTreeMap<TissueId, &Tissue> = self.own(o).map(|t| (t.id, t)).collect();
        let mut parent = BTreeMap::new();
        let mut children: BTreeMap<TissueId, Vec<TissueId>> = BTreeMap::new();
        for &(a, b) in &o.connections {
            if tissues.contains_key(&a) && tissues.contains_key(&b) {
                parent.insert(b, a);
                children.entry(a).or_default().push(b);
            }
        }
        let mut beyond: BTreeMap<TissueId, [u16; 3]> = BTreeMap::new();
        for t in tissues.values().rev() {
            let needle = t.health > 0 && t.role == Role::Shoot;
            let mut sum = [
                u16::from(needle),
                if needle { t.size } else { 0 },
                u16::from(t.health == 0),
            ];
            for child in children.get(&t.id).into_iter().flatten() {
                let c = beyond[child];
                sum = std::array::from_fn(|i| sum[i].saturating_add(c[i]));
            }
            beyond.insert(t.id, sum);
        }
        Shape {
            tissues,
            parent,
            children,
            beyond,
        }
    }

    fn carry(&self, t: &Tissue) -> Option<u16> {
        (t.role == Role::Wood).then(|| t.size.saturating_mul(self.traits.foliage_per_wood_size))
    }

    /// The least more foliage the wood from the seed out to a tissue can
    /// supply, counting the foliage this decision has already grown.
    fn headroom(
        &self,
        shape: &Shape,
        id: TissueId,
        added: &BTreeMap<TissueId, u16>,
    ) -> Option<u16> {
        shape
            .path(id)
            .filter_map(|id| {
                let carry = self.carry(shape.tissues[&id])?;
                let load =
                    shape.beyond[&id][1].saturating_add(added.get(&id).copied().unwrap_or(0));
                Some(carry.saturating_sub(load))
            })
            .min()
    }

    /// Empty air cells beside the tree's living wood and roots, to measure
    /// light at before planning. Beyond `MAX_PROBES` the set rotates with the
    /// tick, so every cell is measured within a few decisions.
    pub fn probes(&self, o: &Observation, env: &impl Environment) -> Vec<[i32; 2]> {
        let occupied: BTreeSet<[i32; 2]> = o.tissues.iter().map(|t| t.cell).collect();
        let mut cells = BTreeSet::new();
        for t in self
            .own(o)
            .filter(|t| t.health > 0 && t.role != Role::Shoot)
        {
            for delta in self.steps() {
                let Some(cell) = step(t.cell, delta) else {
                    continue;
                };
                if !occupied.contains(&cell)
                    && env.support_valid(Some(self.support), cell)
                    && env.space_available(cell, Role::Shoot, self.traits.needle_size)
                {
                    cells.insert(cell);
                }
            }
        }
        let cells: Vec<_> = cells.into_iter().collect();
        if cells.len() <= MAX_PROBES {
            return cells;
        }
        let start = (o.tick as usize).wrapping_mul(MAX_PROBES) % cells.len();
        let mut chosen: Vec<_> = (0..MAX_PROBES)
            .map(|i| cells[(start + i) % cells.len()])
            .collect();
        chosen.sort_unstable();
        chosen
    }

    /// Every point's view, in tissue order.
    pub fn views(
        &self,
        o: &Observation,
        env: &impl Environment,
        senses: &impl Senses,
    ) -> Vec<View> {
        self.survey(o, env, senses).1
    }

    fn survey<'a>(
        &self,
        o: &'a Observation,
        env: &impl Environment,
        senses: &impl Senses,
    ) -> (Shape<'a>, Vec<View>) {
        let shape = self.shape(o);
        let exposure: BTreeMap<TissueId, Exposure> = shape
            .tissues
            .values()
            .filter(|t| t.health > 0)
            .map(|t| (t.id, env.exposure(t)))
            .collect();
        let whole = self.whole(o, &shape, &exposure);
        let holders: BTreeMap<[i32; 2], Holder> = o
            .tissues
            .iter()
            .map(|t| {
                let holder = match (shape.tissues.contains_key(&t.id), t.role) {
                    (true, Role::Wood) => Holder::Wood,
                    (true, Role::Shoot) => Holder::Needle,
                    (true, Role::Root) => Holder::Root,
                    _ => Holder::Other,
                };
                (t.cell, holder)
            })
            .collect();
        let none = BTreeMap::new();
        let views = shape
            .tissues
            .values()
            .map(|t| {
                let [height, across] = self.frame(t.cell);
                let grew = |role: Role, on_axis: bool| {
                    shape.children.get(&t.id).into_iter().flatten().any(|c| {
                        let child = shape.tissues[c];
                        child.role == role && (!on_axis || self.frame(child.cell)[1] == 0)
                    })
                };
                let kind = match t.role {
                    Role::Wood if across == 0 && !grew(Role::Wood, true) => Kind::Leader,
                    Role::Wood if across == 0 => Kind::Trunk,
                    Role::Wood if !grew(Role::Wood, false) => Kind::BranchTip,
                    Role::Wood => Kind::Branch,
                    Role::Root if !grew(Role::Root, false) => Kind::RootTip,
                    Role::Root => Kind::Root,
                    _ => Kind::Needle,
                };
                let light = (t.health > 0 && t.role == Role::Shoot)
                    .then(|| exposure[&t.id].light)
                    .filter(|light| *light <= 1000);
                let [needles_beyond, foliage_beyond, dead_beyond] = shape.beyond[&t.id];
                let point = Point {
                    tissue: t.id,
                    kind,
                    alive: t.health > 0,
                    attached: shape.parent.contains_key(&t.id),
                    height,
                    across,
                    size: t.size,
                    health: t.health,
                    shade_ticks: t.shade_ticks,
                    shortage_ticks: t.shortage_ticks,
                    light,
                    needles_beyond,
                    foliage_beyond,
                    dead_beyond,
                    carry: self.carry(t),
                    headroom: self.headroom(&shape, t.id, &none),
                    development: t.development,
                    target: o.config.development_target(t.role, t.size),
                };
                let candidates = self.steps().map(|delta| {
                    let Some(cell) = step(t.cell, delta) else {
                        return Candidate {
                            cell: t.cell,
                            along: 0,
                            outward: 0,
                            holder: None,
                            open: [false; 3],
                            light: None,
                            soil: None,
                        };
                    };
                    let [h, a] = self.frame(cell);
                    let holder = holders.get(&cell).copied();
                    let open = [Role::Wood, Role::Shoot, Role::Root].map(|role| {
                        holder.is_none()
                            && env.support_valid(Some(self.support), cell)
                            && env.space_available(cell, role, self.traits.size(role))
                    });
                    Candidate {
                        cell,
                        along: (h - height).signum() as i8,
                        outward: (a.abs() - across.abs()).signum() as i8,
                        holder,
                        open,
                        light: holder.is_none().then(|| senses.light(cell)).flatten(),
                        soil: holder.is_none().then(|| senses.soil(cell)).flatten(),
                    }
                });
                View {
                    whole: whole.clone(),
                    point,
                    candidates,
                }
            })
            .collect();
        (shape, views)
    }

    fn whole(
        &self,
        o: &Observation,
        shape: &Shape,
        exposure: &BTreeMap<TissueId, Exposure>,
    ) -> Whole {
        let c = &o.config;
        let mut whole = Whole {
            max_tissues: count(self.traits.max_tissues),
            ..Whole::default()
        };
        for t in shape.tissues.values() {
            if t.health == 0 {
                whole.dead = whole.dead.saturating_add(1);
                continue;
            }
            let [height, _] = self.frame(t.cell);
            let size = u64::from(t.size);
            add(&mut whole.stores, t.stores);
            add(
                &mut whole.capacity,
                scaled(c.capacity_per_size[t.role.index()], t.size),
            );
            add(
                &mut whole.upkeep,
                scaled(c.maintenance_per_size[t.role.index()], t.size),
            );
            let e = exposure[&t.id];
            // The runtime's own intake rules, from this tick's exposures.
            match t.role {
                Role::Root if e.dirt_contact => {
                    for (i, fraction) in [e.nutrients, e.water].into_iter().enumerate() {
                        whole.intake[i] +=
                            c.root_intake_per_size[i] * size * u64::from(fraction.min(1000)) / 1000;
                    }
                }
                Role::Shoot => {
                    whole.intake[2] +=
                        c.shoot_energy_per_size * size * u64::from(e.light.min(1000)) / 1000;
                }
                _ => {}
            }
            whole.short = whole.short.saturating_add(u16::from(t.shortage_ticks > 0));
            match t.role {
                Role::Shoot => {
                    whole.needles = whole.needles.saturating_add(1);
                    whole.foliage = whole.foliage.saturating_add(t.size);
                    whole.shaded = whole.shaded.saturating_add(u16::from(t.shade_ticks > 0));
                }
                Role::Wood => {
                    whole.wood = whole.wood.saturating_add(1);
                    whole.height = whole.height.max(height);
                }
                _ => {
                    whole.roots = whole.roots.saturating_add(1);
                    whole.depth = whole.depth.max(-height);
                }
            }
        }
        whole
    }

    /// Ask every point for its bid, then take the bids from the highest down:
    /// the world refuses what it cannot place or the tree cannot afford, and
    /// turns the rest into intents until the decision's budget is spent.
    /// Costs are advisory; runtime admission after upkeep is final.
    pub fn plan(
        &self,
        o: &Observation,
        env: &impl Environment,
        senses: &impl Senses,
        brain: &mut impl TreeBrain,
    ) -> Round {
        if !o
            .tick
            .is_multiple_of(u64::from(self.traits.decision_interval_ticks))
        {
            return Round {
                waiting: Some(Limit::Cadence),
                ..Round::default()
            };
        }
        let (shape, views) = self.survey(o, env, senses);
        let bids: Vec<Bid> = views.iter().map(|view| brain.bid(view)).collect();
        let mut order: Vec<usize> = (0..views.len()).collect();
        order.sort_by_key(|&i| (Reverse(bids[i].priority), views[i].point.tissue));
        let mut d = Decision {
            budget: self.traits.bids_per_decision.min(o.config.max_intents),
            population: views.len(),
            total: o.tissues.len(),
            claimed: BTreeSet::new(),
            gone: BTreeSet::new(),
            pools: component_pools(o),
            spent: BTreeMap::new(),
            added: BTreeMap::new(),
            intents: Vec::new(),
        };
        let mut outcomes = vec![Outcome::Waited; views.len()];
        for i in order {
            outcomes[i] = self.resolve(o, env, &shape, &views[i], &bids[i], &mut d);
        }
        Round {
            waiting: None,
            intents: d.intents,
            records: views
                .into_iter()
                .zip(bids)
                .zip(outcomes)
                .map(|((view, bid), outcome)| Record { view, bid, outcome })
                .collect(),
        }
    }

    fn resolve(
        &self,
        o: &Observation,
        env: &impl Environment,
        shape: &Shape,
        view: &View,
        bid: &Bid,
        d: &mut Decision,
    ) -> Outcome {
        let p = &view.point;
        if bid.action == Action::Wait {
            return Outcome::Waited;
        }
        if d.budget == 0 {
            return Outcome::Outbid;
        }
        if d.gone.contains(&p.tissue) {
            return Outcome::Refused(Refusal::Shed);
        }
        if bid.action == Action::Shed {
            if !p.attached {
                return Outcome::Refused(Refusal::Seed);
            }
            d.gone.extend(shape.subtree(p.tissue));
            d.intents.push(Intent::Shed { tissue: p.tissue });
            d.budget -= 1;
            return Outcome::Won(None);
        }
        if !p.alive {
            return Outcome::Refused(Refusal::Dead);
        }
        let proposal = match bid.action {
            Action::Grow(role) => self.grow(o, view, bid, role, d),
            _ => self.thicken(env, shape.tissues[&p.tissue], p),
        };
        let (cost, intent) = match proposal {
            Ok(proposal) => proposal,
            Err(refusal) => return Outcome::Refused(refusal),
        };
        // New and maturing needles add foliage the wood behind them supplies.
        let foliage = match intent {
            Intent::Grow {
                role: Role::Shoot,
                size,
                ..
            } => size,
            Intent::Develop { .. } if p.kind == Kind::Needle => 1,
            _ => 0,
        };
        if foliage > 0
            && self
                .headroom(shape, p.tissue, &d.added)
                .is_some_and(|room| room < foliage)
        {
            return Outcome::Refused(Refusal::Load);
        }
        let group = shape.component(p.tissue);
        let spent = d.spent.get(&group).copied().unwrap_or_default();
        if (0..3).any(|r| d.pools[&p.tissue][r].saturating_sub(spent[r]) < cost[r]) {
            return Outcome::Refused(Refusal::Resource);
        }
        add(d.spent.entry(group).or_default(), cost);
        d.budget -= 1;
        for id in shape.path(p.tissue) {
            if foliage > 0 && shape.tissues[&id].role == Role::Wood {
                let added = d.added.entry(id).or_default();
                *added = added.saturating_add(foliage);
            }
        }
        let mut chosen = None;
        if let Intent::Grow { cell, .. } = intent {
            d.claimed.insert(cell);
            d.population += 1;
            d.total += 1;
            chosen = view.candidates.iter().position(|c| c.cell == cell);
        }
        d.intents.push(intent);
        Outcome::Won(chosen)
    }

    fn grow(
        &self,
        o: &Observation,
        view: &View,
        bid: &Bid,
        role: Role,
        d: &Decision,
    ) -> Result<(Resources, Intent), Refusal> {
        let p = &view.point;
        let slot = slot(role).ok_or(Refusal::Role)?;
        let allowed = match p.kind {
            Kind::Needle => false,
            Kind::Root | Kind::RootTip => role == Role::Root || (role == Role::Wood && !p.attached),
            Kind::Leader | Kind::Trunk | Kind::BranchTip | Kind::Branch => role != Role::Root,
        };
        if !allowed {
            return Err(Refusal::Role);
        }
        if d.population >= self.traits.max_tissues || d.total >= o.config.max_tissues {
            return Err(Refusal::Population);
        }
        let size = self.traits.size(role);
        if size > o.config.max_size {
            return Err(Refusal::Size);
        }
        let cell = bid
            .ranking
            .iter()
            .filter_map(|&i| view.candidates.get(i))
            .find(|c| c.open[slot] && !d.claimed.contains(&c.cell))
            .ok_or(Refusal::Space)?
            .cell;
        Ok((
            scaled(o.config.growth_per_size[role.index()], size),
            Intent::Grow {
                parent: p.tissue,
                role,
                size,
                cell,
                support: Some(self.support),
            },
        ))
    }

    fn thicken(
        &self,
        env: &impl Environment,
        t: &Tissue,
        p: &Point,
    ) -> Result<(Resources, Intent), Refusal> {
        let target = p.target.ok_or(Refusal::Size)?;
        if p.shade_ticks > 0 {
            return Err(Refusal::Shade);
        }
        if p.shortage_ticks > 0 {
            return Err(Refusal::Shortage);
        }
        if !env.space_available(t.cell, t.role, t.size.saturating_add(1)) {
            return Err(Refusal::Space);
        }
        Ok((
            std::array::from_fn(|i| u64::from(p.development[i] < target[i])),
            Intent::Develop { tissue: p.tissue },
        ))
    }
}

/// The hand-written conifer brain. The leader climbs while energy keeps up,
/// branches come out in tiers and reach further the lower they sit, needles
/// go where the light is and mature there, roots grow when water or
/// nutrients run short, wood thickens as the needles beyond it fill what it
/// supplies, and shaded needles, dead tissue and bare tips are dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conifer {
    /// Height of the lowest branch tier, and the cells between tiers.
    pub first_tier: i32,
    pub tier_spacing: i32,
    /// The leader stops climbing at this height, and roots at this depth.
    pub max_height: i32,
    pub max_depth: i32,
    /// The least light worth growing a needle into.
    pub needle_light: u16,
    /// Shade ticks a needle is kept before it is dropped.
    pub needle_shade_ticks: u32,
}
impl Default for Conifer {
    fn default() -> Self {
        Self {
            first_tier: 3,
            tier_spacing: 2,
            max_height: 14,
            max_depth: 6,
            needle_light: 400,
            needle_shade_ticks: 5,
        }
    }
}

const UP: usize = 0;
const DOWN: usize = 1;
const SIDES: [usize; 2] = [2, 3];
/// A needle's full size: the runtime matures shoots to size three.
const MATURE: u16 = 3;

fn grow(role: Role, priority: i16, ranking: Vec<usize>) -> Bid {
    Bid {
        action: Action::Grow(role),
        priority,
        ranking,
    }
}

fn act(action: Action, priority: i16) -> Bid {
    Bid {
        action,
        priority,
        ranking: Vec::new(),
    }
}

fn best(options: Vec<Bid>) -> Bid {
    options
        .into_iter()
        .max_by_key(|bid| bid.priority)
        .unwrap_or_else(Bid::wait)
}

impl Conifer {
    /// A resource runs short when this tick's intake would not cover upkeep
    /// with an eighth and two to spare.
    fn short(w: &Whole, r: usize) -> bool {
        w.intake[r] < w.upkeep[r] + w.upkeep[r] / 8 + 2
    }

    /// Energy intake less upkeep this tick.
    fn net(w: &Whole) -> i64 {
        w.intake[2] as i64 - w.upkeep[2] as i64
    }

    /// The surplus the tree keeps before it builds wood it will not need yet.
    fn margin(w: &Whole) -> i64 {
        6 + w.upkeep[2] as i64 / 4
    }

    /// Open candidates for this role, brightest first.
    fn by_light(view: &View, role: Role, picks: &[usize], least: Option<u16>) -> Vec<usize> {
        let slot = slot(role).unwrap_or(0);
        let mut ranked: Vec<usize> = picks
            .iter()
            .copied()
            .filter(|&i| {
                let c = &view.candidates[i];
                c.open[slot] && least.is_none_or(|least| c.light.is_some_and(|l| l >= least))
            })
            .collect();
        ranked.sort_by_key(|&i| Reverse(view.candidates[i].light.unwrap_or(0)));
        ranked
    }

    /// A needle in the brightest open candidate lit enough, while the wood
    /// behind the point can supply more foliage.
    fn needle(&self, view: &View, picks: &[usize]) -> Option<Bid> {
        if view.point.headroom == Some(0) {
            return None;
        }
        let ranking = Self::by_light(view, Role::Shoot, picks, Some(self.needle_light));
        let light = view.candidates[*ranking.first()?].light.unwrap_or(0);
        let need = if Self::short(&view.whole, 2) { 200 } else { 0 };
        Some(grow(
            Role::Shoot,
            300 + need + (light.min(1000) / 10) as i16,
            ranking,
        ))
    }

    /// Wood thickens once the foliage beyond it fills what it supplies.
    fn thicken(p: &Point) -> Option<Bid> {
        (p.target.is_some() && p.foliage_beyond >= p.carry?).then(|| act(Action::Thicken, 450))
    }

    fn root(&self, view: &View) -> Bid {
        let (w, p) = (&view.whole, &view.point);
        // The seed starts the trunk straight up.
        if !p.attached && w.wood == 0 {
            return grow(Role::Wood, 1000, vec![UP]);
        }
        let plenty = |r: usize| w.intake[r] >= 2 * (w.upkeep[r] + 2);
        if p.kind == Kind::RootTip
            && p.attached
            && plenty(0)
            && plenty(1)
            && Self::net(w) < Self::margin(w)
        {
            // Roots cost energy they never return: with water and nutrients
            // to spare and energy tight, the tree drops a root tip.
            return act(Action::Shed, 150);
        }
        if !Self::short(w, 0) && !Self::short(w, 1) || Self::net(w) < Self::margin(w) {
            return Bid::wait();
        }
        let mut ranking: Vec<usize> = [DOWN, SIDES[0], SIDES[1]]
            .into_iter()
            .filter(|&i| {
                let c = &view.candidates[i];
                c.open[2] && p.height + i32::from(c.along) >= -self.max_depth
            })
            .collect();
        if ranking.is_empty() {
            return Bid::wait();
        }
        // Richer soil first, and down before the sides.
        ranking.sort_by_key(|&i| {
            Reverse(
                view.candidates[i]
                    .soil
                    .map_or(0, |s| u32::from(s[0]) + u32::from(s[1])),
            )
        });
        let tip = if p.kind == Kind::RootTip { 20 } else { 0 };
        grow(
            Role::Root,
            700 + tip + (-p.height).clamp(0, 20) as i16,
            ranking,
        )
    }

    fn trunk(&self, view: &View) -> Bid {
        let (w, p) = (&view.whole, &view.point);
        if w.needles == 0 {
            let ranking = Self::by_light(view, Role::Shoot, &[SIDES[0], SIDES[1], UP], None);
            return grow(Role::Shoot, 1000, ranking);
        }
        let tier = p.height >= self.first_tier
            && (p.height - self.first_tier) % self.tier_spacing.max(1) == 0;
        let mut options = Vec::new();
        if p.kind == Kind::Leader {
            if p.height >= self.max_height {
                // A full-height leader tops itself with a needle.
                options.extend(self.needle(view, &[UP]));
            } else if p.foliage_beyond >= MATURE * p.needles_beyond
                && Self::net(w) >= Self::margin(w)
            {
                // Climbing adds no light by itself, and the needles grown
                // beside the next leader shade the ones beside this one. It
                // waits for those to mature and for a surplus, and comes after
                // the branches it makes room for.
                options.push(grow(Role::Wood, 280, vec![UP]));
            }
            if !tier {
                options.extend(self.needle(view, &SIDES));
            }
        }
        // Tier heights keep their sides for branches, which sprout once the
        // leader is a tier above. Wood earns nothing, so branches grow from a
        // surplus that can keep them and the needles they carry.
        if tier && Self::net(w) >= 3 && w.height >= p.height + self.tier_spacing {
            let ranking = Self::by_light(view, Role::Wood, &SIDES, None);
            if !ranking.is_empty() {
                let lower = (self.max_height - p.height).clamp(0, 50) as i16;
                options.push(grow(Role::Wood, 330 + lower, ranking));
            }
        }
        options.extend(Self::thicken(p));
        best(options)
    }

    fn branch(&self, view: &View) -> Bid {
        let (w, p) = (&view.whole, &view.point);
        let tip = p.kind == Kind::BranchTip;
        // A tier reaches one cell, plus one for each tier spacing of trunk
        // above it.
        let reach = 1 + (w.height - p.height) / self.tier_spacing.max(1);
        let extend = (0..4).find(|&i| {
            let c = &view.candidates[i];
            tip && c.along == 0
                && c.outward == 1
                && c.open[0]
                && p.across.abs() < reach
                && c.light.is_none_or(|l| l >= self.needle_light / 2)
        });
        let mut options = Vec::new();
        if let Some(out) = extend.filter(|_| Self::net(w) >= 3) {
            // Lower tiers first, so they stay wider than the tiers above.
            let lower = (self.max_height - p.height).clamp(0, 50) as i16;
            options.push(grow(Role::Wood, 300 + lower, vec![out]));
        }
        options.extend(self.needle(view, &[UP]));
        options.extend(Self::thicken(p));
        let up = &view.candidates[UP];
        let dark = up.holder.is_some() || up.light.is_some_and(|l| l < self.needle_light);
        if options.is_empty()
            && tip
            && extend.is_none()
            && dark
            && p.needles_beyond == 0
            && p.dead_beyond == 0
            && p.across.abs() > 1
        {
            // A bare tip in shade with nowhere to reach is dropped, back to
            // the stub beside the trunk. The stub waits: shed, it would sprout
            // again into the same shade.
            return act(Action::Shed, 100);
        }
        best(options)
    }
}
impl TreeBrain for Conifer {
    fn bid(&mut self, view: &View) -> Bid {
        let (w, p) = (&view.whole, &view.point);
        if !p.alive {
            // Dead tissue holds its cell and feeds nothing.
            return if p.attached {
                act(Action::Shed, 100)
            } else {
                Bid::wait()
            };
        }
        match p.kind {
            // A needle kept in shade costs upkeep it cannot repay.
            Kind::Needle if p.shade_ticks > self.needle_shade_ticks && p.attached => {
                act(Action::Shed, 500)
            }
            Kind::Needle
                if p.target.is_some()
                    && p.shade_ticks == 0
                    && p.shortage_ticks == 0
                    && p.headroom != Some(0) =>
            {
                let need = if Self::short(w, 2) { 200 } else { 0 };
                act(Action::Thicken, 260 + need)
            }
            Kind::Needle => Bid::wait(),
            Kind::Root | Kind::RootTip => self.root(view),
            Kind::Leader | Kind::Trunk => self.trunk(view),
            Kind::Branch | Kind::BranchTip => self.branch(view),
        }
    }
}

/// Encoder version 1. A view encodes as `COMMON` features of the tree and the
/// point, then `PER_CANDIDATE` features for each of the four candidates in
/// view order. Values lie in -1..=1. A reading the world did not take encodes
/// as zero beside a zero "known" flag, never as darkness or drought.
pub const ENCODER: u32 = 1;
pub const COMMON: usize = 42;
pub const PER_CANDIDATE: usize = 15;
pub const INPUTS: usize = COMMON + 4 * PER_CANDIDATE;

pub fn encode(view: &View) -> [f32; INPUTS] {
    let (w, p) = (&view.whole, &view.point);
    let fraction = |a: u64, b: u64| {
        if b == 0 {
            0.0
        } else {
            (a as f32 / b as f32).min(1.0)
        }
    };
    let scale = |v: f32, by: f32| (v / by).clamp(-1.0, 1.0);
    let per_mille = |v: u16| f32::from(v.min(1000)) / 1000.0;
    let per_tree = |n: u16| fraction(u64::from(n), u64::from(w.max_tissues));
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    let mut x = Vec::with_capacity(INPUTS);
    // The whole tree: 16 features.
    for r in 0..3 {
        x.push(fraction(w.stores[r], w.capacity[r]));
    }
    for r in 0..3 {
        let net = w.intake[r] as f32 - w.upkeep[r] as f32;
        x.push(scale(net, w.upkeep[r].max(1) as f32));
    }
    let living = w.needles.saturating_add(w.wood).saturating_add(w.roots);
    x.push(fraction(u64::from(w.short), u64::from(living)));
    x.push(fraction(u64::from(w.shaded), u64::from(w.needles)));
    for n in [
        w.needles,
        w.wood,
        w.roots,
        w.dead,
        living.saturating_add(w.dead),
    ] {
        x.push(per_tree(n));
    }
    x.push(fraction(
        u64::from(w.foliage),
        u64::from(w.needles) * u64::from(MATURE),
    ));
    x.push(scale(w.height as f32, 32.0));
    x.push(scale(w.depth as f32, 16.0));
    // The point: 26 features.
    for kind in KINDS {
        x.push(flag(p.kind == kind));
    }
    x.push(flag(p.alive));
    x.push(flag(p.attached));
    x.push(scale(p.height as f32, 32.0));
    x.push(scale(p.across.abs() as f32, 16.0));
    x.push(scale(f32::from(p.size), 16.0));
    x.push(f32::from(p.health.min(100)) / 100.0);
    x.push(flag(p.shade_ticks > 0));
    x.push(flag(p.shortage_ticks > 0));
    x.push(flag(p.light.is_some()));
    x.push(p.light.map_or(0.0, per_mille));
    x.push(scale(f32::from(p.needles_beyond), 64.0));
    x.push(fraction(
        u64::from(p.foliage_beyond),
        u64::from(p.needles_beyond) * u64::from(MATURE),
    ));
    x.push(scale(f32::from(p.dead_beyond), 64.0));
    x.push(flag(p.carry.is_some()));
    x.push(p.carry.map_or(0.0, |carry| {
        fraction(u64::from(p.foliage_beyond), u64::from(carry))
    }));
    x.push(flag(p.headroom.is_some()));
    x.push(p.headroom.map_or(0.0, |h| scale(f32::from(h), 32.0)));
    x.push(flag(p.target.is_some()));
    x.push(p.target.map_or(0.0, |target| {
        fraction(p.development.iter().sum(), target.iter().sum())
    }));
    // Each candidate: 15 features.
    for c in &view.candidates {
        for holder in [
            None,
            Some(Holder::Wood),
            Some(Holder::Needle),
            Some(Holder::Root),
            Some(Holder::Other),
        ] {
            x.push(flag(c.holder == holder));
        }
        for open in c.open {
            x.push(flag(open));
        }
        x.push(f32::from(c.along));
        x.push(f32::from(c.outward));
        x.push(flag(c.light.is_some()));
        x.push(c.light.map_or(0.0, per_mille));
        x.push(flag(c.soil.is_some()));
        let soil = c.soil.unwrap_or([0; 2]);
        x.push(per_mille(soil[0]));
        x.push(per_mille(soil[1]));
    }
    debug_assert_eq!(x.len(), INPUTS);
    std::array::from_fn(|i| x[i])
}

#[cfg(test)]
mod tree_policy_contract;
