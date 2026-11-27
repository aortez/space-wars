//! The tree bed's physical wood: the grown conifer sags under its own weight
//! and settles, snow bends a branch that springs back when it clears, heavy
//! snow breaks the branch away to fall onto the dirt, and nothing of the
//! wood's bodies outlives replanting.
use super::*;

/// The grown conifer, its wood made physical.
fn grown() -> TreeBed {
    let mut bed = TreeBed::new(42);
    while bed.plants.observe().tick < 600 {
        bed.tick();
    }
    bed
}

/// Let the bed run for `seconds` of displayed updates.
fn run(bed: &mut TreeBed, seconds: f32) {
    for _ in 0..(seconds / WOOD_DT).round() as u32 {
        bed.advance(Duration::from_secs_f32(WOOD_DT));
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
    run(bed, seconds - 1.0);
    let (mut least, mut most) = (f32::MAX, f32::MIN);
    for _ in 0..(1.0 / WOOD_DT).round() as u32 {
        run(bed, WOOD_DT);
        let droop = hinge(bed, tip).droop;
        least = least.min(droop);
        most = most.max(droop);
    }
    (least, most)
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
fn physical_wood_sags_under_its_own_weight_and_settles_while_growth_waits() {
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
    assert_eq!(bed.plants.observe().tick, o.tick, "growth went on");
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
    assert_eq!((bed.wood.breaks, bed.wood.fallen.len()), (0, 0));
    assert!(
        bed.wood_summary()
            .starts_with("Wood physical, growth waits | hinges")
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
    run(&mut bed, SNOW_SECONDS);
    assert!(bed.wood.snow.unwrap().share() > 0.999);
    let (least, most) = settle(&mut bed, 6.0, RIGHT_TIP);
    assert!(
        (1.8 * bare..2.4 * bare).contains(&least) && most - least < 0.01,
        "under light snow the tip droops {least}..{most}, bare {bare}"
    );
    let root = hinge(&bed, RIGHT_ROOT).utilization;
    assert!((0.5..0.8).contains(&root), "the root carries {root}");
    assert!(
        bed.wood_summary()
            .contains("snow light on wood at [18, 10]")
    );
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
    run(&mut bed, 8.0);
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
    while bed.wood.breaks == 0 {
        run(&mut bed, WOOD_DT);
        seconds += WOOD_DT;
        assert!(seconds < 2.0, "heavy snow broke nothing in 2 s");
    }
    // The branch's root broke, and all of it left the tree.
    assert_eq!(bed.wood.breaks, 1);
    assert_eq!(bed.wood.fallen.len(), branch.len());
    assert_eq!(bed.wood.living.len(), living - branch.len());
    let o = bed.plants.observe();
    assert!(!o.tissues.iter().any(|t| branch.contains(&t.cell)));
    assert!(!bed.wood.snow.unwrap().fastened);
    assert!(
        bed.wood_summary()
            .contains("snow heavy on the dirt, slid off fallen wood")
    );

    // It lands on the dirt, and the snow beside it, and they come to rest.
    run(&mut bed, 4.0);
    let surface = bed.at([0, TOP_DIRT]).y + 0.5;
    for fallen in &bed.wood.fallen {
        let m = bed.world.motion(fallen.segment.body()).unwrap();
        assert!(
            m.position.y > surface && m.position.y < surface + 1.0,
            "fallen wood at {:?} above the dirt at {surface}",
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
    run(&mut bed, 2.0);
    let living = bed.wood.living.len();
    bed.command(Command::CutBranch);
    assert_eq!((bed.cuts, bed.wood.breaks), (1, 0));
    assert_eq!(bed.wood.fallen.len(), 6);
    assert_eq!(bed.wood.living.len(), living - 6);
    run(&mut bed, 3.0);
    let lying: Vec<Vec2> = bed
        .wood
        .fallen
        .iter()
        .map(|f| bed.world.motion(f.segment.body()).unwrap().position)
        .collect();
    assert!(lying.iter().all(|p| p.y < bed.at([0, TOP_DIRT]).y + 1.5));

    // Resting the wood takes its living bodies away, and the tree grows on
    // from its own clock. The fallen wood stays where it lies.
    let tick = bed.plants.observe().tick;
    bed.command(Command::PhysicalWood);
    assert!(!bed.wood.physical && bed.wood.living.is_empty());
    assert_eq!(bed.world.body_count(), bodies + 6);
    run(&mut bed, 1.0);
    assert!(bed.plants.observe().tick > tick);
    assert_eq!(bed.wood.fallen.len(), 6);
    assert!(
        bed.wood_summary()
            .starts_with("Wood at rest while it grows")
    );

    // Replanted, the bed keeps none of it.
    bed.command(Command::LightSnow);
    assert!(bed.wood.physical && bed.wood.snow.is_some());
    bed.command(Command::PlantTree);
    assert!(!bed.wood.physical && bed.wood.snow.is_none());
    assert!(bed.wood.living.is_empty() && bed.wood.fallen.is_empty());
    assert_eq!(bed.world.body_count(), bodies);
}
