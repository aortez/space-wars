//! The trees' roots as what holds them. Its roots carry the grown conifer's
//! weight between them, and in a wind they hold it down to windward and up
//! to lee, most where they reach farthest. On all its roots it stands in a
//! gale that uproots it once its side roots are cut: it falls toward the
//! grass and dies there with its roots. A root that pulled out grips nothing
//! again, a dead tree stands on the roots it died with, roots that grow or
//! are shed gain and lose their anchors before the next step, and nothing
//! that falls out of the bed, rests or is replanted leaves a body behind.
use super::*;

/// The grown conifer, at rest.
pub(super) fn grown() -> TreeBed {
    let mut bed = TreeBed::new(42);
    while bed.plants.observe().tick < 600 {
        bed.tick();
    }
    bed
}

/// The grown conifer with its wood physical, hanging as it stood.
pub(super) fn standing() -> TreeBed {
    let mut bed = grown();
    bed.command(Command::PhysicalWood);
    hold(&mut bed, 8.0);
    bed
}

/// Move the bodies alone for `seconds`, the trees' clock held.
pub(super) fn hold(bed: &mut TreeBed, seconds: f32) {
    for _ in 0..(seconds / WOOD_DT).round() as u32 {
        bed.step_wood(WOOD_DT);
    }
}

/// Cut the root at `cell` away, with all that grew from it.
fn cut(bed: &mut TreeBed, cell: [i32; 2]) {
    let o = bed.plants.observe();
    bed.selected = o
        .tissues
        .iter()
        .position(|t| t.cell == cell && t.role == Role::Root)
        .unwrap();
    bed.command(Command::CutBranch);
}

/// Turn the wind up to the gale.
pub(super) fn gale(bed: &mut TreeBed) {
    while WINDS[bed.wood.wind.level].0 != "gale" {
        bed.command(Command::Wind);
    }
}

/// The most loaded anchor's load over its safe limit.
fn most(bed: &TreeBed) -> f32 {
    bed.anchors()
        .iter()
        .map(|a| a.utilization)
        .fold(0.0, f32::max)
}

/// The force each anchor's root puts on the dirt, by its cell: its pull
/// along the way the root grew and across it, turned to the bed's axes.
fn forces(bed: &TreeBed) -> BTreeMap<[i32; 2], Vec2> {
    let o = bed.plants.observe();
    bed.anchors()
        .iter()
        .map(|a| {
            let grew_from = o
                .connections
                .iter()
                .find(|&&(_, child)| child == a.tissue)
                .and_then(|&(p, _)| o.tissues.iter().find(|t| t.id == p));
            let along = grew_from.map_or(Vec2::new(0.0, -1.0), |p| {
                Vec2::new(
                    (a.cell[0] - p.cell[0]) as f32,
                    (a.cell[1] - p.cell[1]) as f32,
                )
            });
            let across = Vec2::new(-along.y, along.x);
            (a.cell, along * a.pull[0] + across * a.pull[1])
        })
        .collect()
}

/// What the conifer's wood and roots weigh together.
fn weight(bed: &TreeBed) -> f32 {
    let wood: f32 = bed
        .wood
        .living
        .values()
        .map(|s| bed.world.body_mass(s.body()).unwrap())
        .sum();
    let roots: f32 = bed
        .wood
        .plates
        .values()
        .map(|p| bed.world.body_mass(p.body()).unwrap())
        .sum();
    GRAVITY * (wood + roots)
}

/// The fastest any dead wood moves.
fn fastest(bed: &TreeBed) -> f32 {
    bed.wood
        .dead
        .iter()
        .map(|d| {
            bed.world
                .motion(d.segment.body())
                .unwrap()
                .linear_velocity
                .length()
        })
        .fold(0.0, f32::max)
}

/// The bed's longest line of text, in characters.
pub(super) fn longest_line(bed: &TreeBed) -> usize {
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

#[test]
fn its_roots_carry_the_grown_trees_weight_between_them() {
    let mut bed = grown();
    let o = bed.plants.observe();
    let roots: Vec<&Tissue> = o
        .tissues
        .iter()
        .filter(|t| t.policy == tree::POLICY && t.role == Role::Root)
        .collect();
    assert_eq!(roots.len(), 32);
    let (bodies, joints) = (bed.world.body_count(), bed.world.joint_count());
    bed.command(Command::PhysicalWood);
    // One body for its roots, anchored at each root, and its trunk's foot
    // hinged to that body.
    assert_eq!(bed.wood.plates.len(), 1);
    let plate = bed.wood.plates[&roots[0].organism].clone();
    assert_eq!(
        plate.anchors.keys().copied().collect::<Vec<_>>(),
        roots.iter().map(|t| t.id).collect::<Vec<_>>()
    );
    assert_eq!(bed.world.body_count(), bodies + bed.wood.living.len() + 1);
    assert_eq!(
        bed.world.joint_count(),
        joints + bed.wood.living.len() + roots.len()
    );
    let foot = bed
        .wood
        .living
        .values()
        .find(|s| s.parent.is_none())
        .unwrap();
    assert_eq!(
        bed.world.connection(foot.hinge()).unwrap().root_body,
        plate.body()
    );
    let mass = bed.world.body_mass(plate.body()).unwrap();
    assert!((mass - ROOT_MASS * 2.0 * 32.0).abs() < 1e-3, "{mass}");

    hold(&mut bed, 8.0);
    // The anchors carry all of it down into the dirt and none of it
    // sideways, each well within the dirt's grip.
    let weight = weight(&bed);
    let carried = forces(&bed)
        .values()
        .fold(Vec2::ZERO, |sum, force| sum + *force);
    assert!(
        (carried.y + weight).abs() < 0.01 * weight && carried.x.abs() < 0.01 * weight,
        "{carried:?} of {weight}"
    );
    let most = most(&bed);
    assert!((0.05..0.2).contains(&most), "an anchor carries {most}");
    // The dirt gives a little under it, and it stands upright.
    let moved = bed.plate_moved(&plate).unwrap();
    let sunk = bed.at(TREE_ORIGIN).y - moved.apply(bed.at(TREE_ORIGIN)).y;
    assert!((0.005..0.03).contains(&sunk), "sunk {sunk}");
    assert!(moved.turn().abs() < 0.002, "turned {}", moved.turn());
    assert_eq!((bed.wood.pulls, bed.wood.uprooted), (0, 0));
    assert!(
        bed.roots_summary()
            .starts_with("Roots on 32 anchors, most loaded 0."),
        "{}",
        bed.roots_summary()
    );
    assert!(
        bed.roots_summary()
            .ends_with("| wind calm | pulled 0 | uprooted 0")
    );
    assert_eq!(bed.world.identity_fault(), None);
}

#[test]
fn wind_leans_a_tree_on_roots_that_hold_it_down_to_windward_and_up_to_lee() {
    let mut bed = standing();
    let calm = (most(&bed), bed.leans()[&1]);
    let still = forces(&bed);
    let mut longest = 0;
    let mut leans = vec![calm.1];
    for name in ["breeze", "gale"] {
        bed.command(Command::Wind);
        assert_eq!(WINDS[bed.wood.wind.level].0, name);
        for _ in 0..(12.0 / WOOD_DT) as u32 {
            bed.step_wood(WOOD_DT);
            longest = longest.max(longest_line(&bed));
        }
        // It leans toward the grass and holds there.
        leans.push(bed.leans()[&1]);
        assert_eq!(
            (bed.wood.pulls, bed.wood.uprooted, bed.wood.breaks),
            (0, 0, 0)
        );
    }
    assert!(
        leans[1] < -0.002 && leans[2] < 2.0 * leans[1] && leans[2] > -0.03,
        "{leans:?}"
    );
    // In the gale the root that reaches farthest to lee and deepest carries
    // the most, several times what any carried in the calm, and still holds.
    let loaded = bed
        .anchors()
        .into_iter()
        .max_by(|a, b| a.utilization.total_cmp(&b.utilization))
        .unwrap();
    assert_eq!(loaded.cell, [15, 1]);
    assert!(
        loaded.utilization > 3.0 * calm.0 && loaded.utilization < 0.8,
        "{loaded:?} after {calm:?}"
    );
    // The tree's turning lifts its windward roots, which the dirt holds
    // down, and presses its lee roots into the dirt, which holds them up:
    // each farthest column of roots now carries more that way than it did.
    let blown = forces(&bed);
    let column = |forces: &BTreeMap<[i32; 2], Vec2>, x: i32| -> f32 {
        forces
            .iter()
            .filter(|(cell, _)| cell[0] == x)
            .map(|(_, force)| force.y)
            .sum()
    };
    let (windward, lee) = (column(&blown, 10), column(&blown, 15));
    assert!(
        windward > 0.0 && column(&still, 10) < 0.0,
        "windward roots press {windward}"
    );
    assert!(
        lee < 3.0 * column(&still, 15),
        "lee roots press {lee} after {}",
        column(&still, 15)
    );
    // Together they still carry its weight, and now the wind's push.
    let carried = blown.values().fold(Vec2::ZERO, |sum, force| sum + *force);
    let weight = weight(&bed);
    assert!((carried.y + weight).abs() < 0.01 * weight, "{carried:?}");
    assert!(carried.x > 0.2 * weight, "{carried:?} of {weight}");
    // The roots share the grove's line, which stays within the window.
    assert!(longest <= 192, "a line of {longest} characters");
    assert!(bed.roots_summary().contains("| wind gale |"));

    // The wind drops, and the tree stands as it stood.
    bed.command(Command::Wind);
    assert_eq!(WINDS[bed.wood.wind.level].0, "calm");
    hold(&mut bed, 12.0);
    assert_eq!(bed.wood.wind.push, 0.0);
    assert!((bed.leans()[&1] - calm.1).abs() < 0.001);
    assert!((most(&bed) - calm.0).abs() < 0.01);
}

#[test]
fn the_gale_that_all_its_roots_hold_uproots_a_tree_whose_side_roots_are_cut() {
    let mut bed = standing();
    let o = bed.plants.observe();
    let tree = o.tissues.iter().filter(|t| t.organism == 1).count();
    let (bodies, grass) = (bed.world.body_count(), o.tissues.len() - tree);
    let stood = bed.at(TREE_ORIGIN).x;
    // Each side root cut takes the anchors of all that grew from it, and
    // the roots that are left carry more.
    let calm = most(&bed);
    cut(&mut bed, [13, 7]);
    assert_eq!(bed.anchors().len(), 24);
    cut(&mut bed, [11, 7]);
    assert_eq!(bed.anchors().len(), 17);
    assert_eq!(bed.cuts, 2);
    hold(&mut bed, 8.0);
    let cut_calm = most(&bed);
    assert!(
        cut_calm > 1.5 * calm && cut_calm < 0.3,
        "{cut_calm} after {calm}"
    );
    assert_eq!(bed.wood.pulls, 0);

    // The same gale now pulls its roots out of the dirt one after another,
    // and it is uprooted.
    gale(&mut bed);
    let mut pulls = Vec::new();
    let mut steps = 0;
    while bed.wood.uprooted == 0 {
        bed.step_wood(WOOD_DT);
        steps += 1;
        if pulls.last() != Some(&bed.wood.pulls) {
            pulls.push(bed.wood.pulls);
        }
        assert!(steps < 600, "the gale left it standing");
    }
    assert!(pulls.len() > 5, "its roots let go together: {pulls:?}");
    assert_eq!(bed.wood.pulls, 17);
    // It has left the bed's plants, with nothing of the grass.
    let o = bed.plants.observe();
    assert!(!o.tissues.iter().any(|t| t.organism == 1));
    assert_eq!(o.tissues.len(), grass);
    // Its wood and roots are dead where they were, the same bodies.
    assert!(bed.wood.living.is_empty() && bed.wood.plates.is_empty());
    assert_eq!((bed.wood.dead.len(), bed.wood.dead_plates.len()), (54, 1));
    assert!(bed.wood.dead_plates[0].plate.anchors.is_empty());
    assert_eq!(bed.world.body_count(), bodies);
    assert_eq!(bed.anchors().len(), 0);
    assert!(bed.wood.pulled.is_empty());

    // The gale goes on pushing it until it has gone over. It falls toward
    // the grass and comes to rest on the dirt and on its own branches, its
    // roots torn out and turned with it.
    hold(&mut bed, 16.0);
    assert!(fastest(&bed) < 0.01, "still moving at {}", fastest(&bed));
    let surface = bed.at([0, TOP_DIRT]).y + 0.5;
    let lying: Vec<Vec2> = bed
        .wood
        .dead
        .iter()
        .map(|d| bed.world.motion(d.segment.body()).unwrap().position)
        .collect();
    let reach = lying.iter().map(|p| p.x).fold(f32::MIN, f32::max);
    assert!(reach > stood + 10.0, "it reaches {reach} from {stood}");
    assert!(lying.iter().all(|p| p.x > stood - 2.0));
    assert!(
        lying
            .iter()
            .all(|p| p.y > surface - 0.6 && p.y < surface + 8.0)
    );
    let turned = bed
        .plate_moved(&bed.wood.dead_plates[0].plate)
        .unwrap()
        .turn();
    assert!(turned < -FALLEN, "its roots turned {turned}");
    assert_eq!(bed.world.identity_fault(), None);
    assert_eq!(
        bed.roots_summary(),
        "No living roots hold | wind gale | pulled 17 | uprooted 1"
    );
    // At the next boundary the grove no longer has the tree. It counts
    // among the uprooted, not among those that died standing.
    bed.tick();
    assert!(bed.grove.trees.iter().all(|t| t.origin != TREE_ORIGIN));
    assert_eq!((bed.cleared, bed.wood.uprooted), (0, 1));
    assert_eq!(bed.wood.dead.len(), 54);
}

#[test]
fn one_side_root_fewer_still_holds_the_tree_in_the_gale() {
    for side in [[13, 7], [11, 7]] {
        let mut bed = standing();
        cut(&mut bed, side);
        hold(&mut bed, 8.0);
        gale(&mut bed);
        let mut peak = 0.0_f32;
        for _ in 0..(20.0 / WOOD_DT) as u32 {
            bed.step_wood(WOOD_DT);
            peak = peak.max(most(&bed));
        }
        // Harder pressed than on all its roots, it holds.
        assert!((0.7..1.0).contains(&peak), "{side:?} cut: {peak}");
        assert_eq!(
            (bed.wood.pulls, bed.wood.uprooted, bed.wood.breaks),
            (0, 0, 0)
        );
    }
}

#[test]
fn a_root_that_pulled_out_grips_nothing_again() {
    let mut bed = standing();
    let before = most(&bed);
    // The dirt lets go of the seed's root, as a load would make it.
    let o = bed.plants.observe();
    let seed = o.tissues.iter().find(|t| t.cell == TREE_ORIGIN).unwrap();
    let joint = bed.wood.plates[&1].anchors[&seed.id].joint;
    assert!(bed.pull(joint));
    assert!(!bed.pull(joint));
    assert_eq!(bed.anchors().len(), 31);
    assert_eq!((bed.wood.pulls, bed.wood.uprooted), (1, 0));
    assert!(bed.wood.pulled.contains(&seed.id));
    assert!(bed.world.connection(joint).is_none());
    hold(&mut bed, 6.0);
    // The rest take up what it carried.
    assert!(most(&bed) > before, "{} after {before}", most(&bed));
    bed.selected = o.tissues.iter().position(|t| t.id == seed.id).unwrap();
    let text = |bed: &TreeBed| -> Vec<String> {
        bed.render_frame()
            .layers
            .into_iter()
            .flat_map(|l| l.primitives)
            .filter_map(|p| match p {
                RenderPrimitive::Text(t) => Some(t.text),
                _ => None,
            })
            .collect()
    };
    assert!(
        text(&bed)
            .iter()
            .any(|l| l.ends_with("| pulled out of the dirt"))
    );
    // At rest and physical again, and as the tree grows on, it still grips
    // nothing, while every other root does.
    bed.command(Command::PhysicalWood);
    assert!(bed.wood.plates.is_empty());
    bed.command(Command::PhysicalWood);
    assert_eq!(bed.anchors().len(), 31);
    for _ in 0..5 {
        bed.tick();
    }
    let o = bed.plants.observe();
    let roots = o
        .tissues
        .iter()
        .filter(|t| t.organism == 1 && t.role == Role::Root);
    assert_eq!(bed.anchors().len(), roots.count() - 1);
    assert!(!bed.wood.plates[&1].anchors.contains_key(&seed.id));
    // Another root, selected, shows what its anchor carries.
    bed.selected = o.tissues.iter().position(|t| t.cell == [12, 6]).unwrap();
    hold(&mut bed, 1.0);
    assert!(text(&bed).iter().any(|l| l.contains("| anchor 0.")));
}

#[test]
fn a_dead_tree_stands_on_the_roots_it_died_with_and_leans_in_the_wind() {
    let mut bed = grown();
    bed.command(Command::PhysicalWood);
    let plate = bed.wood.plates[&1].clone();
    // Without water the grove dies, and its trees are cleared away.
    bed.command(Command::Water);
    bed.speed = MAX_SPEED;
    while bed.grove.trees.iter().any(|t| t.origin == TREE_ORIGIN) {
        for _ in 0..60 {
            bed.advance(Duration::from_secs_f32(WOOD_DT));
        }
        assert!(bed.plants.observe().tick < 1200);
    }
    bed.command(Command::Pause);
    // Its roots died with it, the same body, and hold it by every anchor
    // they had.
    assert!(bed.wood.plates.is_empty());
    let dead = bed
        .wood
        .dead_plates
        .iter()
        .find(|d| d.plate.entity == plate.entity)
        .unwrap()
        .plate
        .clone();
    assert_eq!(dead.anchors.len(), 32);
    assert!(
        dead.anchors
            .values()
            .all(|hold| bed.world.connection(hold.joint).is_some())
    );
    hold(&mut bed, 5.0);
    let upright = bed.plate_moved(&dead).unwrap().turn();
    assert!(upright.abs() < 0.003, "turned {upright}");
    // The gale leans the dead tree as it leaned the living one, and it
    // holds.
    gale(&mut bed);
    hold(&mut bed, 12.0);
    let leaning = bed.plate_moved(&dead).unwrap().turn();
    assert!((-0.03..-0.005).contains(&leaning), "turned {leaning}");
    assert_eq!((bed.wood.pulls, bed.wood.uprooted), (0, 0));
    assert_eq!(bed.world.identity_fault(), None);
    // Replanted, the bed keeps none of it.
    bed.command(Command::PlantTree);
    assert!(bed.wood.dead_plates.is_empty() && bed.wood.plates.is_empty());
}

#[test]
fn roots_gain_and_lose_their_anchors_as_the_trees_grow_and_shed_them() {
    let mut bed = TreeBed::new(42);
    while bed.plants.observe().tick < 300 {
        bed.tick();
    }
    bed.command(Command::PhysicalWood);
    let count = |bed: &TreeBed| {
        let o = bed.plants.observe();
        let sprouted: BTreeSet<u64> = o
            .tissues
            .iter()
            .filter(|t| t.policy == tree::POLICY && t.role == Role::Wood)
            .map(|t| t.organism)
            .collect();
        o.tissues
            .iter()
            .filter(|t| t.role == Role::Root && sprouted.contains(&t.organism))
            .count()
    };
    let young = count(&bed);
    assert_eq!(bed.anchors().len(), young);
    let mut mass = bed.world.body_mass(bed.wood.plates[&1].body()).unwrap();
    // As the conifer grows on, each new root grips before the next step, and
    // its plate weighs what its roots weigh. It grows several roots at a
    // boundary, when it runs short of water.
    let mut grew = 0;
    while bed.plants.observe().tick < 620 {
        bed.advance(Duration::from_secs_f32(WOOD_DT));
        let roots = count(&bed);
        assert_eq!(bed.anchors().len(), roots);
        let now = bed.world.body_mass(bed.wood.plates[&1].body()).unwrap();
        if now != mass {
            grew += 1;
            mass = now;
        }
    }
    assert!(count(&bed) > young + 8 && grew > 1, "{grew} from {young}");
    let o = bed.plants.observe();
    let roots = o
        .tissues
        .iter()
        .filter(|t| t.organism == 1 && t.role == Role::Root);
    assert!((mass - ROOT_MASS * 2.0 * roots.count() as f32).abs() < 1e-3);
    assert_eq!(
        (bed.wood.pulls, bed.wood.uprooted, bed.wood.breaks),
        (0, 0, 0)
    );
    assert!(most(&bed) < 0.2);
    assert_eq!(bed.world.identity_fault(), None);
    // A root cut at rest holds nothing once the wood is physical again.
    bed.command(Command::PhysicalWood);
    let joints = bed.world.joint_count();
    cut(&mut bed, [13, 7]);
    assert_eq!(bed.world.joint_count(), joints);
    bed.command(Command::PhysicalWood);
    assert_eq!(bed.anchors().len(), count(&bed));
    assert!(!bed.anchors().iter().any(|a| a.cell == [13, 7]));
}

#[test]
fn resting_takes_the_living_roots_and_the_wind_and_leaves_the_dead() {
    let mut bed = grown();
    let bodies = bed.world.body_count();
    bed.command(Command::Wind);
    // The wind makes the wood physical, as snow does.
    assert!(bed.wood.physical && bed.wood.plates.len() == 1);
    hold(&mut bed, 4.0);
    assert!(bed.wood.wind.push > 0.0);
    bed.command(Command::PhysicalWood);
    assert_eq!(bed.wood.wind, Wind::default());
    assert!(bed.wood.plates.is_empty());
    assert_eq!(
        (bed.world.body_count(), bed.world.joint_count()),
        (bodies, 0)
    );
    assert_eq!(bed.world.identity_fault(), None);
}

#[test]
fn wood_and_roots_that_fall_out_of_the_bed_are_cleared() {
    let mut bed = standing();
    cut(&mut bed, [13, 7]);
    cut(&mut bed, [11, 7]);
    gale(&mut bed);
    hold(&mut bed, 16.0);
    assert_eq!((bed.wood.uprooted, bed.wood.dead_plates.len()), (1, 1));
    let (bodies, dead) = (bed.world.body_count(), bed.wood.dead.len());
    // Its roots are cut loose from its trunk by hand, as nothing in the bed
    // does. Nothing holds them, the dirt least of all, and they fall out of
    // the bed.
    let foot = bed
        .wood
        .dead
        .iter()
        .map(|d| d.segment)
        .find(|s| s.parent.is_none())
        .unwrap();
    assert!(bed.world.remove_connection(foot.hinge()));
    hold(&mut bed, 4.0);
    assert!(bed.wood.dead_plates.is_empty());
    assert_eq!(bed.world.body_count(), bodies - 1);
    // So does the trunk's foot, which stood in the dirt, once it is cut
    // loose from the trunk above it.
    let next = bed
        .wood
        .dead
        .iter()
        .map(|d| d.segment)
        .find(|s| {
            bed.world
                .connection(s.hinge())
                .is_some_and(|c| c.root_body == foot.body())
        })
        .unwrap();
    assert!(bed.world.remove_connection(next.hinge()));
    hold(&mut bed, 4.0);
    assert_eq!(bed.wood.dead.len(), dead - 1);
    assert!(
        !bed.wood
            .dead
            .iter()
            .any(|d| d.segment.entity == foot.entity)
    );
    assert_eq!(bed.world.body_count(), bodies - 2);
    assert_eq!(bed.wood.work.cleared, 1);
    assert_eq!(bed.world.identity_fault(), None);
}
