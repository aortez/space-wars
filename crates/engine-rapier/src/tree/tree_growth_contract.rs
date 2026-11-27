use super::*;
use crate::{
    grass::GrassEnvironment,
    plant_light::{PlantLight, WorldOccluders, full_sun},
    terrain::{TerrainAssembly, TerrainSpec},
    world::*,
};
use engine_core::{
    Vec2,
    lighting::LightingPublisher,
    plants::{
        Observation, Plants, Role, Tissue, grass,
        tree::{self, Conifer, Grove, Round, Tree},
    },
};
use engine_terrain::{Material, MaterialId, Terrain, TerrainGeometry};

const WIDTH: u32 = 32;
const TOP_DIRT: i32 = 7;
const TREE: [i32; 2] = [12, TOP_DIRT];
const GRASS: [i32; 2] = [20, TOP_DIRT];
/// Toward a sun fifteen degrees off vertical from the left, as in Garden
/// Lab's tree bed.
const SUN: Vec2 = Vec2::new(-0.258_819, 0.965_926);

/// A conifer, the trees grown from its seeds and a grass patch on one
/// support, in full sun from this direction, through the real light
/// publication with the grove's probes, the light the trees' wood and needles
/// let through, and the real soil: the bed before each tick and after the
/// last, and each tick's decisions, the planted tree's first.
fn grow(ticks: u64, sun: Vec2) -> (Vec<Observation>, Vec<Vec<Round>>) {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    let terrain = Terrain::generate(
        WIDTH,
        26,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 10,
        }],
        |c| {
            if c.y <= TOP_DIRT {
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
            ..Default::default()
        },
        &terrain,
        &geometry,
        TerrainSpec::default(),
    )
    .unwrap();
    world.step(1.0 / 60.0);
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
    let up = crate::grass::local_up(&roots, Vec2::new(0.0, -1.0)).unwrap();
    let support = assembly.plant_support();
    let tree = Tree::new(tree::Traits::default(), TREE, up, support).unwrap();
    let grass = grass::Grass::new(grass::Traits::default(), GRASS, up, support).unwrap();
    let mut plants = Plants::new(grass::physiology(), 42).unwrap();
    let env = GrassEnvironment {
        roots: &roots,
        physiology: &roots,
    };
    tree.plant(&mut plants, &env).unwrap();
    grass.plant(&mut plants, &env).unwrap();
    let mut grove = Grove { trees: vec![tree] };
    let mut brain = Conifer::default();
    let mut publisher = LightingPublisher::default();
    let (mut history, mut rounds) = (Vec::new(), Vec::new());
    for _ in 0..ticks {
        let o = plants.observe();
        let probes = grove.probes(&o, &env);
        assert!(probes.len() <= tree::MAX_PROBES);
        let owners = grove.owners(&o);
        let light = PlantLight::publish_with_probes(
            &mut publisher,
            o.tick,
            o.tick,
            full_sun(sun),
            &roots,
            &o.tissues,
            u64::MAX,
            WorldOccluders {
                tick: o.tick,
                geometry_generation: o.tick,
                footprints: &[],
            },
            &probes,
            &|t| {
                owners
                    .get(&t.organism)
                    .map_or(0.0, |tree| tree.transmits(t))
            },
        )
        .unwrap();
        let lit = GrassEnvironment {
            roots: &roots,
            physiology: light.environment(o.tick).unwrap(),
        };
        let senses = TreeSenses {
            light: &light,
            roots: &roots,
        };
        // The grass makes at most one intent a tick; the trees share the rest.
        let grass_plan = grass.plan(&o, &lit);
        let budget = o.config.max_intents - grass_plan.intents.len();
        let decisions = grove.plan(&o, &lit, &senses, &mut brain, budget);
        let mut intents: Vec<_> = decisions
            .iter()
            .flat_map(|r| r.intents.iter().cloned())
            .collect();
        intents.extend(grass_plan.intents);
        plants.tick(o.tick, false, 1, &intents, &lit).unwrap();
        grove.tend(&mut plants).unwrap();
        history.push(o);
        rounds.push(decisions);
    }
    history.push(plants.observe());
    (history, rounds)
}

/// Rows from the top: the trees' wood `#`, needles `*`, cones `o` and roots
/// `r`, grass blades `,`, runners `~` and roots `;`, dead tissue `x`.
fn draw(o: &Observation) -> String {
    let at: std::collections::BTreeMap<[i32; 2], &Tissue> =
        o.tissues.iter().map(|t| (t.cell, t)).collect();
    (0..26)
        .rev()
        .map(|y| {
            (0..WIDTH as i32)
                .map(|x| match at.get(&[x, y]) {
                    Some(t) if t.health == 0 => 'x',
                    Some(t) => match (t.policy == tree::POLICY, t.role) {
                        (true, Role::Wood) => '#',
                        (true, Role::Shoot) => '*',
                        (true, Role::Cone) => 'o',
                        (true, _) => 'r',
                        (false, Role::Shoot) => ',',
                        (false, Role::Runner) => '~',
                        (false, _) => ';',
                    },
                    None if y <= TOP_DIRT => ':',
                    None => '.',
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn tree_growth_contract_conifer_lit_from_the_left_shades_grass_and_replays() {
    let (history, rounds) = grow(600, SUN);
    let o = history.last().unwrap();
    let picture = draw(o);
    let brain = Conifer::default();
    let own = |role: Role| {
        o.tissues
            .iter()
            .filter(move |t| t.policy == tree::POLICY && t.role == role && t.health > 0)
    };
    // A full-height trunk with tiers that shorten toward the top.
    let wood: std::collections::BTreeSet<[i32; 2]> = own(Role::Wood).map(|t| t.cell).collect();
    let trunk = (1..)
        .take_while(|h| wood.contains(&[TREE[0], TREE[1] + h]))
        .count();
    assert_eq!(trunk as i32, brain.max_height, "{picture}");
    for side in [-1, 1] {
        let reach: Vec<usize> = (brain.first_tier..=brain.max_height - brain.tier_spacing)
            .step_by(brain.tier_spacing as usize)
            .map(|h| {
                (1..)
                    .take_while(|d| wood.contains(&[TREE[0] + side * d, TREE[1] + h]))
                    .count()
            })
            .collect();
        assert_eq!(reach, [6, 5, 4, 3, 2], "{picture}");
    }
    // The brain read light at probed cells, in full sun and through the
    // crown, and supply at dirt cells.
    let records: Vec<_> = rounds.iter().flatten().flat_map(|r| &r.records).collect();
    let candidates = || records.iter().flat_map(|r| &r.view.candidates);
    assert!(candidates().any(|c| c.light == Some(1000)));
    assert!(candidates().any(|c| c.light.is_some_and(|l| l > 0 && l < 1000)));
    assert!(candidates().any(|c| c.soil == Some([1000; 2])));
    // Light gets through the crown: every needle it kept is lit, many
    // through the tissue above them, and the sunny side carries more.
    let last = &rounds.last().unwrap()[0];
    let needles: Vec<_> = last
        .records
        .iter()
        .filter(|r| r.view.point.kind == tree::Kind::Needle && r.view.point.alive)
        .collect();
    let light = |r: &&tree::Record| r.view.point.light.unwrap_or(0);
    assert!(needles.len() >= 30, "{picture}");
    assert!(needles.iter().all(|r| light(r) >= 250), "{picture}");
    assert!(
        needles.iter().filter(|r| light(r) < 1000).count() >= 10,
        "{picture}"
    );
    let side = |s: i32| {
        own(Role::Shoot)
            .filter(|t| (t.cell[0] - TREE[0]).signum() == s)
            .count()
    };
    assert!(side(-1) > side(1), "{picture}");
    // Grass in the crown's shadow, on its right, died; beyond it, grass lives.
    let blades = |alive: bool| {
        o.tissues
            .iter()
            .filter(move |t| {
                t.policy == grass::POLICY && t.role == Role::Shoot && (t.health > 0) == alive
            })
            .map(|t| t.cell[0])
    };
    assert!(blades(false).any(|x| x == TREE[0] + 5), "{picture}");
    assert!(blades(true).count() >= 6, "{picture}");
    assert_eq!((history, rounds), grow(600, SUN));
}

/// What became of a seed: where and when it landed, and when it sprouted or
/// was cleared away, if it did.
#[derive(Debug)]
struct Life {
    parent: u64,
    cell: [i32; 2],
    landed: usize,
    sprouted: Option<usize>,
    cleared: Option<usize>,
}

fn lives(history: &[Observation]) -> std::collections::BTreeMap<u64, Life> {
    let mut lives = std::collections::BTreeMap::new();
    for (tick, pair) in history.windows(2).enumerate() {
        let (before, after) = (&pair[0], &pair[1]);
        let wood = |o: &Observation, organism| {
            o.tissues
                .iter()
                .any(|t| t.organism == organism && t.role == Role::Wood)
        };
        for (&child, &parent) in &after.parents {
            if !before.parents.contains_key(&child) {
                let cell = after
                    .tissues
                    .iter()
                    .find(|t| t.organism == child)
                    .unwrap()
                    .cell;
                lives.insert(
                    child,
                    Life {
                        parent,
                        cell,
                        landed: tick,
                        sprouted: None,
                        cleared: None,
                    },
                );
            }
        }
        for (&child, life) in lives.iter_mut() {
            if !after.parents.contains_key(&child) && life.cleared.is_none() {
                life.cleared = Some(tick);
            }
            if wood(after, child) && !wood(before, child) {
                life.sprouted = Some(tick);
            }
        }
    }
    lives
}

#[test]
fn tree_growth_contract_seeds_sprout_in_the_open_and_die_in_the_crowns_shade() {
    let (history, rounds) = grow(1000, SUN);
    let lives = lives(&history);
    let records: Vec<_> = rounds.iter().flatten().flat_map(|r| &r.records).collect();
    // The grown conifer's cones dropped seeds onto free dirt in the top row,
    // never onto the grass or the tree's own roots: a gust toward those left
    // the seed in its cone.
    assert!(lives.len() >= 4, "{lives:#?}");
    assert!(
        lives
            .values()
            .all(|l| l.parent == 1 && l.cell[1] == TOP_DIRT)
    );
    assert!(records.iter().any(|r| {
        r.bid.action == tree::Action::Release
            && r.outcome == tree::Outcome::Refused(tree::Refusal::Space)
    }));
    // Seeds in the sun beyond the crown, left of it and right of the grass,
    // sprouted at their first decision and grew needles of their own.
    let o = history.last().unwrap();
    let open: Vec<_> = lives.values().filter(|l| l.sprouted.is_some()).collect();
    assert!(open.len() >= 2, "{lives:#?}");
    for l in &open {
        assert_eq!(l.sprouted, Some(l.landed + 1), "{l:?}");
        assert!(l.cell[0] < 6 || l.cell[0] > GRASS[0] + 4, "{l:?}");
        let tree = o
            .tissues
            .iter()
            .find(|t| t.cell == l.cell)
            .unwrap()
            .organism;
        assert!(
            o.tissues
                .iter()
                .any(|t| { t.organism == tree && t.role == Role::Shoot && t.health > 0 })
        );
    }
    // One under the crown's right side stayed in its shade: it never
    // sprouted, died when its reserves ran out, and was cleared away.
    let shaded = lives
        .iter()
        .find(|(_, l)| (TREE[0]..=TREE[0] + 6).contains(&l.cell[0]))
        .expect("a seed under the crown");
    let (organism, life) = shaded;
    assert_eq!(life.sprouted, None);
    assert!(
        life.cleared.is_some_and(|c| c > life.landed + 100),
        "{life:?}"
    );
    for r in records.iter().filter(|r| {
        !r.view.point.attached
            && history[life.landed + 1]
                .tissues
                .iter()
                .any(|t| t.id == r.view.point.tissue && t.organism == *organism)
    }) {
        assert_eq!(
            r.outcome,
            tree::Outcome::Refused(tree::Refusal::Shade),
            "{life:?}"
        );
    }
    assert_eq!((history, rounds), grow(1000, SUN));
}
