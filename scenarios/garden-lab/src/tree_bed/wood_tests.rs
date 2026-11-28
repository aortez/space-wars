//! The tree bed's physical wood. Held as it stood, the grown conifer sags
//! under its own weight and settles, snow bends a branch that springs back
//! when it clears, and heavy snow breaks the branch away to fall onto the
//! dirt. Growing on, the trees grow as they would at rest while new wood is
//! hinged where its parent hangs and old wood is reshaped where it hangs;
//! wood they shed comes away with the motion it had, a tree that dies stands
//! as dead wood, the oldest dead wood is cleared past the cap, and nothing of
//! the bodies outlives replanting.
use super::*;

/// The grown conifer, its wood made physical.
fn grown() -> TreeBed {
    let mut bed = TreeBed::new(42);
    while bed.plants.observe().tick < 600 {
        bed.tick();
    }
    bed
}

/// Let the bed run for `seconds` of displayed updates, its trees growing at
/// its speed.
fn run(bed: &mut TreeBed, seconds: f32) {
    for _ in 0..(seconds / WOOD_DT).round() as u32 {
        bed.advance(Duration::from_secs_f32(WOOD_DT));
    }
}

/// Move the bodies alone for `seconds`, the trees' clock held, so that the
/// wood is measured as it stood.
fn hold(bed: &mut TreeBed, seconds: f32) {
    for _ in 0..(seconds / WOOD_DT).round() as u32 {
        bed.step_wood(WOOD_DT);
    }
}

fn hinge(bed: &TreeBed, cell: [i32; 2]) -> Hinge {
    *bed.hinges()
        .iter()
        .find(|h| h.cell == cell)
        .unwrap_or_else(|| panic!("no living wood at {cell:?}"))
}

/// The lowest branch toward the grass reaches from the trunk at x 12 out to
/// x 18 along row 10; the one away from it, thicker near the trunk, out to x 6.
const RIGHT_ROOT: [i32; 2] = [13, 10];
const RIGHT_TIP: [i32; 2] = [18, 10];
const LEFT_TIP: [i32; 2] = [6, 10];

/// The droop of a branch's tip over the last second of `seconds`: its least
/// and its most.
fn settle(bed: &mut TreeBed, seconds: f32, tip: [i32; 2]) -> (f32, f32) {
    hold(bed, seconds - 1.0);
    let (mut least, mut most) = (f32::MAX, f32::MIN);
    for _ in 0..(1.0 / WOOD_DT).round() as u32 {
        hold(bed, WOOD_DT);
        let droop = hinge(bed, tip).droop;
        least = least.min(droop);
        most = most.max(droop);
    }
    (least, most)
}

/// The bed's longest line of text, in characters.
fn longest_line(bed: &TreeBed) -> usize {
    bed.render_frame()
        .layers
        .into_iter()
        .flat_map(|l| l.primitives)
        .filter_map(|p| match p {
            RenderPrimitive::Text(t) => Some(t.text.chars().count()),
            _ => None,
        })
        .max()
        .unwrap()
}

/// Every point of the bed's drawing but its text.
fn shapes(bed: &TreeBed) -> Vec<RenderPoint> {
    bed.render_frame()
        .layers
        .into_iter()
        .flat_map(|l| l.primitives)
        .flat_map(|p| match p {
            RenderPrimitive::Circle(c) => vec![c.center],
            RenderPrimitive::Line(l) => vec![l.start, l.end],
            RenderPrimitive::Polygon(p) => p.points,
            RenderPrimitive::Text(_) => vec![],
        })
        .collect()
}

#[test]
fn physical_wood_sags_under_its_own_weight_and_settles() {
    let mut bed = grown();
    let o = bed.plants.observe();
    let wood = o
        .tissues
        .iter()
        .filter(|t| t.policy == tree::POLICY && t.role == Role::Wood)
        .count();
    let at_rest = shapes(&bed);
    bed.command(Command::PhysicalWood);
    assert!(bed.wood.physical);
    assert_eq!(bed.wood.living.len(), wood);
    assert_eq!(bed.hinges().len(), wood);
    // Built where it is drawn, it has not moved yet.
    let built = shapes(&bed);
    assert_eq!(built.len(), at_rest.len());
    for (a, b) in at_rest.iter().zip(&built) {
        assert!(
            (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4,
            "{a:?} drawn at {b:?}"
        );
    }

    let (least, most) = settle(&mut bed, 8.0, RIGHT_TIP);
    assert!(
        (0.2..0.4).contains(&least) && most - least < 0.01,
        "the lowest branch's tip droops {least}..{most}"
    );
    let root = hinge(&bed, RIGHT_ROOT);
    assert!(
        (0.25..0.4).contains(&root.utilization),
        "the lowest branch's root carries {}",
        root.utilization
    );
    let worst = bed
        .hinges()
        .iter()
        .map(|h| h.utilization)
        .fold(0.0, f32::max);
    assert!(worst < 0.5, "a hinge carries {worst} of its safe load");
    assert_eq!((bed.wood.breaks, bed.wood.dead.len()), (0, 0));
    assert!(
        bed.wood_summary()
            .starts_with(&format!("Wood on {wood} hinges, most loaded 0."))
    );
}

#[test]
fn light_snow_bends_a_branch_that_springs_back_when_it_clears() {
    let mut bed = grown();
    bed.command(Command::PhysicalWood);
    let (bare, _) = settle(&mut bed, 8.0, RIGHT_TIP);
    // With nothing selected but the first tissue, a root, snow lands on the
    // lowest branch's tip and builds up over a second.
    bed.command(Command::LightSnow);
    let snow = bed.wood.snow.unwrap();
    assert_eq!(hinge(&bed, RIGHT_TIP).tissue, snow.on);
    assert_eq!(snow.share(), 0.0);
    hold(&mut bed, SNOW_SECONDS);
    assert!(bed.wood.snow.unwrap().share() > 0.999);
    let (least, most) = settle(&mut bed, 6.0, RIGHT_TIP);
    assert!(
        (1.8 * bare..2.4 * bare).contains(&least) && most - least < 0.01,
        "under light snow the tip droops {least}..{most}, bare {bare}"
    );
    let root = hinge(&bed, RIGHT_ROOT).utilization;
    assert!((0.5..0.8).contains(&root), "the root carries {root}");
    assert!(bed.wood_summary().contains("snow light at [18, 10] |"));
    // Again clears it, and the branch springs back.
    bed.command(Command::LightSnow);
    assert!(bed.wood.snow.is_none());
    let (least, most) = settle(&mut bed, 10.0, RIGHT_TIP);
    assert!(
        (least - bare).abs() < 0.01 && (most - bare).abs() < 0.01,
        "cleared, the tip droops {least}..{most}, bare {bare}"
    );
    assert_eq!(bed.wood.breaks, 0);
}

#[test]
fn the_same_snow_bends_a_thicker_branch_and_one_nearer_the_trunk_less() {
    // How far light snow on `on` adds to `tip`'s droop, settled.
    let added = |on: [i32; 2], tip: [i32; 2]| {
        let mut bed = grown();
        bed.command(Command::PhysicalWood);
        let (bare, _) = settle(&mut bed, 8.0, tip);
        let o = bed.plants.observe();
        bed.selected = o.tissues.iter().position(|t| t.cell == on).unwrap();
        bed.command(Command::LightSnow);
        assert_eq!(hinge(&bed, on).tissue, bed.wood.snow.unwrap().on);
        let (least, _) = settle(&mut bed, 8.0, tip);
        least - bare
    };
    let right = added(RIGHT_TIP, RIGHT_TIP);
    let left = added(LEFT_TIP, LEFT_TIP);
    let near = added(RIGHT_ROOT, RIGHT_TIP);
    assert!(left < 0.6 * right, "thicker {left}, thinner {right}");
    assert!(
        near < 0.1 * right,
        "near the trunk {near}, at the tip {right}"
    );
}

#[test]
fn heavy_snow_breaks_a_branch_that_falls_to_the_dirt_as_dead_wood() {
    let mut bed = grown();
    bed.command(Command::PhysicalWood);
    hold(&mut bed, 8.0);
    let living = bed.wood.living.len();
    let branch: Vec<[i32; 2]> = (13..=18).map(|x| [x, 10]).collect();
    let frame = |bed: &TreeBed| {
        bed.render_frame()
            .layers
            .into_iter()
            .flat_map(|l| l.primitives)
            .collect::<Vec<_>>()
    };
    let grey = |primitives: &[RenderPrimitive]| {
        primitives
            .iter()
            .filter(|p| matches!(p, RenderPrimitive::Polygon(p) if p.fill.as_ref().is_some_and(|f| f.color == DEAD)))
            .count()
    };
    let dead = grey(&frame(&bed));
    bed.command(Command::HeavySnow);
    let mut seconds = 0.0;
    let mut longest = 0;
    while bed.wood.breaks == 0 {
        hold(&mut bed, WOOD_DT);
        seconds += WOOD_DT;
        assert!(seconds < 2.0, "heavy snow broke nothing in 2 s");
        longest = longest.max(longest_line(&bed));
    }
    // The wood's summary is longest while snow builds up, and it shares the
    // light's line. A window 1280 pixels wide fits about 208 characters of
    // the bed's text, and later in a run that line's numbers have up to 16
    // more digits.
    assert!(longest <= 192, "a line of {longest} characters");
    // The branch's root broke, and all of it left the tree.
    assert_eq!(bed.wood.breaks, 1);
    assert_eq!(bed.wood.dead.len(), branch.len());
    assert_eq!(bed.wood.living.len(), living - branch.len());
    let o = bed.plants.observe();
    assert!(!o.tissues.iter().any(|t| branch.contains(&t.cell)));
    assert!(!bed.wood.snow.unwrap().fastened);
    assert!(bed.wood_summary().contains("snow heavy slid off |"));

    // It lands on the dirt, and the snow beside it, and they come to rest.
    hold(&mut bed, 4.0);
    let surface = bed.at([0, TOP_DIRT]).y + 0.5;
    for dead in &bed.wood.dead {
        let m = bed.world.motion(dead.segment.body()).unwrap();
        assert!(
            m.position.y > surface && m.position.y < surface + 1.0,
            "dead wood at {:?} above the dirt at {surface}",
            m.position
        );
        assert!(m.linear_velocity.length() < 0.05, "still moving: {m:?}");
    }
    let snow = bed.world.motion(bed.wood.snow.unwrap().body()).unwrap();
    assert!(
        snow.position.y > surface && snow.position.y < surface + 0.5,
        "the snow at {:?}",
        snow.position
    );
    assert!(
        snow.linear_velocity.length() < 0.05,
        "the snow moves: {snow:?}"
    );
    // Fallen, it is drawn grey where it lies.
    assert!(grey(&frame(&bed)) > dead + 6);
    assert_eq!(bed.wood.breaks, 1);
}

#[test]
fn a_cut_drops_physical_wood_and_resting_wood_grows_again_without_leftovers() {
    let mut bed = grown();
    let bodies = bed.world.body_count();
    bed.command(Command::PhysicalWood);
    hold(&mut bed, 2.0);
    let living = bed.wood.living.len();
    bed.command(Command::CutBranch);
    assert_eq!((bed.cuts, bed.wood.breaks), (1, 0));
    assert_eq!(bed.wood.dead.len(), 6);
    assert_eq!(bed.wood.living.len(), living - 6);
    hold(&mut bed, 3.0);
    let lying: Vec<Vec2> = bed
        .wood
        .dead
        .iter()
        .map(|f| bed.world.motion(f.segment.body()).unwrap().position)
        .collect();
    assert!(lying.iter().all(|p| p.y < bed.at([0, TOP_DIRT]).y + 1.5));

    // Resting the wood takes its living bodies away, and the tree grows on
    // as its cells stand. The dead wood stays where it lies.
    let tick = bed.plants.observe().tick;
    bed.command(Command::PhysicalWood);
    assert!(!bed.wood.physical && bed.wood.living.is_empty());
    assert_eq!(bed.world.body_count(), bodies + 6);
    run(&mut bed, 1.0);
    assert!(bed.plants.observe().tick > tick);
    assert_eq!(bed.wood.dead.len(), 6);
    assert!(bed.wood_summary().starts_with("Wood at rest"));

    // Replanted, the bed keeps none of it.
    bed.command(Command::LightSnow);
    assert!(bed.wood.physical && bed.wood.snow.is_some());
    bed.command(Command::PlantTree);
    assert!(!bed.wood.physical && bed.wood.snow.is_none());
    assert!(bed.wood.living.is_empty() && bed.wood.dead.is_empty());
    assert_eq!(bed.world.body_count(), bodies);
}

/// Where a segment's ends are, as its body now lies: the end it grew from,
/// and its own.
fn ends(bed: &TreeBed, segment: &Segment) -> (Vec2, Vec2) {
    let m = bed.world.motion(segment.body()).unwrap();
    let half = Vec2::new(0.0, 0.5 * segment.length).rotate_radians(m.angle);
    (m.position - half, m.position + half)
}

/// Where living wood should start: the end of the wood it is hinged to, or
/// for a trunk's foot the cell it grew from.
fn anchor(bed: &TreeBed, o: &plants::Observation, id: u64) -> Vec2 {
    match bed.wood.living[&id].parent {
        Some(p) => ends(bed, &bed.wood.living[&p]).1,
        None => {
            let p = o.connections.iter().find(|&&(_, c)| c == id).unwrap().0;
            bed.at(o.tissues.iter().find(|t| t.id == p).unwrap().cell)
        }
    }
}

/// What a wood tissue's segment weighs: a square beam of wood as wide as it
/// and as long, with the needles and cones that grow on it.
fn weight(o: &plants::Observation, id: u64, segment: &Segment) -> f32 {
    let own = engine_material::WOOD.density * segment.width * segment.width * segment.length;
    let on: f32 = o
        .connections
        .iter()
        .filter(|&&(p, _)| p == id)
        .map(|&(_, c)| o.tissues.iter().find(|t| t.id == c).unwrap())
        .map(|c| match c.role {
            Role::Shoot => NEEDLE_MASS * f32::from(c.size),
            Role::Cone => CONE_MASS * f32::from(c.size),
            _ => 0.0,
        })
        .sum();
    own + on
}

fn grey(bed: &TreeBed) -> usize {
    bed.render_frame()
        .layers
        .into_iter()
        .flat_map(|l| l.primitives)
        .filter(|p| matches!(p, RenderPrimitive::Polygon(p) if p.fill.as_ref().is_some_and(|f| f.color == DEAD)))
        .count()
}

#[test]
fn the_trees_grow_on_while_their_wood_hangs_as_they_would_at_rest() {
    let mut bed = grown();
    let mut twin = grown();
    bed.command(Command::PhysicalWood);
    let mut grown_so_far = 0;
    for _ in 0..(30.0 / WOOD_DT).round() as u32 {
        bed.advance(Duration::from_secs_f32(WOOD_DT));
        if bed.wood.work.grown == grown_so_far {
            continue;
        }
        grown_so_far = bed.wood.work.grown;
        // Before the next step, new wood starts where the wood it grew on
        // now ends, at rest on its hinge, moving as that wood moves there.
        let o = bed.plants.observe();
        for (&id, segment) in &bed.wood.living {
            let state = bed.world.connection(segment.hinge()).unwrap();
            if state.load != Default::default() {
                continue;
            }
            let (start, _) = ends(&bed, segment);
            let gap = (start - anchor(&bed, &o, id)).length();
            assert!(gap < 1e-4, "new wood {id} starts {gap} from its anchor");
            let turn = state.deflection.unwrap();
            assert!(turn.abs() < 1e-6, "new wood {id} turned {turn}");
            if let Some(p) = segment.parent {
                let carrier = bed.wood.living[&p].body();
                let at = bed.world.motion(segment.body()).unwrap().position;
                let (own, parent) = (
                    bed.world.velocity_at_point(segment.body(), at).unwrap(),
                    bed.world.velocity_at_point(carrier, at).unwrap(),
                );
                assert!((own - parent).length() < 1e-4, "{own:?} on {parent:?}");
            }
        }
    }
    let o = bed.plants.observe();
    assert!(o.tick > 1050, "{}", o.tick);
    let work = bed.wood.work;
    assert!(work.grown > 20 && work.reshaped > 100, "{work:?}");
    assert_eq!((work.died, work.refused, bed.wood.breaks), (0, 0, 0));

    // Its bodies keep step with its tissues: one segment for each piece of
    // wood, as wide as its wood and as heavy as it and what grows on it, on
    // a hinge of its own width, holding together where they join.
    let wood: BTreeSet<u64> = o
        .tissues
        .iter()
        .filter(|t| t.policy == tree::POLICY && t.role == Role::Wood)
        .map(|t| t.id)
        .collect();
    assert_eq!(
        bed.wood.living.keys().copied().collect::<BTreeSet<_>>(),
        wood
    );
    for (&id, segment) in &bed.wood.living {
        let t = o.tissues.iter().find(|t| t.id == id).unwrap();
        assert_eq!(segment.width, 0.5 * f32::from(t.size).sqrt());
        let mass = bed.world.body_mass(segment.body()).unwrap();
        let expected = weight(&o, id, segment);
        assert!(
            (mass - expected).abs() < 1e-4 * expected,
            "{id}: {mass} for {expected}"
        );
        let state = bed.world.connection(segment.hinge()).unwrap();
        let beam = state.connection.beam();
        assert_eq!(
            [beam.thickness, beam.height, beam.length],
            [segment.width, segment.width, segment.length]
        );
        assert!(state.load.utilization < 1.0, "{id}: {state:?}");
        let gap = (ends(&bed, segment).0 - anchor(&bed, &o, id)).length();
        assert!(gap < 0.02, "wood {id} parted {gap} from its anchor");
    }
    assert_eq!(bed.world.joint_count(), wood.len());
    assert_eq!(bed.world.identity_fault(), None);
    // Its lowest branch still hangs: nothing set it back upright.
    let droop = hinge(&bed, RIGHT_TIP).droop;
    assert!(droop > 0.15, "the lowest branch's tip droops {droop}");

    // Light passes living wood, whose tissues shade at their cells, so the
    // trees grew as they would have at rest.
    while twin.plants.observe().tick < o.tick {
        twin.tick();
    }
    assert_eq!(twin.plants.observe(), o);
}

#[test]
fn thickened_wood_keeps_the_rest_its_hinge_was_joined_at() {
    let mut bed = grown();
    bed.command(Command::PhysicalWood);
    hold(&mut bed, 8.0);
    // The trees' clock alone, so that only growth moves anything.
    let turns = |bed: &TreeBed| -> BTreeMap<u64, f32> {
        bed.wood
            .living
            .iter()
            .map(|(&id, s)| {
                (
                    id,
                    bed.world.connection(s.hinge()).unwrap().deflection.unwrap(),
                )
            })
            .collect()
    };
    let mut thickened = 0;
    while thickened == 0 {
        let (widths, before) = (
            bed.wood
                .living
                .iter()
                .map(|(&id, s)| (id, s.width))
                .collect::<BTreeMap<_, _>>(),
            turns(&bed),
        );
        bed.tick();
        let after = turns(&bed);
        for (id, width) in widths {
            let segment = bed.wood.living[&id];
            if segment.width == width {
                continue;
            }
            thickened += 1;
            // The same turn from the same rest, on a wider beam.
            assert_eq!(after[&id], before[&id], "wood {id}");
            let beam = bed
                .world
                .connection(segment.hinge())
                .unwrap()
                .connection
                .beam();
            assert_eq!(beam.thickness, segment.width);
            assert!(segment.width > width);
        }
        assert!(bed.plants.observe().tick < 650, "nothing thickened");
    }
    // Settled again, the thicker wood hangs no lower than before: it springs
    // up toward the rest it was joined at, not from where it hung.
    let droops = |bed: &TreeBed| -> BTreeMap<u64, f32> {
        bed.hinges().iter().map(|h| (h.tissue, h.droop)).collect()
    };
    let hung = droops(&bed);
    hold(&mut bed, 8.0);
    let settled = droops(&bed);
    for (id, droop) in &hung {
        assert!(
            settled[id] < droop + 0.01,
            "wood {id} dropped from {droop} to {}",
            settled[id]
        );
    }
}

#[test]
fn wood_the_tree_sheds_comes_away_with_the_motion_it_had() {
    let mut bed = grown();
    bed.command(Command::PhysicalWood);
    hold(&mut bed, 0.3);
    // The runtime drops the branch above the lowest on the grass's side at a
    // boundary, as a tree's shed does, while the branches swing down.
    let before = bed.plants.observe();
    let base = before
        .tissues
        .iter()
        .find(|t| t.cell == [13, 12])
        .unwrap()
        .id;
    let branch = beyond(&before, base);
    let wood: Vec<u64> = branch
        .iter()
        .copied()
        .filter(|id| bed.wood.living.contains_key(id))
        .collect();
    assert_eq!(wood.len(), 5);
    let motions: BTreeMap<PhysicsId, BodyMotion> = wood
        .iter()
        .map(|id| {
            let segment = bed.wood.living[id];
            (segment.entity, bed.world.motion(segment.body()).unwrap())
        })
        .collect();
    let falling = motions
        .values()
        .map(|m| m.linear_velocity.length())
        .fold(0.0, f32::max);
    assert!(falling > 0.1, "the branch is still: {falling}");
    let (grey_before, bodies) = (grey(&bed), bed.world.body_count());
    bed.plants.remove_many(&branch).unwrap();
    bed.sync_wood(&before);

    // The same bodies, as they moved, come away from the trunk and hold
    // together, drawn grey; the tree keeps none of its tissue or stores.
    assert_eq!(bed.wood.work.died, 5);
    assert_eq!(bed.world.body_count(), bodies);
    let dead: Vec<Segment> = bed.wood.dead.iter().map(|d| d.segment).collect();
    for segment in &dead {
        assert_eq!(
            bed.world.motion(segment.body()).unwrap(),
            motions[&segment.entity]
        );
    }
    let parted: Vec<bool> = dead
        .iter()
        .map(|s| bed.world.connection(s.hinge()).is_none())
        .collect();
    assert_eq!(parted, [true, false, false, false, false]);
    // Its wood and the needles on it.
    assert!(
        grey(&bed) > grey_before + 10,
        "{} after {grey_before}",
        grey(&bed)
    );
    let o = bed.plants.observe();
    assert!(!o.tissues.iter().any(|t| branch.contains(&t.id)));

    // It falls to the dirt, where it lies.
    hold(&mut bed, 6.0);
    let surface = bed.at([0, TOP_DIRT]).y + 0.5;
    for segment in &dead {
        let m = bed.world.motion(segment.body()).unwrap();
        assert!(
            m.position.y < surface + 1.0,
            "shed wood at {:?}",
            m.position
        );
        assert!(m.linear_velocity.length() < 0.05, "still moving: {m:?}");
    }
}

#[test]
fn a_tree_that_dies_stands_as_dead_wood_until_its_wood_is_cleared() {
    let mut bed = grown();
    let bodies = bed.world.body_count();
    bed.command(Command::PhysicalWood);
    let conifer: BTreeMap<u64, Segment> = bed.wood.living.clone();
    // Without water the grove dies, and its trees are cleared away.
    bed.command(Command::Water);
    bed.speed = MAX_SPEED;
    while bed.grove.trees.iter().any(|t| t.origin == TREE_ORIGIN) {
        run(&mut bed, 1.0);
        assert!(
            bed.plants.observe().tick < 1200,
            "the conifer outlived the drought"
        );
    }
    let o = bed.plants.observe();
    assert!(!o.tissues.iter().any(|t| conifer.contains_key(&t.id)));

    // Its wood stands where it died: the same bodies, on their hinges, the
    // trunk's foot still on the dirt, drawn grey.
    let dead: BTreeMap<PhysicsId, Segment> = bed
        .wood
        .dead
        .iter()
        .map(|d| (d.segment.entity, d.segment))
        .collect();
    assert_eq!(
        dead.keys().copied().collect::<BTreeSet<_>>(),
        conifer.values().map(|s| s.entity).collect::<BTreeSet<_>>()
    );
    assert!(
        dead.values()
            .all(|s| bed.world.connection(s.hinge()).is_some())
    );
    assert_eq!(bed.world.identity_fault(), None);
    let standing: Vec<Vec2> = dead
        .values()
        .map(|s| bed.world.motion(s.body()).unwrap().position)
        .collect();
    assert!(grey(&bed) >= dead.len());
    hold(&mut bed, 5.0);
    assert_eq!(bed.wood.breaks, 0);
    for (s, at) in dead.values().zip(&standing) {
        let now = bed.world.motion(s.body()).unwrap().position;
        assert!(
            (now - *at).length() < 0.05,
            "dead wood moved from {at:?} to {now:?}"
        );
    }
    assert!(bed.wood.living.is_empty());
    assert_eq!(
        bed.wood_summary(),
        format!(
            "Wood physical, none living | snow none | dead {}/{MAX_DEAD} | breaks 0",
            dead.len()
        )
    );

    // Replanted, the bed keeps none of it.
    bed.command(Command::PlantTree);
    assert!(bed.wood.dead.is_empty());
    assert_eq!(bed.world.body_count(), bodies);
}

#[test]
fn past_the_cap_the_wood_that_died_first_is_cleared() {
    let mut bed = grown();
    let bodies = bed.world.body_count();
    bed.wood.max_dead = 10;
    bed.command(Command::PhysicalWood);
    hold(&mut bed, 1.0);
    let living = bed.wood.living.len();
    let joints = bed.world.joint_count();
    // The lowest branch toward the grass, then the one across from it, six
    // segments each.
    bed.command(Command::CutBranch);
    let first: Vec<PhysicsId> = bed.wood.dead.iter().map(|d| d.segment.entity).collect();
    assert_eq!(first.len(), 6);
    let o = bed.plants.observe();
    bed.selected = o.tissues.iter().position(|t| t.cell == [11, 10]).unwrap();
    bed.command(Command::CutBranch);
    assert_eq!(bed.wood.dead.len(), 6);
    assert!(
        bed.wood
            .dead
            .iter()
            .all(|d| !first.contains(&d.segment.entity))
    );
    assert_eq!((bed.wood.work.died, bed.wood.work.cleared), (12, 6));
    assert_eq!(bed.world.body_count(), bodies + living - 6);
    // Each cut took a branch's root hinge, and the cleared branch took the
    // five within it, with nothing it named left behind.
    assert_eq!(bed.world.joint_count(), joints - 7);
    assert_eq!(bed.world.identity_fault(), None);
    assert!(bed.wood_summary().contains("| dead 6/10 |"));
}
