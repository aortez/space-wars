//! A connection that bends turns its branch by the law's elastic rotation for
//! the moment it carries, settles back when a load is lifted, bends less when
//! thicker, holds a stone dropped low but breaks when it falls from high, and
//! severs under an overload as a weld does, on the same step after a save.
use super::connection::{BRANCH, DT, SCALES, Scale, TRUNK, TWIN, beam, body, weld_id, world};
use super::*;
use crate::world::connection::hinge_stiffness;
use engine_material::{Beam, Connection, ConnectionLoad, FailureMode, Material};

const GROUND: PhysicsId = PhysicsId::new(4);
const STONE: PhysicsId = PhysicsId::new(5);

/// Wood's damping: a fifth of the stored energy lost each cycle.
const WOOD_DAMPING: f32 = 0.2;

/// A branch four units long and half a unit high whose end meets the trunk's
/// side at mid-height, to the right for `side` 1 and the left for -1. It
/// never sleeps, so that it settles all the way.
fn grow(world: &mut PhysicsWorld, scale: Scale, entity: PhysicsId, side: f32) {
    assert!(world.insert_body(
        body(entity),
        BodySpec {
            position: Vec2::new(side * 2.5 * scale.k, 0.0),
            can_sleep: false,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            2.0 * scale.k,
            0.25 * scale.k,
        )],
    ));
}

fn material(stiffness: f32, strength: f32, toughness: f32) -> Material {
    Material {
        name: "test",
        density: 1.0,
        stiffness,
        strength,
        toughness,
        damping: WOOD_DAMPING,
        mode: FailureMode::Brittle,
    }
}

/// How far a settled hinge may turn beyond its spring's balance, as a share
/// of the turn. The solver takes the spring's row before the two locked rows
/// at the root in each substep, so a hinge settles a little past the balance:
/// by a few thousandths of the turn with four substeps or more, and by about
/// three hundredths with one. The moment it reports is the load it holds
/// either way.
fn sag_tolerance(iterations: usize) -> f32 {
    if iterations < 4 { 0.04 } else { 0.006 }
}

/// A branch's weight and the moment it puts on its root while it lies level,
/// at its center two units out.
fn weight_and_moment(world: &PhysicsWorld, scale: Scale, entity: PhysicsId) -> (f32, f32) {
    let weight = world.body_mass(body(entity)).unwrap() * scale.g;
    (weight, weight * 2.0 * scale.k)
}

/// The stiffness that turns the branch's beam by `turn` under the moment its
/// weight puts on its root while level.
fn stiffness_for(world: &PhysicsWorld, scale: Scale, beam: Beam, turn: f32) -> f32 {
    let (_, moment) = weight_and_moment(world, scale, BRANCH);
    moment * beam.length / (turn * beam.second_moment())
}

/// The turn at which a hinge of `stiffness` holds a weight whose center is
/// `reach` out along the branch: `stiffness * turn = weight * reach * cos(turn)`.
fn balance(stiffness: f32, weight: f32, reach: f32) -> f32 {
    let mut turn = 0.0_f32;
    for _ in 0..64 {
        turn = weight * reach * turn.cos() / stiffness;
    }
    turn
}

fn hinge(
    world: &mut PhysicsWorld,
    scale: Scale,
    entity: PhysicsId,
    side: f32,
    connection: Connection,
) {
    world
        .insert_connection(
            weld_id(entity),
            ConnectionSpec {
                root_body: body(TRUNK),
                body: body(entity),
                root: Vec2::new(side * 0.5 * scale.k, 0.0),
                axis: Vec2::new(side, 0.0),
                connection,
                bends: true,
            },
        )
        .unwrap();
}

/// A world with the branch on a hinge that sags `turn` under its own weight,
/// at a sixteenth of its safe stress, so that even a swing under several
/// times its weight stays safe.
fn sagging(scale: Scale, iterations: usize, beam: Beam, turn: f32) -> (PhysicsWorld, f32) {
    let mut world = world(scale, iterations);
    grow(&mut world, scale, BRANCH, 1.0);
    let (_, moment) = weight_and_moment(&world, scale, BRANCH);
    let stiffness = stiffness_for(&world, scale, beam, turn);
    let strength = 16.0 * moment / beam.section_modulus();
    let connection = Connection::new(material(stiffness, strength, 1.0), beam).unwrap();
    hinge(&mut world, scale, BRANCH, 1.0, connection);
    (world, hinge_stiffness(&connection))
}

fn run(world: &mut PhysicsWorld, seconds: f32, rate: f32) {
    for _ in 0..(seconds * rate).round() as usize {
        world.step(1.0 / rate);
    }
}

fn deflection(world: &PhysicsWorld, entity: PhysicsId) -> f32 {
    world
        .connection(weld_id(entity))
        .unwrap()
        .deflection
        .unwrap()
}

#[test]
fn a_bending_branch_sags_by_the_laws_elastic_rotation_and_costs_nothing() {
    for scale in SCALES {
        for (rate, iterations) in [(60.0, 4), (240.0, 4), (60.0, 1), (60.0, 8)] {
            let (mut world, stiffness) = sagging(scale, iterations, beam(scale), 0.05);
            let (weight, _) = weight_and_moment(&world, scale, BRANCH);
            let turn = balance(stiffness, weight, 2.0 * scale.k);
            let case = format!("g {} at {rate} Hz, {iterations} iterations", scale.g);
            let mut lowest = 0.0_f32;
            for _ in 0..(60.0 * rate) as usize {
                world.step(1.0 / rate);
                let state = world.connection(weld_id(BRANCH)).unwrap();
                assert_eq!(state.load.charged, 0.0, "{case}: {state:?}");
                lowest = lowest.min(state.deflection.unwrap());
            }
            // Released level, it swings below its balance and back up, and
            // settles there. The branch reaches right, so it turns clockwise.
            assert!(lowest < -1.5 * turn, "{case}: lowest {lowest} for {turn}");
            let state = world.connection(weld_id(BRANCH)).unwrap();
            let sag = state.deflection.unwrap();
            assert!(
                (sag + turn).abs() < sag_tolerance(iterations) * turn,
                "{case}: {sag} for {turn}"
            );
            // The spring holds the weight at its lever arm as it now lies.
            let reach = world.motion(body(BRANCH)).unwrap().position.x - 0.5 * scale.k;
            let moment = weight * reach;
            let load = state.load;
            assert!(
                (load.moment + moment).abs() < 2.0e-3 * moment,
                "{case}: {load:?} for {moment}"
            );
            // The beam's frame turns with the fixed trunk, so the weight
            // presses straight across it.
            assert!(
                (load.shear + weight).abs() < 2.0e-3 * weight,
                "{case}: {load:?}"
            );
            assert!(load.axial.abs() < 2.0e-3 * weight, "{case}: {load:?}");
            let region = state.connection.region();
            assert_eq!(region.remaining_precise(), f64::from(region.capacity()));
            assert_eq!(state.failure, None);
        }
    }
}

#[test]
fn a_bent_branch_settles_back_when_its_load_is_lifted() {
    for scale in SCALES {
        let (mut world, stiffness) = sagging(scale, 4, beam(scale), 0.05);
        let (weight, _) = weight_and_moment(&world, scale, BRANCH);
        let reach = 2.0 * scale.k;
        run(&mut world, 20.0, 60.0);
        let rest = deflection(&world, BRANCH);
        let expected = balance(stiffness, weight, reach);
        assert!((rest + expected).abs() < sag_tolerance(4) * expected);
        // Three times its weight bends it about three times as far, within
        // the safe stress, for nothing.
        assert!(world.set_gravity_scale(body(BRANCH), 3.0, true));
        run(&mut world, 20.0, 60.0);
        let loaded = deflection(&world, BRANCH);
        let expected = balance(stiffness, 3.0 * weight, reach);
        assert!(
            (loaded + expected).abs() < sag_tolerance(4) * expected,
            "g {}: {loaded} for {expected}",
            scale.g
        );
        assert!(loaded < 2.9 * rest, "g {}: {loaded} and {rest}", scale.g);
        // Lifted, it springs back past where it rested and settles there.
        assert!(world.set_gravity_scale(body(BRANCH), 1.0, true));
        let mut highest = f32::MIN;
        for step in 0..(20.0 * 60.0) as usize {
            world.step(DT);
            let state = world.connection(weld_id(BRANCH)).unwrap();
            assert_eq!(state.load.charged, 0.0, "step {step}: {state:?}");
            highest = highest.max(state.deflection.unwrap());
        }
        assert!(highest > 0.5 * rest, "g {}: highest {highest}", scale.g);
        let settled = deflection(&world, BRANCH);
        assert!(
            (settled - rest).abs() < 2.0e-3 * rest.abs(),
            "g {}: {settled} after resting at {rest}",
            scale.g
        );
        let region = *world
            .connection(weld_id(BRANCH))
            .unwrap()
            .connection
            .region();
        assert_eq!(region.remaining_precise(), f64::from(region.capacity()));
    }
}

#[test]
fn a_thicker_branch_bends_less_by_the_cube_of_its_height() {
    for scale in SCALES {
        let thin = beam(scale);
        let thick = Beam::new(thin.thickness, 2.0 * thin.height, thin.length);
        let (mut slender, _) = sagging(scale, 4, thin, 0.08);
        // The same material and branch, with a beam twice as deep.
        let state = slender.connection(weld_id(BRANCH)).unwrap();
        let mut stout = world(scale, 4);
        grow(&mut stout, scale, BRANCH, 1.0);
        let connection = Connection::new(state.connection.region().material(), thick).unwrap();
        hinge(&mut stout, scale, BRANCH, 1.0, connection);
        run(&mut slender, 20.0, 60.0);
        run(&mut stout, 20.0, 60.0);
        let (weight, _) = weight_and_moment(&slender, scale, BRANCH);
        let reach = 2.0 * scale.k;
        let (thin_turn, thick_turn) = (deflection(&slender, BRANCH), deflection(&stout, BRANCH));
        let (thin_expected, thick_expected) = (
            balance(hinge_stiffness(&state.connection), weight, reach),
            balance(hinge_stiffness(&connection), weight, reach),
        );
        let ratio = thin_turn / thick_turn;
        let expected = thin_expected / thick_expected;
        assert!(
            (ratio - expected).abs() < 2.0 * sag_tolerance(4) * expected,
            "g {}: {ratio} for {expected}: {thin_turn} for {thin_expected}, {thick_turn} for {thick_expected}",
            scale.g
        );
        assert!(ratio > 7.5, "g {}: {ratio}", scale.g);
        // A rigid weld of the thin beam barely turns at all.
        let mut welded = world(scale, 4);
        grow(&mut welded, scale, BRANCH, 1.0);
        welded
            .insert_connection(
                weld_id(BRANCH),
                ConnectionSpec {
                    root_body: body(TRUNK),
                    body: body(BRANCH),
                    root: Vec2::new(0.5 * scale.k, 0.0),
                    axis: Vec2::X,
                    connection: state.connection,
                    bends: false,
                },
            )
            .unwrap();
        run(&mut welded, 2.0, 60.0);
        let give = deflection(&welded, BRANCH).abs();
        assert!(
            give < 1.0e-3 * deflection(&slender, BRANCH).abs(),
            "g {}: {give}",
            scale.g
        );
    }
}

#[test]
fn a_branch_bends_toward_its_load() {
    for scale in SCALES {
        let (mut world, _) = sagging(scale, 4, beam(scale), 0.05);
        // A twin on the trunk's left, with the same hinge.
        grow(&mut world, scale, TWIN, -1.0);
        let connection = world.connection(weld_id(BRANCH)).unwrap().connection;
        hinge(&mut world, scale, TWIN, -1.0, connection);
        run(&mut world, 20.0, 60.0);
        let right = world.connection(weld_id(BRANCH)).unwrap();
        let left = world.connection(weld_id(TWIN)).unwrap();
        // Each droops away from the trunk: the right one clockwise and the
        // left one counterclockwise, by the same amount.
        let (r, l) = (right.deflection.unwrap(), left.deflection.unwrap());
        assert!(r < 0.0 && l > 0.0, "g {}: {r} {l}", scale.g);
        assert!((r + l).abs() < 1.0e-3 * l, "g {}: {r} {l}", scale.g);
        assert!(right.load.moment < 0.0 && left.load.moment > 0.0);
        // A load lifting the right branch bends it the other way.
        assert!(world.set_gravity_scale(body(BRANCH), -1.0, true));
        run(&mut world, 20.0, 60.0);
        let lifted = world.connection(weld_id(BRANCH)).unwrap();
        assert!(
            (lifted.deflection.unwrap() + r).abs() < 2.0e-3 * l,
            "g {}: {lifted:?}",
            scale.g
        );
        assert!(lifted.load.moment > 0.0, "g {}: {lifted:?}", scale.g);
        assert!((deflection(&world, TWIN) - l).abs() < 1.0e-3 * l);
    }
}

#[test]
fn each_hinge_of_a_chain_carries_the_weight_beyond_it_where_it_now_lies() {
    for scale in SCALES {
        let mut world = world(scale, 4);
        grow(&mut world, scale, BRANCH, 1.0);
        let beam = beam(scale);
        let (_, moment) = weight_and_moment(&world, scale, BRANCH);
        // Damped nearly critically, since the pair's slower swing would
        // outlast wood's damping, and strong enough for both branches.
        let connection = Connection::new(
            Material {
                damping: 0.99,
                ..material(
                    stiffness_for(&world, scale, beam, 0.03),
                    16.0 * moment / beam.section_modulus(),
                    1.0,
                )
            },
            beam,
        )
        .unwrap();
        hinge(&mut world, scale, BRANCH, 1.0, connection);
        // The outer branch grows from the inner one's tip, so its hinge's
        // root moves and turns.
        assert!(world.insert_body(
            body(TWIN),
            BodySpec {
                position: Vec2::new(6.5 * scale.k, 0.0),
                can_sleep: false,
                ..BodySpec::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(TWIN, ColliderRole::PRIMARY, 0),
                2.0 * scale.k,
                0.25 * scale.k,
            )],
        ));
        world
            .insert_connection(
                weld_id(TWIN),
                ConnectionSpec {
                    root_body: body(BRANCH),
                    body: body(TWIN),
                    root: Vec2::new(4.5 * scale.k, 0.0),
                    axis: Vec2::X,
                    connection,
                    bends: true,
                },
            )
            .unwrap();
        run(&mut world, 30.0, 60.0);
        let (inner, _) = weight_and_moment(&world, scale, BRANCH);
        let (outer, _) = weight_and_moment(&world, scale, TWIN);
        let at = |entity| world.motion(body(entity)).unwrap();
        // The tip hinge sits at the inner branch's end, four units along it.
        let inner_motion = at(BRANCH);
        let tip = inner_motion.position
            + Vec2::new(inner_motion.angle.cos(), inner_motion.angle.sin()) * 2.0 * scale.k;
        let root_x = 0.5 * scale.k;
        let outer_x = at(TWIN).position.x;
        let tip_moment = outer * (outer_x - tip.x);
        let root_moment = inner * (inner_motion.position.x - root_x) + outer * (outer_x - root_x);
        let tip_load = world.connection(weld_id(TWIN)).unwrap();
        let root_load = world.connection(weld_id(BRANCH)).unwrap();
        let case = format!(
            "g {}: tip {} for {tip_moment}, root {} for {root_moment}, turns {:?} {:?}, motion {:?} {:?}",
            scale.g,
            tip_load.load.moment,
            root_load.load.moment,
            root_load.deflection,
            tip_load.deflection,
            at(BRANCH),
            at(TWIN)
        );
        assert!(
            (tip_load.load.moment + tip_moment).abs() < 2.0e-3 * tip_moment,
            "{case}"
        );
        assert!(
            (root_load.load.moment + root_moment).abs() < 2.0e-3 * root_moment,
            "{case}"
        );
        // Both turn clockwise, the outer one less, since it carries less.
        let (root_turn, tip_turn) = (root_load.deflection.unwrap(), tip_load.deflection.unwrap());
        assert!(root_turn < tip_turn && tip_turn < 0.0, "{case}");
    }
}

/// The law's charge for one step of `moment` alone, uncapped by capacity.
fn charge_per_step(material: Material, beam: Beam, moment: f32) -> f32 {
    let probe = Connection::new(
        Material {
            toughness: 1.0e15,
            ..material
        },
        beam,
    )
    .unwrap();
    engine_material::connection(
        &probe,
        ConnectionLoad {
            moment,
            axial: 0.0,
            duration: DT,
            measured_plastic_rotation: None,
            measured_plastic_extension: None,
        },
    )
    .unwrap()
    .consumed[0]
}

/// A branch on a hinge at twice its safe stress whose capacity lasts about
/// `steps` steps of its weight, above fixed ground.
fn overloaded(scale: Scale, steps: f32) -> PhysicsWorld {
    let mut world = world(scale, 4);
    assert!(world.insert_body(
        body(GROUND),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(4.0 * scale.k, -6.0 * scale.k),
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(GROUND, ColliderRole::PRIMARY, 0),
            8.0 * scale.k,
            0.5 * scale.k,
        )],
    ));
    grow(&mut world, scale, BRANCH, 1.0);
    let beam = beam(scale);
    let (_, moment) = weight_and_moment(&world, scale, BRANCH);
    let strength = 0.5 * moment / beam.section_modulus();
    let probe = material(stiffness_for(&world, scale, beam, 0.05), strength, 1.0);
    let toughness = steps * charge_per_step(probe, beam, moment) / beam.volume();
    let connection = Connection::new(Material { toughness, ..probe }, beam).unwrap();
    hinge(&mut world, scale, BRANCH, 1.0, connection);
    world
}

#[test]
fn an_overloaded_hinge_severs_once_and_its_branch_falls_onto_the_ground() {
    for scale in SCALES {
        let mut world = overloaded(scale, 240.0);
        let capacity = f64::from(
            world
                .connection(weld_id(BRANCH))
                .unwrap()
                .connection
                .region()
                .capacity(),
        );
        let mut charged = 0.0_f64;
        let mut severed = None;
        for step in 1..=600_u32 {
            world.step(DT);
            let state = world.connection(weld_id(BRANCH)).unwrap();
            if severed.is_none() {
                charged += f64::from(state.load.charged);
            }
            if world.severed_connections().is_empty() {
                continue;
            }
            assert_eq!(world.severed_connections(), [weld_id(BRANCH)]);
            assert_eq!(severed, None, "severed once");
            severed = Some(step);
            assert!(state.connection.failed());
            assert!(state.failure.is_some_and(|f| f.excess >= 0.0));
            assert!(state.load.utilization > 1.0, "{state:?}");
            assert_eq!(state.deflection, None);
            assert!(!world.contains_joint(weld_id(BRANCH)));
        }
        let step = severed.expect("the overload exhausts the hinge");
        // It swings while overloaded, so it lasts only about as long as a
        // steady load would.
        assert!(step.abs_diff(240) <= 24, "g {}: step {step}", scale.g);
        assert!(
            (charged - capacity).abs() <= 1.0e-5 * capacity,
            "{charged} of {capacity}"
        );
        // Free of the hinge, it falls and comes to rest on the ground, its
        // half-unit height above the ground's top face.
        let motion = world.motion(body(BRANCH)).unwrap();
        let resting = -5.5 * scale.k + 0.25 * scale.k;
        assert!(
            (motion.position.y - resting).abs() < 0.05 * scale.k,
            "g {}: {motion:?}",
            scale.g
        );
        assert!(motion.angle.sin().abs() < 0.05, "g {}: {motion:?}", scale.g);
        assert!(motion.linear_velocity.length() < 0.05 * scale.k);
        assert_eq!(world.identity_fault(), None);
    }
}

/// What a host sees of the branch and its hinge.
fn observed(world: &PhysicsWorld) -> impl PartialEq + std::fmt::Debug {
    let state = world.connection(weld_id(BRANCH)).unwrap();
    let region = state.connection.region();
    (
        world.motion(body(BRANCH)),
        state.load,
        state.failure,
        state.deflection,
        region.remaining_precise(),
        region.material().identity(),
        world.severed_connections().to_vec(),
    )
}

#[test]
fn a_restored_or_cloned_bending_world_swings_and_severs_exactly_as_the_original() {
    let scale = SCALES[0];
    let mut world = overloaded(scale, 90.0);
    for _ in 0..40 {
        world.step(DT);
    }
    let bytes = world.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
    let mut cloned = world.clone();
    let mut severed = 0;
    for step in 0..120 {
        world.step(DT);
        restored.step(DT);
        cloned.step(DT);
        assert_eq!(observed(&restored), observed(&world), "step {step}");
        assert_eq!(observed(&cloned), observed(&world), "step {step}");
        severed += world.severed_connections().len();
    }
    assert_eq!(severed, 1);
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        world.snapshot_bytes().unwrap()
    );
}

/// A square stone half a unit wide, of `mass`, dropped from `height` units
/// above the level branch's top face at 3.5 units out from its root.
fn drop_stone(world: &mut PhysicsWorld, scale: Scale, mass: f32, height: f32) {
    let half = 0.25 * scale.k;
    let mut stone =
        ColliderSpec::cuboid(ColliderId::new(STONE, ColliderRole::PRIMARY, 0), half, half);
    stone.friction = 1.0;
    stone.mass_properties = Some(ColliderMassProperties {
        center: Vec2::ZERO,
        mass,
        inertia: mass * 2.0 * half * half / 3.0,
    });
    assert!(world.insert_body(
        body(STONE),
        BodySpec {
            position: Vec2::new(4.0 * scale.k, (0.25 + height) * scale.k + half),
            can_sleep: false,
            ..BodySpec::default()
        },
        &[stone],
    ));
}

#[test]
fn a_hinge_holds_a_stone_dropped_low_but_breaks_when_it_falls_from_high() {
    for scale in SCALES {
        for height in [0.25, 2.0] {
            let mut world = world(scale, 4);
            grow(&mut world, scale, BRANCH, 1.0);
            let beam = beam(scale);
            let (weight, moment) = weight_and_moment(&world, scale, BRANCH);
            // Safe to eight times the branch's own moment, so it can bend
            // about 0.16 before it yields, as brittle wood of its shape bends
            // a few degrees. It has almost no capacity for an overload.
            let connection = Connection::new(
                material(
                    stiffness_for(&world, scale, beam, 0.02),
                    8.0 * moment / beam.section_modulus(),
                    1.0e-6,
                ),
                beam,
            )
            .unwrap();
            hinge(&mut world, scale, BRANCH, 1.0, connection);
            run(&mut world, 60.0, 60.0);
            let rest = deflection(&world, BRANCH);
            // A stone as heavy as the branch.
            let mass = world.body_mass(body(BRANCH)).unwrap();
            drop_stone(&mut world, scale, mass, height);
            let (mut peak, mut charged, mut severed) = (0.0_f32, 0.0, None);
            for step in 0..(30.0 * 60.0) as usize {
                world.step(DT);
                let load = world.connection(weld_id(BRANCH)).unwrap().load;
                peak = peak.max(load.utilization);
                charged += load.charged;
                if !world.severed_connections().is_empty() {
                    severed = Some(step);
                    break;
                }
            }
            let case = format!("g {}, stone from {height}", scale.g);
            if height > 1.0 {
                // From higher, the stone strikes hard enough to swing the
                // branch past its safe stress, and with no capacity to spare
                // the hinge gives at once.
                assert!(
                    peak > 1.0 && severed.is_some(),
                    "{case}: {peak} {severed:?}"
                );
                continue;
            }
            // Dropped low, the stone swings the branch further but within its
            // safe stress, then rests on it.
            assert!(
                peak < 1.0 && severed.is_none(),
                "{case}: {peak} {severed:?}"
            );
            assert_eq!(charged, 0.0, "{case}");
            let stone = world.motion(body(STONE)).unwrap();
            let state = world.connection(weld_id(BRANCH)).unwrap();
            let reach = stone.position.x - 0.5 * scale.k;
            let branch_reach = world.motion(body(BRANCH)).unwrap().position.x - 0.5 * scale.k;
            let carried = weight * branch_reach + mass * scale.g * reach;
            assert!(
                (state.load.moment + carried).abs() < 1.0e-3 * carried,
                "{case}: {state:?} for {carried}"
            );
            // Resting, it loads the hinge to a third of its safe stress, so
            // only the strike from higher can break it.
            assert!(
                state.load.utilization < 0.5 && peak > 2.0 * state.load.utilization,
                "{case}: {state:?} after {peak}"
            );
            let loaded = state.deflection.unwrap();
            assert!(loaded < 2.0 * rest, "{case}: {loaded} from {rest}");
            // Taken off, the stone leaves the branch to spring back.
            world.remove_entity(STONE);
            run(&mut world, 60.0, 60.0);
            let settled = deflection(&world, BRANCH);
            assert!(
                (settled - rest).abs() < 1.0e-3 * rest.abs(),
                "{case}: {settled} after resting at {rest}"
            );
        }
    }
}
