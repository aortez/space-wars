use super::*;

const SUPPORT: Support = Support {
    body: 3,
    generation: 1,
};
const WIDTH: i32 = 25;
const HEIGHT: i32 = 20;
const DEPTH: i32 = 8;
const ORIGIN: [i32; 2] = [12, -1];
const FULL: (u16, [u16; 2]) = (1000, [1000; 2]);

/// A flat bed WIDTH cells wide: dirt from y = -DEPTH to -1, air from 0 up to
/// HEIGHT, and light straight down. A cell is dark when any wood, blade or
/// runner lies above it in its column, as the light adapter's disks make it
/// under an overhead sun.
#[derive(Clone)]
struct Bed {
    above_ground: BTreeSet<[i32; 2]>,
    light: u16,
    soil: [u16; 2],
}
impl Bed {
    fn new(o: &Observation, (light, soil): (u16, [u16; 2])) -> Self {
        Self {
            above_ground: o
                .tissues
                .iter()
                .filter(|t| matches!(t.role, Role::Runner | Role::Shoot | Role::Wood))
                .map(|t| t.cell)
                .collect(),
            light,
            soil,
        }
    }
    fn inside(cell: [i32; 2]) -> bool {
        (0..WIDTH).contains(&cell[0]) && (-DEPTH..HEIGHT).contains(&cell[1])
    }
    fn lit(&self, cell: [i32; 2]) -> u16 {
        if self
            .above_ground
            .iter()
            .any(|c| c[0] == cell[0] && c[1] > cell[1])
        {
            0
        } else {
            self.light
        }
    }
}
impl Environment for Bed {
    fn exposure(&self, t: &Tissue) -> Exposure {
        Exposure {
            dirt_contact: t.role == Role::Root && t.cell[1] < 0,
            surface_contact: false,
            nutrients: self.soil[0],
            water: self.soil[1],
            light: if t.role == Role::Shoot {
                self.lit(t.cell)
            } else {
                0
            },
        }
    }
    fn space_available(&self, cell: [i32; 2], role: Role, _: u16) -> bool {
        Self::inside(cell)
            && match role {
                Role::Root | Role::Runner => cell[1] < 0,
                Role::Shoot | Role::Wood => cell[1] >= 0,
                Role::Moss => false,
            }
    }
    fn support_valid(&self, support: Option<Support>, cell: [i32; 2]) -> bool {
        support == Some(SUPPORT) && Self::inside(cell)
    }
}
impl Senses for Bed {
    fn light(&self, cell: [i32; 2]) -> Option<u16> {
        (Self::inside(cell) && cell[1] >= 0).then(|| self.lit(cell))
    }
    fn soil(&self, cell: [i32; 2]) -> Option<[u16; 2]> {
        (Self::inside(cell) && cell[1] < 0).then_some(self.soil)
    }
}

fn tree(traits: Traits) -> Tree {
    Tree::new(traits, ORIGIN, [0, 1], SUPPORT).unwrap()
}

fn planted(traits: Traits) -> (Plants, Tree) {
    let mut plants = Plants::new(grass::physiology(), 7).unwrap();
    let tree = tree(traits);
    let bed = Bed::new(&plants.observe(), FULL);
    tree.plant(&mut plants, &bed).unwrap();
    (plants, tree)
}

/// Rows from the top. The tree's wood is `#`, its needles `*` and its roots
/// `r`; grass blades are `,`, runners `~` and grass roots `;`; dead tissue is
/// `x` and empty dirt `:`.
fn draw(o: &Observation) -> String {
    let at: BTreeMap<[i32; 2], &Tissue> = o.tissues.iter().map(|t| (t.cell, t)).collect();
    let mut rows = Vec::new();
    for y in (-DEPTH..HEIGHT).rev() {
        rows.push(
            (0..WIDTH)
                .map(|x| match at.get(&[x, y]) {
                    Some(t) if t.health == 0 => 'x',
                    Some(t) => match (t.policy == POLICY, t.role) {
                        (true, Role::Wood) => '#',
                        (true, Role::Shoot) => '*',
                        (true, _) => 'r',
                        (false, Role::Shoot) => ',',
                        (false, Role::Runner) => '~',
                        (false, _) => ';',
                    },
                    None if y < 0 => ':',
                    None => '.',
                })
                .collect::<String>(),
        );
    }
    rows.join("\n")
}

/// The conifer alone, with the bed's light and soil set before each tick.
fn grow_under(
    ticks: u64,
    conditions: impl Fn(u64) -> (u16, [u16; 2]),
) -> (Vec<Observation>, Vec<Round>) {
    let (mut plants, tree) = planted(Traits::default());
    let mut brain = Conifer::default();
    let (mut history, mut rounds) = (Vec::new(), Vec::new());
    for _ in 0..ticks {
        let o = plants.observe();
        let bed = Bed::new(&o, conditions(o.tick));
        let round = tree.plan(&o, &bed, &bed, &mut brain);
        plants.tick(o.tick, false, 1, &round.intents, &bed).unwrap();
        history.push(o);
        rounds.push(round);
    }
    history.push(plants.observe());
    (history, rounds)
}

fn living(o: &Observation, role: Role) -> impl Iterator<Item = &Tissue> {
    o.tissues
        .iter()
        .filter(move |t| t.policy == POLICY && t.role == role && t.health > 0)
}

#[test]
fn conifer_grows_from_a_seed_into_a_conical_crown_and_replays() {
    let (history, rounds) = grow_under(600, |_| FULL);
    let o = history.last().unwrap();
    let picture = draw(o);
    let brain = Conifer::default();
    let wood: BTreeMap<[i32; 2], u16> = living(o, Role::Wood).map(|t| (t.cell, t.size)).collect();
    let trunk: Vec<u16> = (0..)
        .map_while(|y| wood.get(&[ORIGIN[0], y]).copied())
        .collect();
    assert_eq!(trunk.len() as i32, brain.max_height, "{trunk:?}\n{picture}");
    // The trunk tapers from a thickened base.
    assert!(trunk[0] >= 4, "{trunk:?}\n{picture}");
    assert!(
        trunk.windows(2).all(|w| w[0] >= w[1]),
        "{trunk:?}\n{picture}"
    );
    // A tier every two cells from the third up to a tier below the top, each
    // reaching one cell further than the tier above it, on both sides.
    for side in [-1, 1] {
        let reach: Vec<usize> = (brain.first_tier..=brain.max_height - brain.tier_spacing)
            .step_by(brain.tier_spacing as usize)
            .map(|tier| {
                (1..WIDTH)
                    .take_while(|d| wood.contains_key(&[ORIGIN[0] + side * d, ORIGIN[1] + tier]))
                    .count()
            })
            .collect();
        assert_eq!(reach, [6, 5, 4, 3, 2], "{picture}");
    }
    // As the tiers above extended over them, the needles inside the crown fell
    // into shade and were shed. Every needle left is lit, and nothing dead is
    // left.
    let bed = Bed::new(o, FULL);
    let needles: Vec<&Tissue> = living(o, Role::Shoot).collect();
    assert!(needles.len() >= 12, "{} needles\n{picture}", needles.len());
    assert!(needles.iter().all(|t| bed.lit(t.cell) > 0), "{picture}");
    assert!(o.tissues.iter().all(|t| t.health > 0), "{picture}");
    let shaded = rounds
        .iter()
        .flat_map(|r| &r.records)
        .filter(|r| r.bid.action == Action::Shed && matches!(r.outcome, Outcome::Won(_)))
        .filter(|r| r.view.point.kind == Kind::Needle && r.view.point.shade_ticks > 0)
        .count();
    assert!(shaded >= 20, "{shaded} shaded needles shed\n{picture}");
    // Roots stay within the depth the brain allows.
    let roots: Vec<&Tissue> = living(o, Role::Root).collect();
    assert!(roots.len() >= 3, "{picture}");
    assert!(
        roots
            .iter()
            .all(|t| t.cell[1] >= ORIGIN[1] - brain.max_depth)
    );
    // Every decision kept to its budget, and wood never fed more foliage than
    // its size supplies.
    let traits = Traits::default();
    assert!(
        rounds
            .iter()
            .all(|r| r.intents.len() <= traits.bids_per_decision)
    );
    for record in rounds.iter().flat_map(|r| &r.records) {
        let p = &record.view.point;
        if let Some(carry) = p.carry {
            assert!(p.foliage_beyond <= carry, "{p:?}");
        }
    }
    let own = o.tissues.iter().filter(|t| t.policy == POLICY).count();
    assert!(own <= traits.max_tissues);
    assert_eq!((history, rounds), grow_under(600, |_| FULL));
}

#[test]
fn light_water_and_nutrients_each_hold_the_tree_back_until_they_return() {
    let wood = |o: &Observation| living(o, Role::Wood).count();
    let (full, _) = grow_under(300, |_| FULL);
    let grown = wood(&full[300]);
    for (limit, conditions) in [
        ("light", (400, [1000; 2])),
        ("water", (1000, [1000, 300])),
        ("nutrients", (1000, [300, 1000])),
    ] {
        let (run, _) = grow_under(600, |tick| if tick < 300 { conditions } else { FULL });
        let (held, after) = (&run[300], &run[600]);
        // Short of one, the seedling lives on but builds at most a quarter
        // of the wood a tree in full supply has by then.
        assert!(
            wood(held) * 4 <= grown && living(held, Role::Root).count() > 0,
            "{limit}: {} wood\n{}",
            wood(held),
            draw(held)
        );
        // In as many ticks after supply returns, it builds at least three
        // quarters of that wood.
        assert!(
            wood(after) * 4 >= grown * 3,
            "{limit}: {} wood\n{}",
            wood(after),
            draw(after)
        );
    }
}

#[test]
fn the_crown_shades_out_grass_beneath_it_while_grass_beyond_it_grows() {
    let (mut plants, tree) = planted(Traits::default());
    let grass = grass::Grass::new(grass::Traits::default(), [19, -1], [0, 1], SUPPORT).unwrap();
    let bed = Bed::new(&plants.observe(), FULL);
    grass.plant(&mut plants, &bed).unwrap();
    let blades = |o: &Observation, alive: bool| -> Vec<Tissue> {
        o.tissues
            .iter()
            .filter(|t| {
                t.policy == grass::POLICY && t.role == Role::Shoot && (t.health > 0) == alive
            })
            .cloned()
            .collect()
    };
    let mut brain = Conifer::default();
    let mut history = Vec::new();
    for _ in 0..600 {
        let o = plants.observe();
        let bed = Bed::new(&o, FULL);
        let mut intents = tree.plan(&o, &bed, &bed, &mut brain).intents;
        intents.extend(grass.plan(&o, &bed).intents);
        plants.tick(o.tick, false, 1, &intents, &bed).unwrap();
        history.push(o);
    }
    let o = plants.observe();
    let picture = draw(&o);
    // The tree grew its full crown beside the grass.
    let crown: Vec<i32> = living(&o, Role::Wood).map(|t| t.cell[0]).collect();
    let (left, right) = (*crown.iter().min().unwrap(), *crown.iter().max().unwrap());
    assert_eq!((left, right), (ORIGIN[0] - 6, ORIGIN[0] + 6), "{picture}");
    let under = |t: &Tissue| (left..=right).contains(&t.cell[0]);
    // Grass under the crown grew in the light before the crown closed over
    // it, and died back in its shade.
    let lit_under = history
        .iter()
        .map(|o| {
            let bed = Bed::new(o, FULL);
            blades(o, true)
                .iter()
                .filter(|t| under(t) && bed.lit(t.cell) > 0)
                .count()
        })
        .max()
        .unwrap();
    assert!(lit_under >= 2, "{picture}");
    assert!(blades(&o, true).iter().all(|t| !under(t)), "{picture}");
    assert!(
        blades(&o, false).iter().filter(|t| under(t)).count() >= 2,
        "{picture}"
    );
    // Beyond the crown, grass lives in full light.
    let bed = Bed::new(&o, FULL);
    let open = blades(&o, true);
    assert!(open.len() >= 3, "{picture}");
    assert!(open.iter().all(|t| bed.lit(t.cell) == 1000), "{picture}");
}

#[derive(Default)]
struct Script {
    bids: BTreeMap<TissueId, Bid>,
    asked: Vec<TissueId>,
}
impl TreeBrain for Script {
    fn bid(&mut self, view: &View) -> Bid {
        self.asked.push(view.point.tissue);
        self.bids
            .get(&view.point.tissue)
            .cloned()
            .unwrap_or_else(Bid::wait)
    }
}
/// A brain that bids as listed, one bid per tissue, and waits elsewhere.
fn scripted(bids: &[(TissueId, Action, i16, &[usize])]) -> Script {
    Script {
        bids: bids
            .iter()
            .map(|&(id, action, priority, ranking)| {
                let bid = Bid {
                    action,
                    priority,
                    ranking: ranking.to_vec(),
                };
                (id, bid)
            })
            .collect(),
        asked: Vec::new(),
    }
}
fn outcome(round: &Round, tissue: TissueId) -> Outcome {
    round
        .records
        .iter()
        .find(|r| r.view.point.tissue == tissue)
        .unwrap()
        .outcome
}

/// Candidate indices: up, down, and the sides in the bed's +x and -x.
const UP_: usize = 0;
const DOWN_: usize = 1;
const RIGHT: usize = 2;
const LEFT: usize = 3;

/// Seed 1 at the origin, trunk 2 and 3 above it, needle 4 left of 2, and a
/// branch 5 right of 3 with needle 6 on top.
fn sapling(traits: Traits) -> (Plants, Tree) {
    let (mut plants, tree) = planted(traits);
    let x = ORIGIN[0];
    for (parent, role, cell) in [
        (1, Role::Wood, [x, 0]),
        (2, Role::Wood, [x, 1]),
        (2, Role::Shoot, [x - 1, 0]),
        (3, Role::Wood, [x + 1, 1]),
        (5, Role::Shoot, [x + 1, 2]),
    ] {
        let o = plants.observe();
        let bed = Bed::new(&o, FULL);
        let intent = Intent::Grow {
            parent,
            role,
            size: tree.traits.size(role),
            cell,
            support: Some(SUPPORT),
        };
        plants.tick(o.tick, false, 1, &[intent], &bed).unwrap();
        assert_eq!(plants.observe().ledger.results, vec![Ok(())]);
    }
    (plants, tree)
}

fn plan(plants: &Plants, tree: &Tree, brain: &mut impl TreeBrain) -> Round {
    let o = plants.observe();
    let bed = Bed::new(&o, FULL);
    tree.plan(&o, &bed, &bed, brain)
}

#[test]
fn every_point_bids_at_every_decision_even_when_it_cannot_pay() {
    let (plants, tree) = sapling(Traits::default());
    let mut brain = Script::default();
    let round = plan(&plants, &tree, &mut brain);
    assert_eq!(brain.asked, vec![1, 2, 3, 4, 5, 6]);
    let kinds: Vec<Kind> = round.records.iter().map(|r| r.view.point.kind).collect();
    use Kind::*;
    assert_eq!(kinds, [RootTip, Trunk, Leader, Needle, BranchTip, Needle]);
    assert!(round.records.iter().all(|r| r.outcome == Outcome::Waited));

    // A broke seed is still asked, and the world refuses what it cannot pay.
    let (mut broke, tree) = planted(Traits {
        starting_reserves: [0; 3],
        ..Traits::default()
    });
    let round = plan(&broke, &tree, &mut Conifer::default());
    assert_eq!(round.records.len(), 1);
    assert_eq!(round.records[0].bid.action, Action::Grow(Role::Wood));
    assert_eq!(
        round.records[0].outcome,
        Outcome::Refused(Refusal::Resource)
    );
    assert!(round.intents.is_empty());
    let o = broke.observe();
    broke
        .tick(o.tick, false, 1, &round.intents, &Bed::new(&o, FULL))
        .unwrap();

    // Between decisions no one is asked, and physiology still runs.
    let slow = Tree::new(
        Traits {
            decision_interval_ticks: 3,
            ..Traits::default()
        },
        ORIGIN,
        [0, 1],
        SUPPORT,
    )
    .unwrap();
    let mut brain = Script::default();
    let round = plan(&broke, &slow, &mut brain);
    assert_eq!(round.waiting, Some(Limit::Cadence));
    assert!(brain.asked.is_empty() && round.records.is_empty());
}

#[test]
fn the_world_takes_bids_highest_first_and_checks_each() {
    let (plants, tree) = sapling(Traits {
        bids_per_decision: 2,
        ..Traits::default()
    });
    let x = ORIGIN[0];
    // Trunk 2 and branch 5 both want the cell right of 2, below 5, and the
    // higher bid gets it. The seed cannot be shed, the trunk's right is held
    // by the branch, and the needle cannot grow.
    let mut brain = scripted(&[
        (1, Action::Shed, 90, &[]),
        (2, Action::Grow(Role::Shoot), 80, &[LEFT, RIGHT]),
        (3, Action::Grow(Role::Wood), 70, &[RIGHT]),
        (4, Action::Grow(Role::Shoot), 60, &[UP_]),
        (5, Action::Grow(Role::Shoot), 50, &[DOWN_, RIGHT]),
        (6, Action::Thicken, 40, &[]),
    ]);
    let round = plan(&plants, &tree, &mut brain);
    assert_eq!(outcome(&round, 1), Outcome::Refused(Refusal::Seed));
    assert_eq!(outcome(&round, 2), Outcome::Won(Some(RIGHT)));
    assert_eq!(outcome(&round, 3), Outcome::Refused(Refusal::Space));
    assert_eq!(outcome(&round, 4), Outcome::Refused(Refusal::Role));
    // Its first choice is taken by now, so 5 grows to its right instead, and
    // that spends the decision's two winning bids.
    assert_eq!(outcome(&round, 5), Outcome::Won(Some(RIGHT)));
    assert_eq!(outcome(&round, 6), Outcome::Outbid);
    let needle = |parent, cell| Intent::Grow {
        parent,
        role: Role::Shoot,
        size: 1,
        cell,
        support: Some(SUPPORT),
    };
    assert_eq!(
        round.intents,
        [needle(2, [x + 1, 0]), needle(5, [x + 2, 1])]
    );

    // A bid the stores cannot pay is refused and cheaper bids after it still
    // go. Shedding costs nothing and takes what grew beyond the shed tissue.
    let (mut poor, tree) = sapling(Traits::default());
    for t in poor.tissues.values_mut() {
        t.stores = if t.id == 1 { [4; 3] } else { [0; 3] };
    }
    let mut brain = scripted(&[
        (1, Action::Grow(Role::Root), 80, &[DOWN_]),
        (3, Action::Shed, 75, &[]),
        (5, Action::Grow(Role::Shoot), 70, &[RIGHT]),
        (2, Action::Grow(Role::Shoot), 60, &[RIGHT]),
        (4, Action::Thicken, 50, &[]),
        (6, Action::Shed, 40, &[]),
    ]);
    let round = plan(&poor, &tree, &mut brain);
    assert_eq!(outcome(&round, 1), Outcome::Refused(Refusal::Resource));
    assert_eq!(outcome(&round, 3), Outcome::Won(None));
    assert_eq!(outcome(&round, 5), Outcome::Refused(Refusal::Shed));
    assert_eq!(outcome(&round, 2), Outcome::Won(Some(RIGHT)));
    assert_eq!(outcome(&round, 4), Outcome::Refused(Refusal::Resource));
    assert_eq!(outcome(&round, 6), Outcome::Refused(Refusal::Shed));
    assert_eq!(
        round.intents,
        [Intent::Shed { tissue: 3 }, needle(2, [x + 1, 0])]
    );

    // Nothing to thicken toward, shade, a shortage and death all refuse.
    let (mut worn, tree) = sapling(Traits::default());
    for (id, shade, shortage, health) in [(4, 5, 0, 50), (6, 0, 2, 100), (2, 0, 0, 0)] {
        let t = worn.tissues.get_mut(&id).unwrap();
        (t.shade_ticks, t.shortage_ticks, t.health) = (shade, shortage, health);
    }
    let mut brain = scripted(&[
        (1, Action::Thicken, 90, &[]),
        (4, Action::Thicken, 80, &[]),
        (6, Action::Thicken, 70, &[]),
        (2, Action::Grow(Role::Shoot), 60, &[RIGHT, LEFT]),
    ]);
    let round = plan(&worn, &tree, &mut brain);
    assert_eq!(outcome(&round, 1), Outcome::Refused(Refusal::Size));
    assert_eq!(outcome(&round, 4), Outcome::Refused(Refusal::Shade));
    assert_eq!(outcome(&round, 6), Outcome::Refused(Refusal::Shortage));
    assert_eq!(outcome(&round, 2), Outcome::Refused(Refusal::Dead));
    assert!(round.intents.is_empty());

    // At its population limit the tree grows nothing more, but its needles
    // still mature.
    let (crowded, tree) = sapling(Traits {
        max_tissues: 6,
        ..Traits::default()
    });
    let mut brain = scripted(&[
        (2, Action::Grow(Role::Shoot), 60, &[RIGHT]),
        (6, Action::Thicken, 50, &[]),
    ]);
    let round = plan(&crowded, &tree, &mut brain);
    assert_eq!(outcome(&round, 2), Outcome::Refused(Refusal::Population));
    assert_eq!(outcome(&round, 6), Outcome::Won(None));
}

#[test]
fn roots_grow_roots_wood_grows_the_rest_and_thickens_to_carry_it() {
    // Each size of wood supplies one size of foliage here, so the wood under
    // the two needles is already full.
    let (mut plants, tree) = sapling(Traits {
        foliage_per_wood_size: 1,
        ..Traits::default()
    });
    let o = plants.observe();
    let root = Intent::Grow {
        parent: 1,
        role: Role::Root,
        size: 2,
        cell: [ORIGIN[0], ORIGIN[1] - 1],
        support: Some(SUPPORT),
    };
    plants
        .tick(o.tick, false, 1, &[root], &Bed::new(&o, FULL))
        .unwrap();
    let mut brain = scripted(&[
        (1, Action::Grow(Role::Shoot), 90, &[UP_]),
        (7, Action::Grow(Role::Wood), 80, &[RIGHT]),
        (2, Action::Grow(Role::Root), 70, &[DOWN_]),
        (3, Action::Grow(Role::Shoot), 60, &[UP_]),
        (6, Action::Thicken, 50, &[]),
        (5, Action::Grow(Role::Wood), 40, &[RIGHT]),
        (4, Action::Shed, 35, &[]),
    ]);
    brain.bids.insert(
        2,
        Bid {
            action: Action::Thicken,
            priority: 30,
            ranking: Vec::new(),
        },
    );
    let round = plan(&plants, &tree, &mut brain);
    let point = |id: TissueId| {
        &round
            .records
            .iter()
            .find(|r| r.view.point.tissue == id)
            .unwrap()
            .view
            .point
    };
    assert_eq!((point(2).carry, point(2).foliage_beyond), (Some(1), 2));
    assert_eq!((point(5).headroom, point(7).headroom), (Some(0), None));
    // The seed starts a trunk but no needle, other roots grow only roots, and
    // wood grows no roots.
    assert_eq!(outcome(&round, 1), Outcome::Refused(Refusal::Role));
    assert_eq!(outcome(&round, 7), Outcome::Refused(Refusal::Role));
    // Wood full of foliage takes no new needle, and no needle beyond it
    // matures, but wood still grows from it and thickens.
    assert_eq!(outcome(&round, 3), Outcome::Refused(Refusal::Load));
    assert_eq!(outcome(&round, 6), Outcome::Refused(Refusal::Load));
    assert_eq!(outcome(&round, 5), Outcome::Won(Some(RIGHT)));
    assert_eq!(outcome(&round, 4), Outcome::Won(None));
    assert_eq!(outcome(&round, 2), Outcome::Won(None));
}

#[test]
fn views_measure_neighbors_in_the_trees_frame() {
    let (plants, tree) = sapling(Traits::default());
    let o = plants.observe();
    let bed = Bed::new(&o, FULL);
    let views = tree.views(&o, &bed, &bed);
    let x = ORIGIN[0];
    // Branch 5, right of trunk 3: its needle is above, the cell below is open
    // and dark under the branch, its right is open and lit, and its left is
    // the trunk.
    let branch = &views[4];
    assert_eq!((branch.point.height, branch.point.across), (2, 1));
    let [up, down, right, left] = &branch.candidates;
    assert_eq!(up.holder, Some(Holder::Needle));
    assert_eq!((down.cell, down.along, down.outward), ([x + 1, 0], -1, 0));
    assert_eq!(down.light, Some(0));
    assert_eq!(down.open, [true, true, false]);
    assert_eq!(
        (right.cell, right.outward, right.light),
        ([x + 2, 1], 1, Some(1000))
    );
    assert_eq!(
        (left.holder, left.outward, left.light),
        (Some(Holder::Wood), -1, None)
    );
    // The seed's down is dirt a root could take.
    let seed = &views[0];
    assert_eq!(seed.candidates[DOWN_].soil, Some([1000; 2]));
    assert_eq!(seed.candidates[DOWN_].open, [false, false, true]);
    assert_eq!(seed.candidates[UP_].holder, Some(Holder::Wood));
    // Needle 6 counts toward everything it grew from, back to the seed. Wood
    // of size one supplies six sizes of foliage, and the trunk's base already
    // feeds two.
    let beyond: Vec<(u16, u16)> = views
        .iter()
        .map(|v| (v.point.needles_beyond, v.point.foliage_beyond))
        .collect();
    assert_eq!(beyond, [(2, 2), (2, 2), (1, 1), (1, 1), (1, 1), (1, 1)]);
    let headroom: Vec<Option<u16>> = views.iter().map(|v| v.point.headroom).collect();
    assert_eq!(
        headroom,
        [None, Some(4), Some(4), Some(4), Some(4), Some(4)]
    );
    let w = &views[0].whole;
    assert_eq!(
        (
            w.needles, w.foliage, w.wood, w.roots, w.dead, w.height, w.depth
        ),
        (2, 2, 3, 1, 0, 2, 0)
    );
    // Both needles are lit: one beside the trunk's base, one on the branch.
    assert_eq!(w.intake[2], 16);
    // Probes cover the empty air beside the wood, in cell order.
    assert_eq!(
        tree.probes(&o, &bed),
        vec![[x - 1, 1], [x, 2], [x + 1, 0], [x + 2, 1]]
    );
}

#[test]
fn the_encoder_keeps_its_layout_ranges_and_unknowns() {
    let (_, rounds) = grow_under(200, |_| FULL);
    let mut seen = BTreeSet::new();
    for record in rounds.iter().flat_map(|r| &r.records) {
        let x = encode(&record.view);
        assert!(x.iter().all(|v| (-1.0..=1.0).contains(v)), "{x:?}");
        // One kind, and one holder state per candidate.
        assert_eq!(x[16..23].iter().sum::<f32>(), 1.0);
        for (i, c) in record.view.candidates.iter().enumerate() {
            let f = &x[COMMON + i * PER_CANDIDATE..COMMON + (i + 1) * PER_CANDIDATE];
            assert_eq!(f[..5].iter().sum::<f32>(), 1.0);
            // An unmeasured reading is zero beside a zero flag.
            if c.light.is_none() {
                assert_eq!((f[10], f[11]), (0.0, 0.0));
            }
            if c.soil.is_none() {
                assert_eq!((f[12], f[13], f[14]), (0.0, 0.0, 0.0));
            }
        }
        seen.insert(record.view.point.kind);
    }
    assert_eq!(seen.len(), 7, "{seen:?}");
    assert_eq!((ENCODER, COMMON, PER_CANDIDATE, INPUTS), (1, 42, 15, 102));
}
