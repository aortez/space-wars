//! Trenches and the dirt they cut loose. A trench through a root lays the
//! roots in it bare, and they let go of the dirt for good, while the dirt it
//! cuts away comes loose with the anchors in it. A root ball cut loose that
//! fixed ground would hold upright tips into its trench in a gale, and the
//! trench wall stops it so short that the conifer's top snaps. Once the gale
//! drops, the ball rights itself. Filling puts
//! the dirt back where nothing lies and grips no root that lost it. Trenches
//! through most of its roots leave too few to hold a tree up, and its own
//! weight uproots it. Through all of it the dirt keeps every cell, and each
//! anchor holds the dirt its root grips.
use super::roots_tests::{gale, grown, hold, longest_line, standing};
use super::*;

/// The cells of the bed's dirt, at the start: 32 columns, 8 rows.
const CELLS: usize = 256;

/// Select the root at `cell`.
fn select(bed: &mut TreeBed, cell: [i32; 2]) {
    bed.selected = bed
        .plants
        .observe()
        .tissues
        .iter()
        .position(|t| t.cell == cell && t.role == Role::Root)
        .unwrap();
}

/// The cells of dirt in the bed and in every loose block, with the cells
/// the trenches hold open.
fn conserved(bed: &TreeBed) -> usize {
    let dirt = |t: &Terrain| t.cells().iter().filter(|c| c.material == DIRT).count();
    dirt(&bed.terrain)
        + bed
            .dirt
            .loose
            .iter()
            .map(|b| dirt(&b.terrain))
            .sum::<usize>()
        + bed.dirt.dug.len()
}

/// Every anchor holds the dirt its root grips, and the world holds no joint
/// but the wood's hinges and the roots' anchors.
fn owned(bed: &TreeBed) {
    let living = bed.wood.plates.values();
    let dead = bed.wood.dead_plates.iter().map(|d| &d.plate);
    let mut anchors = 0;
    for plate in living.chain(dead) {
        for hold in plate.anchors.values() {
            let state = bed.world.connection(hold.joint).unwrap();
            assert_eq!(state.root_body, hold.dirt);
            assert_eq!(state.body, plate.body());
            anchors += 1;
        }
    }
    let hinges = bed
        .wood
        .living
        .values()
        .chain(bed.wood.dead.iter().map(|d| &d.segment))
        .filter(|s| bed.world.connection(s.hinge()).is_some())
        .count();
    assert_eq!(bed.world.joint_count(), anchors + hinges);
    assert_eq!(bed.world.identity_fault(), None);
}

/// The anchors that grip the bed's own dirt.
fn in_bed(bed: &TreeBed) -> usize {
    bed.anchors()
        .iter()
        .filter(|a| a.dirt == bed.assembly.body().entity)
        .count()
}

/// The living trunk's foot of the eldest tree.
fn foot(bed: &TreeBed) -> Segment {
    *bed.wood
        .living
        .values()
        .find(|s| s.parent.is_none())
        .unwrap()
}

/// The bed's lines of text, with the height of each.
fn placed(bed: &TreeBed) -> Vec<(String, f32)> {
    bed.render_frame()
        .layers
        .into_iter()
        .flat_map(|l| l.primitives)
        .filter_map(|p| match p {
            RenderPrimitive::Text(t) => Some((t.text, t.position.y)),
            _ => None,
        })
        .collect()
}

/// The bed's lines of text.
fn text(bed: &TreeBed) -> Vec<String> {
    placed(bed).into_iter().map(|(line, _)| line).collect()
}

#[test]
fn a_trench_lets_go_the_roots_it_lays_bare_and_the_dirt_it_cuts_away_takes_its_anchors() {
    let mut bed = standing();
    let o = bed.plants.observe();
    let holds = bed.wood.plates[&1].anchors.clone();
    let column: Vec<u64> = o
        .tissues
        .iter()
        .filter(|t| t.organism == 1 && t.role == Role::Root && t.cell[0] == 13)
        .map(|t| t.id)
        .collect();
    assert_eq!((holds.len(), column.len()), (32, 7));
    let lean = bed.leans()[&1];
    // A trench through a root to lee of the trunk takes the whole column of
    // dirt it grows in, top to bottom.
    select(&mut bed, [13, 4]);
    bed.command(Command::Dig);
    assert_eq!((bed.dirt.trenches, bed.dirt.dug.len()), (1, 8));
    assert!((0..=TOP_DIRT).all(|y| bed.dirt.dug.contains_key(&[13, y])));
    // Before the next step, the seven roots it lays bare let go of the dirt
    // for good.
    assert_eq!(bed.dirt.bared, 7);
    for id in &column {
        assert!(bed.wood.pulled.contains(id));
        assert!(bed.world.connection(holds[id].joint).is_none());
    }
    // The trunk's side of the bed is the smaller, and comes loose: thirteen
    // columns of dirt, with the anchors of the 21 roots in them, which hold
    // on as they held. The four roots that reach under the trench to lee
    // still grip the bed.
    let blocks = bed.blocks();
    assert_eq!(blocks.len(), 1);
    assert_eq!((blocks[0].cells, blocks[0].anchors), (104, 21));
    assert_eq!((bed.anchors().len(), in_bed(&bed)), (25, 4));
    let block = BodyId::new(blocks[0].entity, BodyRole::PRIMARY);
    for (id, hold) in bed.wood.plates[&1].anchors.iter() {
        assert_eq!(hold.joint, holds[id].joint);
        let dirt = if hold.dirt == block { "block" } else { "bed" };
        let cell = o.tissues.iter().find(|t| t.id == *id).unwrap().cell;
        assert_eq!(dirt, if cell[0] < 13 { "block" } else { "bed" }, "{cell:?}");
    }
    owned(&bed);
    assert_eq!(conserved(&bed), CELLS);
    // The block rests where it was, and the tree stands as it stood, its
    // foot passing through the dirt it stands in.
    hold(&mut bed, 4.0);
    let rested = bed.blocks()[0];
    assert!((rested.position - blocks[0].position).length() < 0.01);
    assert!(rested.angle.abs() < 0.001);
    assert!((bed.leans()[&1] - lean).abs() < 0.002, "{:?}", bed.leans());
    assert!(
        bed.world
            .contact_points(ColliderId::new(foot(&bed).entity, ColliderRole::PRIMARY, 0))
            .is_empty()
    );
    assert_eq!((bed.wood.pulls, bed.wood.uprooted), (0, 0));
    // The bared root says why it holds nothing, a gripping root what dirt
    // it holds, and the dirt has a line of its own after the grove's.
    select(&mut bed, [13, 4]);
    let lines = text(&bed);
    assert!(lines.iter().any(|l| l.ends_with("| laid bare by a trench")));
    select(&mut bed, [11, 4]);
    let lines = text(&bed);
    assert!(lines.iter().any(|l| l.ends_with("of safe in loose dirt")));
    let grove = lines.iter().position(|l| l.starts_with("Grove |")).unwrap();
    assert!(lines[grove + 1].starts_with("Dirt | trenches 1 | open 8 cells"));
    assert!(longest_line(&bed) <= 192);
    // The twelve lines sit closer, the lowest where the eleventh was, above
    // the sun.
    let heights: Vec<f32> = placed(&bed).into_iter().map(|(_, y)| y).collect();
    assert_eq!(heights.len(), 12);
    assert!(
        (heights[11] - (21.6 - 10.0 * 0.98)).abs() < 1.0e-4,
        "{heights:?}"
    );
}

#[test]
fn a_root_ball_cut_loose_tips_into_its_trench_in_a_gale_and_the_stop_snaps_the_trees_top() {
    let mut bed = standing();
    let lean = bed.leans()[&1];
    // Beside the roots, to windward first: the dirt beyond the trench comes
    // loose and holds no root.
    bed.command(Command::Dig);
    assert_eq!(bed.dirt.dug.keys().next(), Some(&[9, 0]));
    // Then to lee: the conifer's root ball comes loose, all six columns of
    // it with the anchors of all 32 roots.
    bed.command(Command::Dig);
    assert!(bed.dirt.dug.contains_key(&[16, 0]));
    let blocks = bed.blocks();
    let cut = |b: &Block| (b.cells, b.anchors);
    assert_eq!(
        blocks.iter().map(cut).collect::<Vec<_>>(),
        [(72, 0), (48, 32)]
    );
    assert_eq!((bed.dirt.bared, in_bed(&bed)), (0, 0));
    owned(&bed);
    let cut_loose = blocks[1].position;
    // In the calm it stands as fixed ground held it.
    hold(&mut bed, 4.0);
    assert!((bed.leans()[&1] - lean).abs() < 0.001, "{:?}", bed.leans());
    // A gale that leans the tree on fixed ground less than two degrees tips
    // the root ball into the trench to lee, until its top corner meets the
    // trench's far wall: one cell across, eight deep.
    gale(&mut bed);
    hold(&mut bed, 12.0);
    let blocks = bed.blocks();
    let wedged = -(1.0f32 / 8.0).asin();
    assert!(
        (blocks[1].angle - wedged).abs() < 0.002,
        "{:?}",
        blocks[1].angle.to_degrees()
    );
    assert!(blocks[0].angle.abs() < 0.001);
    // The tree leans with it, a little farther, and every root holds.
    assert!(
        bed.leans()[&1] < blocks[1].angle - 0.005,
        "{:?}",
        bed.leans()
    );
    assert_eq!((bed.anchors().len(), bed.wood.pulls), (32, 0));
    // The wall stops the root ball short, and the crown whips on: its top
    // breaks away, from ten cells above the dirt.
    assert_eq!(bed.wood.breaks, 1);
    let o = bed.plants.observe();
    let trunk = |y: i32| o.tissues.iter().any(|t| t.cell == [12, y]);
    assert!(trunk(16) && !trunk(17));
    owned(&bed);
    assert_eq!(conserved(&bed), CELLS);
    // Filled, the trench to windward closes. The one to lee stays open where
    // the root ball lies in it.
    bed.command(Command::Fill);
    assert_eq!(bed.dirt.filled, 8);
    assert!(bed.dirt.dug.keys().all(|c| c[0] == 16));
    assert_eq!(conserved(&bed), CELLS);
    // Once the gale drops, the root ball rights itself, on its own weight
    // and the tree's, back where it came loose, and the tree stands with it.
    // Filled again, the trench to lee closes around it too.
    while WINDS[bed.wood.wind.level].0 != "calm" {
        bed.command(Command::Wind);
    }
    hold(&mut bed, 6.0);
    let ball = bed.blocks()[1];
    assert!(ball.angle.abs() < 0.001, "{ball:?}");
    assert!((ball.position - cut_loose).length() < 0.01, "{ball:?}");
    assert!((bed.leans()[&1] - lean).abs() < 0.002, "{:?}", bed.leans());
    assert_eq!((bed.anchors().len(), bed.wood.pulls), (32, 0));
    let summary = bed.dirt_summary();
    assert!(summary.ends_with("most turned 0.0°"), "{summary}");
    bed.command(Command::Fill);
    assert_eq!((bed.dirt.filled, bed.dirt.dug.len()), (16, 0));
    assert_eq!(bed.blocks().len(), 2);
    owned(&bed);
    assert_eq!(conserved(&bed), CELLS);
}

#[test]
fn filling_puts_back_the_dirt_where_nothing_lies_and_grips_no_root_that_lost_it() {
    let mut bed = standing();
    select(&mut bed, [13, 4]);
    bed.command(Command::Dig);
    hold(&mut bed, 2.0);
    let bared = bed.wood.pulled.clone();
    assert_eq!(bared.len(), 7);
    bed.command(Command::Fill);
    // Every cell the trench took comes back but those the trunk's foot lies
    // in, and the dirt keeps every cell.
    let open: Vec<[i32; 2]> = bed.dirt.dug.keys().copied().collect();
    let foot = bed.lying(&foot(&bed)).unwrap();
    for c in &open {
        let cell = wood::Lying {
            center: bed.at(*c),
            angle: 0.0,
            half: Vec2::splat(0.5),
        };
        assert!(cell.overlaps(&foot), "{c:?}");
    }
    assert_eq!(bed.dirt.filled as usize + open.len(), 8);
    assert_eq!(conserved(&bed), CELLS);
    // A root laid bare says so even with its dirt back around it.
    assert!(!open.contains(&[13, 1]));
    select(&mut bed, [13, 1]);
    assert!(
        text(&bed)
            .iter()
            .any(|l| l.ends_with("| laid bare by a trench"))
    );
    assert_eq!(bed.dirt.bare_roots, bared);
    // The block it cut loose stays loose, and the roots it laid bare grip
    // nothing, as the tree grows on, while the roots that grow grip.
    for _ in 0..20 {
        bed.tick();
    }
    hold(&mut bed, 1.0);
    let o = bed.plants.observe();
    let roots: Vec<&Tissue> = o
        .tissues
        .iter()
        .filter(|t| t.organism == 1 && t.role == Role::Root)
        .collect();
    assert!(roots.len() > 32, "{}", roots.len());
    let anchored: BTreeSet<u64> = bed.anchors().iter().map(|a| a.tissue).collect();
    for t in &roots {
        assert_eq!(
            anchored.contains(&t.id),
            !bared.contains(&t.id),
            "{:?}",
            t.cell
        );
    }
    assert_eq!(bed.blocks().len(), 1);
    owned(&bed);
}

#[test]
fn trenches_through_its_roots_leave_too_few_to_hold_a_tree_up_and_its_weight_uproots_it() {
    let mut bed = standing();
    // From windward, a column at a time: each lays bare the seven roots in
    // it, the rest hold the tree up, and it leans farther toward the
    // trenches.
    let mut lean = bed.leans()[&1];
    for (x, left) in [(10, 25), (11, 18), (12, 11)] {
        select(&mut bed, [x, 1]);
        bed.command(Command::Dig);
        assert_eq!(bed.anchors().len(), left, "column {x}");
        hold(&mut bed, 2.0);
        assert_eq!((bed.wood.pulls, bed.wood.uprooted), (0, 0), "column {x}");
        assert!(bed.leans()[&1] > lean, "column {x}: {:?}", bed.leans());
        lean = bed.leans()[&1];
    }
    // The next leaves only the four roots that reach under it to lee. The
    // tree's own weight pulls them out of the dirt, and it falls.
    select(&mut bed, [13, 1]);
    bed.command(Command::Dig);
    assert_eq!(bed.anchors().len(), 4);
    hold(&mut bed, 2.0);
    assert_eq!((bed.wood.pulls, bed.wood.uprooted), (4, 1));
    assert!(bed.wood.living.is_empty());
    assert!(bed.wood.dead_plates[0].plate.anchors.is_empty());
    // Dirt put back in reattaches nothing.
    bed.command(Command::Fill);
    hold(&mut bed, 1.0);
    assert!(
        bed.wood
            .dead_plates
            .iter()
            .all(|d| d.plate.anchors.is_empty())
    );
    owned(&bed);
    assert_eq!(conserved(&bed), CELLS);
}

#[test]
fn digging_filling_resting_and_losing_loose_dirt_leave_no_anchor_behind() {
    // Digging at rest makes the wood physical first, as the wind does.
    let mut bed = grown();
    let bodies = bed.world.body_count();
    bed.command(Command::Dig);
    assert!(bed.wood.physical && bed.dirt.trenches == 1);
    bed.command(Command::Dig);
    hold(&mut bed, 2.0);
    owned(&bed);
    // At rest the living roots leave with their anchors, and the loose dirt
    // stays where it lies. Physical again, the roots grip it where they lie.
    bed.command(Command::PhysicalWood);
    assert!(bed.wood.plates.is_empty());
    assert_eq!(bed.blocks().len(), 2);
    owned(&bed);
    bed.command(Command::PhysicalWood);
    assert_eq!(bed.blocks()[1].anchors, 32);
    hold(&mut bed, 2.0);
    owned(&bed);
    // A loose block that falls out of the bed is cleared away, and the roots
    // that gripped it grip nothing again: the tree is uprooted.
    let ball = BodyId::new(bed.blocks()[1].entity, BodyRole::PRIMARY);
    assert!(bed.world.set_pose(ball, Vec2::new(-3.0, -40.0), 0.0, true));
    hold(&mut bed, 1.0);
    assert_eq!(bed.blocks().len(), 1);
    assert_eq!(bed.wood.uprooted, 1);
    assert!(
        bed.wood
            .dead_plates
            .iter()
            .all(|d| d.plate.anchors.is_empty())
    );
    owned(&bed);
    // Planted fresh, the bed has no trench and no loose dirt.
    bed.command(Command::PlantTree);
    assert_eq!((bed.dirt.trenches, bed.dirt.loose.len()), (0, 0));
    assert_eq!(bed.world.body_count(), bodies);
}
