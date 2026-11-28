//! An anchor gives by the law's elastic extension for the force it carries
//! and charges that whole force, whichever way it points. Anchors that hold
//! one body share its load by where each holds. An overloaded anchor lets go,
//! and the rest carry on or follow it, on the same step after a save. An
//! anchor holds in its support's own frame wherever the support goes, toward
//! gravity however it points, and from another support it is handed to; a
//! loose support goes with what it holds.
use super::connection::{DT, SCALES, Scale, TRUNK, body, world};
use super::*;
use crate::world::connection::{anchor_stiffness, hinge_stiffness};
use engine_material::{Beam, Connection, FailureMode, Material};
use std::f32::consts::FRAC_PI_2;

const BAR: PhysicsId = PhysicsId::new(2);
const CLOD: PhysicsId = PhysicsId::new(3);

/// A bar eight units long and half a unit high, level, four units below the
/// support's center and so clear of it. Its center of mass lies `center`
/// units right of its middle. It never sleeps, so that it settles all the
/// way.
fn bar(world: &mut PhysicsWorld, scale: Scale, center: f32) {
    let (half_length, half_height) = (4.0 * scale.k, 0.25 * scale.k);
    let mass = 4.0 * half_length * half_height;
    let mut collider = ColliderSpec::cuboid(
        ColliderId::new(BAR, ColliderRole::PRIMARY, 0),
        half_length,
        half_height,
    );
    collider.mass_properties = Some(ColliderMassProperties {
        center: Vec2::new(center * scale.k, 0.0),
        mass,
        inertia: mass * (half_length * half_length + half_height * half_height) / 3.0,
    });
    assert!(world.insert_body(
        body(BAR),
        BodySpec {
            position: Vec2::new(0.0, -4.0 * scale.k),
            can_sleep: false,
            ..BodySpec::default()
        },
        &[collider],
    ));
}

fn anchor_id(index: u32) -> JointId {
    JointId::new(BAR, JointRole::new(1 + index))
}

/// An anchor's own section: half a unit square, one unit long.
fn beam(scale: Scale) -> Beam {
    Beam::new(0.5 * scale.k, 0.5 * scale.k, scale.k)
}

fn material(stiffness: f32, strength: f32, toughness: f32) -> Material {
    Material {
        name: "test",
        density: 1.0,
        stiffness,
        strength,
        toughness,
        damping: 0.5,
        mode: FailureMode::Brittle,
    }
}

/// Anchor the bar in the support `x` units right of its middle, along `axis`.
fn anchor(
    world: &mut PhysicsWorld,
    scale: Scale,
    index: u32,
    x: f32,
    axis: Vec2,
    connection: Connection,
) -> Result<(), ConnectionRejection> {
    world.insert_anchor(
        anchor_id(index),
        AnchorSpec {
            support: body(TRUNK),
            body: body(BAR),
            at: Vec2::new(x * scale.k, -4.0 * scale.k),
            axis,
            connection,
        },
    )
}

fn weight(world: &PhysicsWorld, scale: Scale) -> f32 {
    world.body_mass(body(BAR)).unwrap() * scale.g
}

/// The stiffness at which one anchor of `beam` gives `stretch` under the
/// bar's weight.
fn stiffness_for(world: &PhysicsWorld, scale: Scale, beam: Beam, stretch: f32) -> f32 {
    weight(world, scale) * beam.length / (beam.area() * stretch)
}

fn run(world: &mut PhysicsWorld, seconds: f32, rate: f32) {
    for _ in 0..(seconds * rate).round() as usize {
        world.step(1.0 / rate);
    }
}

fn load(world: &PhysicsWorld, index: u32) -> ConnectionLoadSample {
    world.connection(anchor_id(index)).unwrap().load
}

#[test]
fn an_anchor_gives_by_the_laws_elastic_extension_and_charges_its_whole_force() {
    for scale in SCALES {
        for (rate, iterations) in [(60.0, 4), (240.0, 4), (60.0, 1), (60.0, 8)] {
            // Held along its beam, across it, and between the two.
            for axis in [Vec2::new(0.0, -1.0), Vec2::X, Vec2::new(3.0, -4.0)] {
                let mut world = world(scale, iterations);
                bar(&mut world, scale, 0.0);
                let weight = weight(&world, scale);
                let beam = beam(scale);
                let stretch = 0.05 * scale.k;
                // Safe to four times the bar's weight.
                let connection = Connection::new(
                    material(
                        stiffness_for(&world, scale, beam, stretch),
                        4.0 * weight / beam.area(),
                        1.0,
                    ),
                    beam,
                )
                .unwrap();
                assert!((anchor_stiffness(&connection) * stretch - weight).abs() < 1.0e-4 * weight);
                anchor(&mut world, scale, 0, 0.0, axis, connection).unwrap();
                let case = format!(
                    "g {} at {rate} Hz, {iterations} iterations, along {axis:?}",
                    scale.g
                );
                let mut charged = 0.0;
                for _ in 0..(20.0 * rate) as usize {
                    world.step(1.0 / rate);
                    charged += load(&world, 0).charged;
                }
                // It hangs by the law's extension for its weight, straight
                // below where it was held.
                let motion = world.motion(body(BAR)).unwrap();
                let sunk = -4.0 * scale.k - motion.position.y;
                assert!((sunk - stretch).abs() < 1.0e-3 * stretch, "{case}: {sunk}");
                assert!(
                    motion.position.x.abs() < 1.0e-3 * stretch,
                    "{case}: {motion:?}"
                );
                assert!(motion.angle.abs() < 1.0e-5, "{case}: {motion:?}");
                assert!(motion.linear_velocity.length() < 1.0e-3 * scale.k, "{case}");
                // The bar pulls down: along a beam that points down, and
                // toward the clockwise side of one that points right.
                let state = world.connection(anchor_id(0)).unwrap();
                let along = axis / axis.length();
                let pull = Vec2::new(0.0, -weight);
                let across = Vec2::new(-along.y, along.x);
                let (axial, shear) = (pull.dot(along), pull.dot(across));
                assert!(
                    (state.load.axial - axial).abs() < 1.0e-3 * weight,
                    "{case}: {state:?}"
                );
                assert!(
                    (state.load.shear - shear).abs() < 1.0e-3 * weight,
                    "{case}: {state:?}"
                );
                assert!(
                    state.load.moment.abs() < 1.0e-3 * weight * scale.k,
                    "{case}"
                );
                // Whichever way it points, the whole weight charges: a
                // quarter of the safe stress, which costs nothing.
                assert!(
                    (state.load.utilization - 0.25).abs() < 1.0e-3,
                    "{case}: {state:?}"
                );
                assert_eq!(charged, 0.0, "{case}");
                assert_eq!(state.connection.integrity(), 1.0, "{case}");
                assert!(state.deflection.unwrap().abs() < 1.0e-5, "{case}");
            }
        }
    }
}

/// How a level bar held by like anchors at `xs` settles under `weight` at
/// `center`, right of its middle: how far its middle sinks, and how far it
/// turns clockwise, for anchors that hold with stiffness `pull` and hold
/// their turn with `turn`. For small turns.
fn settled(xs: &[f32], pull: f32, turn: f32, weight: f32, center: f32) -> (f32, f32) {
    let n = xs.len() as f32;
    let sum: f32 = xs.iter().sum();
    let squares: f32 = xs.iter().map(|x| x * x).sum();
    let angle =
        weight * (n * center - sum) / (n * pull * squares - pull * sum * sum + n * n * turn);
    ((weight / pull - sum * angle) / n, angle)
}

/// A world whose bar, its center of mass `center` units right of its middle,
/// hangs from anchors at `xs` that each give `stretch` under a quarter of its
/// weight and are safe to `safe` times its weight.
fn hung(scale: Scale, center: f32, xs: &[f32], safe: f32) -> (PhysicsWorld, Connection) {
    let mut world = world(scale, 4);
    bar(&mut world, scale, center);
    let weight = weight(&world, scale);
    let beam = beam(scale);
    let connection = Connection::new(
        material(
            0.25 * stiffness_for(&world, scale, beam, 0.02 * scale.k),
            safe * weight / beam.area(),
            1.0e-6 * weight * scale.k,
        ),
        beam,
    )
    .unwrap();
    for (index, &x) in xs.iter().enumerate() {
        anchor(
            &mut world,
            scale,
            index as u32,
            x,
            Vec2::new(0.0, -1.0),
            connection,
        )
        .unwrap();
    }
    (world, connection)
}

/// Each anchor's pull and moment, as `settled` has the bar lie.
fn expected(
    scale: Scale,
    xs: &[f32],
    connection: &Connection,
    weight: f32,
    center: f32,
) -> Vec<(f32, f32)> {
    let (pull, turn) = (anchor_stiffness(connection), hinge_stiffness(connection));
    let at: Vec<f32> = xs.iter().map(|x| x * scale.k).collect();
    let (sink, angle) = settled(&at, pull, turn, weight, center * scale.k);
    // The bar turns clockwise, and so bends each anchor clockwise.
    at.iter()
        .map(|x| (pull * (sink + x * angle), -turn * angle))
        .collect()
}

#[test]
fn anchors_share_a_turning_load_by_where_they_hold() {
    let xs = [-3.0, -1.0, 1.0, 3.0];
    for scale in SCALES {
        // Its center of mass a unit right of its middle.
        let (mut world, connection) = hung(scale, 1.0, &xs, 4.0);
        let weight = weight(&world, scale);
        run(&mut world, 30.0, 60.0);
        let loads: Vec<_> = (0..4).map(|i| load(&world, i)).collect();
        let expected = expected(scale, &xs, &connection, weight, 1.0);
        for (i, (load, (pull, moment))) in loads.iter().zip(&expected).enumerate() {
            let case = format!("g {}, anchor {i}: {load:?}", scale.g);
            assert!(
                (load.axial - pull).abs() < 2.0e-3 * weight,
                "{case} for {pull}"
            );
            assert!(load.shear.abs() < 2.0e-3 * weight, "{case}");
            assert!(
                (load.moment - moment).abs() < 2.0e-3 * weight * scale.k,
                "{case} for {moment}"
            );
            assert_eq!(load.charged, 0.0, "{case}");
        }
        // Together they carry the weight, where it lies.
        let carried: f32 = loads.iter().map(|l| l.axial).sum();
        assert!((carried - weight).abs() < 1.0e-3 * weight, "g {}", scale.g);
        let turning: f32 = loads
            .iter()
            .zip(xs)
            .map(|(l, x)| l.axial * x * scale.k - l.moment)
            .sum();
        assert!(
            (turning - weight * scale.k).abs() < 2.0e-3 * weight * scale.k,
            "g {}: {turning}",
            scale.g
        );
        // The farther an anchor holds toward the weight, the more it
        // carries: the outer one there twice what the inner one beside it
        // does, and the outer one on the far side almost nothing.
        assert!(
            loads[3].axial > 1.3 * loads[2].axial && loads[1].axial > 1.5 * loads[0].axial,
            "g {}: {loads:?}",
            scale.g
        );
        assert!(loads[0].axial > 0.0 && loads[0].axial < 0.12 * weight);
        // It hangs level but for a slight turn toward the weight.
        let motion = world.motion(body(BAR)).unwrap();
        assert!(motion.angle < 0.0 && motion.angle > -0.01, "{motion:?}");
    }
}

#[test]
fn an_overloaded_anchor_lets_go_and_the_rest_carry_its_load() {
    let xs = [-3.0, -1.0, 1.0, 3.0];
    for scale in SCALES {
        let (mut world, connection) = hung(scale, 1.0, &xs[..3], 4.0);
        let weight = weight(&world, scale);
        // The outer anchor toward the weight is as stiff as the rest but
        // weak: safe to three tenths of the weight, where it would carry
        // four.
        let frail = Connection::new(
            Material {
                strength: 0.3 * weight / beam(scale).area(),
                ..connection.region().material()
            },
            beam(scale),
        )
        .unwrap();
        anchor(&mut world, scale, 3, 3.0, Vec2::new(0.0, -1.0), frail).unwrap();
        let all = expected(scale, &xs, &connection, weight, 1.0);
        assert!((all[3].0 - 0.4 * weight).abs() < 1.0e-3 * weight, "{all:?}");
        let mut severed = Vec::new();
        for step in 0..(30.0 * 60.0) as usize {
            world.step(DT);
            severed.extend(world.severed_connections().iter().map(|&id| (step, id)));
        }
        // It lets go as the bar settles onto it, and only it.
        assert_eq!(severed.len(), 1, "g {}: {severed:?}", scale.g);
        assert_eq!(severed[0].1, anchor_id(3));
        assert!(severed[0].0 < 30, "g {}: {severed:?}", scale.g);
        let state = world.connection(anchor_id(3)).unwrap();
        assert!(state.failure.is_some() && state.connection.failed());
        assert!(state.load.utilization > 1.0, "{state:?}");
        assert_eq!(state.deflection, None);
        assert!(!world.contains_joint(anchor_id(3)));
        // The three that still hold carry the weight between them as they
        // now lie.
        let rest = expected(scale, &xs[..3], &connection, weight, 1.0);
        for (i, (pull, _)) in rest.iter().enumerate() {
            let load = load(&world, i as u32);
            let case = format!("g {}, anchor {i}: {load:?}", scale.g);
            assert!(
                (load.axial - pull).abs() < 5.0e-3 * weight,
                "{case} for {pull}"
            );
            assert!(load.utilization < 1.0, "{case}");
            assert_eq!(load.charged, 0.0, "{case}");
        }
        // The one beside the weight carries most of it, where it would have
        // carried three tenths, and the far one now holds its end down.
        assert!(load(&world, 2).axial > 2.5 * all[2].0, "g {}", scale.g);
        assert!(load(&world, 0).axial < 0.0, "g {}", scale.g);
        assert_eq!(world.identity_fault(), None);
    }
}

/// A world whose bar, weightless so far, hangs from anchors each safe to
/// three tenths of its weight: enough for the weight shared evenly, too
/// little for the outer anchor toward a center of mass `center` units off
/// the middle, which carries four tenths.
fn frail(scale: Scale, center: f32) -> PhysicsWorld {
    let (mut world, _) = hung(scale, center, &[-3.0, -1.0, 1.0, 3.0], 0.3);
    assert!(world.set_gravity_scale(body(BAR), 0.0, true));
    world
}

/// One step, the bar's `step`th, as its weight comes on over two seconds: a
/// load that builds up, where a weight let go at once would swing its
/// anchors to twice what they then carry.
fn lower(world: &mut PhysicsWorld, step: usize) {
    let share = ((step + 1) as f32 / 120.0).min(1.0);
    assert!(world.set_gravity_scale(body(BAR), share, true));
    world.step(DT);
}

#[test]
fn anchors_too_weak_for_their_share_let_go_from_the_loaded_side_and_the_bar_falls() {
    for scale in SCALES {
        let mut world = frail(scale, 1.0);
        let mut severed = Vec::new();
        for step in 0..240 {
            lower(&mut world, step);
            severed.extend(world.severed_connections().iter().map(|&id| (step, id)));
        }
        // The outer one toward the weight goes first, alone, once about
        // three quarters of the weight has come on. What it carried overloads
        // the rest, and the far one goes last.
        let order: Vec<JointId> = severed.iter().map(|&(_, id)| id).collect();
        assert_eq!(order.len(), 4, "g {}: {severed:?}", scale.g);
        assert_eq!(order[0], anchor_id(3), "g {}: {severed:?}", scale.g);
        assert_eq!(order[3], anchor_id(0), "g {}: {severed:?}", scale.g);
        assert!(
            severed[0].0.abs_diff(90) <= 10 && severed[0].0 < severed[1].0,
            "g {}: {severed:?}",
            scale.g
        );
        assert!(
            severed[3].0 < severed[0].0 + 30,
            "g {}: {severed:?}",
            scale.g
        );
        assert_eq!(world.joint_count(), 0);
        // Free of them, it falls.
        let motion = world.motion(body(BAR)).unwrap();
        assert!(
            motion.position.y < -8.0 * scale.k && motion.linear_velocity.y < -scale.g,
            "g {}: {motion:?}",
            scale.g
        );
        assert_eq!(world.identity_fault(), None);
        // The same anchors hold the bar when its weight lies in the middle.
        let mut level = frail(scale, 0.0);
        for step in 0..600 {
            lower(&mut level, step);
            assert!(level.severed_connections().is_empty(), "g {}", scale.g);
        }
        for i in 0..4 {
            let load = load(&level, i);
            assert!((load.utilization - 0.25 / 0.3).abs() < 0.01, "{load:?}");
            assert_eq!(load.charged, 0.0);
        }
    }
}

/// What a host sees of the bar and its anchors.
fn observed(world: &PhysicsWorld) -> impl PartialEq + std::fmt::Debug {
    let anchors: Vec<_> = (0..4)
        .map(|i| {
            let state = world.connection(anchor_id(i)).unwrap();
            (
                state.load,
                state.failure,
                state.deflection,
                state.connection.region().remaining_precise(),
            )
        })
        .collect();
    (
        world.motion(body(BAR)),
        anchors,
        world.severed_connections().to_vec(),
    )
}

#[test]
fn a_restored_or_cloned_world_lets_its_anchors_go_exactly_as_the_original() {
    let scale = SCALES[0];
    let mut world = frail(scale, 1.0);
    // Half the weight is on, and every anchor still holds.
    for step in 0..60 {
        lower(&mut world, step);
    }
    assert_eq!(world.joint_count(), 4);
    let bytes = world.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
    let mut cloned = world.clone();
    let mut severed = 0;
    for step in 60..180 {
        lower(&mut world, step);
        lower(&mut restored, step);
        lower(&mut cloned, step);
        assert_eq!(observed(&restored), observed(&world), "step {step}");
        assert_eq!(observed(&cloned), observed(&world), "step {step}");
        severed += world.severed_connections().len();
    }
    assert_eq!(severed, 4);
    assert_eq!(
        restored.snapshot_bytes().unwrap(),
        world.snapshot_bytes().unwrap()
    );
}

#[test]
fn an_anchor_reshaped_where_it_holds_gives_by_its_new_beam() {
    for scale in SCALES {
        let (mut world, connection) = hung(scale, 0.0, &[0.0], 4.0);
        run(&mut world, 30.0, 60.0);
        let sunk =
            |world: &PhysicsWorld| -4.0 * scale.k - world.motion(body(BAR)).unwrap().position.y;
        let thin = sunk(&world);
        assert!(
            (thin - 0.08 * scale.k).abs() < 1.0e-3 * thin,
            "g {}: {thin}",
            scale.g
        );
        // Twice as thick, it gives half as far, where it was held.
        let thick = Beam::new(scale.k, 0.5 * scale.k, scale.k);
        world.reshape_connection(anchor_id(0), thick).unwrap();
        run(&mut world, 30.0, 60.0);
        let state = world.connection(anchor_id(0)).unwrap();
        assert_eq!(state.connection.beam(), thick);
        assert!(
            (anchor_stiffness(&state.connection) - 2.0 * anchor_stiffness(&connection)).abs()
                < 1.0e-3 * anchor_stiffness(&connection)
        );
        assert!(
            (sunk(&world) - 0.5 * thin).abs() < 1.0e-3 * thin,
            "g {}",
            scale.g
        );
        assert!((state.load.utilization - 0.125).abs() < 1.0e-3, "{state:?}");
    }
}

#[test]
fn an_anchor_is_refused_as_a_connection_is() {
    let scale = SCALES[0];
    let mut world = world(scale, 4);
    bar(&mut world, scale, 0.0);
    let connection = Connection::new(material(1.0e6, 1.0e6, 1.0), beam(scale)).unwrap();
    let down = Vec2::new(0.0, -1.0);
    let spec = AnchorSpec {
        support: body(TRUNK),
        body: body(BAR),
        at: Vec2::new(0.0, -4.0 * scale.k),
        axis: down,
        connection,
    };
    use ConnectionRejection as R;
    for (changed, why) in [
        (
            AnchorSpec {
                body: body(TRUNK),
                ..spec
            },
            R::Bodies,
        ),
        (
            AnchorSpec {
                support: body(PhysicsId::new(9)),
                ..spec
            },
            R::Bodies,
        ),
        (
            AnchorSpec {
                axis: Vec2::ZERO,
                ..spec
            },
            R::Geometry,
        ),
        (
            AnchorSpec {
                at: Vec2::new(f32::NAN, 0.0),
                ..spec
            },
            R::Geometry,
        ),
    ] {
        assert_eq!(world.insert_anchor(anchor_id(0), changed), Err(why));
    }
    let mut spent = connection;
    spent.absorb(spent.region().capacity()).unwrap();
    assert_eq!(
        world.insert_anchor(
            anchor_id(0),
            AnchorSpec {
                connection: spent,
                ..spec
            }
        ),
        Err(R::Exhausted)
    );
    assert_eq!(world.joint_count(), 0);
    world.insert_anchor(anchor_id(0), spec).unwrap();
    // The identity is taken, by an anchor or a weld.
    assert_eq!(world.insert_anchor(anchor_id(0), spec), Err(R::Taken));
    assert_eq!(
        world.insert_connection(
            anchor_id(0),
            ConnectionSpec {
                root_body: body(TRUNK),
                body: body(BAR),
                root: spec.at,
                axis: down,
                connection,
                bends: true,
            }
        ),
        Err(R::Taken)
    );
    // Two bodies may share any number of anchors.
    world.insert_anchor(anchor_id(1), spec).unwrap();
    assert_eq!(world.joint_count(), 2);
    assert!(world.remove_connection(anchor_id(0)));
    assert!(world.remove_entity(BAR));
    assert_eq!(world.joint_count(), 0);
    assert_eq!(world.connection(anchor_id(1)), None);
    assert_eq!(world.identity_fault(), None);
}

/// A clod of ground one unit square, of the bar's own mass, that never
/// sleeps.
fn clod(world: &mut PhysicsWorld, scale: Scale, kind: BodyKind, position: Vec2) {
    let half = 0.5 * scale.k;
    let mass = 4.0 * scale.k * scale.k;
    let mut collider =
        ColliderSpec::cuboid(ColliderId::new(CLOD, ColliderRole::PRIMARY, 0), half, half);
    collider.mass_properties = Some(ColliderMassProperties {
        center: Vec2::ZERO,
        mass,
        inertia: mass * 2.0 * half * half / 3.0,
    });
    assert!(world.insert_body(
        body(CLOD),
        BodySpec {
            kind,
            position,
            can_sleep: false,
            ..BodySpec::default()
        },
        &[collider],
    ));
}

/// A world whose bar hangs, level, from one anchor at its middle in a clod
/// at `clod_at`, in place of the trunk. The anchor gives `stretch` under the
/// bar's weight and is safe to four times it. A clod where the anchor holds
/// overlaps the bar, which it does not touch while the anchor holds.
fn clodded(scale: Scale, kind: BodyKind, clod_at: Vec2) -> (PhysicsWorld, f32) {
    let mut world = world(scale, 4);
    assert!(world.remove_entity(TRUNK));
    clod(&mut world, scale, kind, clod_at * scale.k);
    bar(&mut world, scale, 0.0);
    let weight = weight(&world, scale);
    let (beam, stretch) = (beam(scale), 0.08 * scale.k);
    let connection = Connection::new(
        material(
            stiffness_for(&world, scale, beam, stretch),
            4.0 * weight / beam.area(),
            1.0,
        ),
        beam,
    )
    .unwrap();
    world
        .insert_anchor(
            anchor_id(0),
            AnchorSpec {
                support: body(CLOD),
                body: body(BAR),
                at: Vec2::new(0.0, -4.0 * scale.k),
                axis: Vec2::new(0.0, -1.0),
                connection,
            },
        )
        .unwrap();
    (world, stretch)
}

/// Drive the clod, over four seconds, from where it lies by `shift` and
/// through `turn`, setting off and coming to rest gently.
fn carry(world: &mut PhysicsWorld, shift: Vec2, turn: f32) {
    let start = world.motion(body(CLOD)).unwrap();
    for step in 1..=240 {
        let t = step as f32 / 240.0;
        let share = t * t * (3.0 - 2.0 * t);
        assert!(world.set_next_kinematic_pose(
            body(CLOD),
            start.position + shift * share,
            start.angle + turn * share
        ));
        world.step(DT);
    }
}

#[test]
fn an_anchor_holds_in_its_supports_own_frame_as_the_support_moves_and_turns() {
    for scale in SCALES {
        // The clod holds the bar four units below its center.
        let (mut world, stretch) = clodded(scale, BodyKind::KinematicPosition, Vec2::ZERO);
        let weight = weight(&world, scale);
        run(&mut world, 10.0, 60.0);
        // Carried three units right and two up, and turned a quarter turn
        // counterclockwise.
        let shift = Vec2::new(3.0, 2.0) * scale.k;
        carry(&mut world, shift, FRAC_PI_2);
        // The long bar swings slowly on its anchor's turn, and takes long
        // to come to rest.
        run(&mut world, 200.0, 60.0);
        // The bar hangs from where the clod now holds it, turned with the
        // clod, by the law's extension for its weight straight down.
        let held = shift + Vec2::new(0.0, -4.0 * scale.k).rotate_radians(FRAC_PI_2);
        let motion = world.motion(body(BAR)).unwrap();
        let case = format!("g {}: {motion:?}", scale.g);
        assert!(
            (held - motion.position - Vec2::new(0.0, stretch)).length() < 2.0e-3 * stretch,
            "{case}"
        );
        assert!((motion.angle - FRAC_PI_2).abs() < 1.0e-4, "{case}");
        // The anchor's beam turned with the clod, so the weight now pulls
        // across it, toward its clockwise side, and charges as before.
        let state = world.connection(anchor_id(0)).unwrap();
        assert!(state.load.axial.abs() < 1.0e-3 * weight, "{state:?}");
        assert!(
            (state.load.shear + weight).abs() < 1.0e-3 * weight,
            "{state:?}"
        );
        assert!((state.load.utilization - 0.25).abs() < 1.0e-3, "{state:?}");
        assert_eq!(state.connection.integrity(), 1.0);
    }
}

#[test]
fn a_loose_support_falls_with_what_its_anchors_hold_and_takes_their_pull() {
    for scale in SCALES {
        // Free in the air, the clod and the bar fall together, and the
        // anchor between them carries nothing: no weight hangs from a
        // support that falls. The clod holds the bar at its own center.
        let middle = Vec2::new(0.0, -4.0);
        let (mut world, stretch) = clodded(scale, BodyKind::Dynamic, middle);
        run(&mut world, 1.0, 60.0);
        let (clod, bar) = (
            world.motion(body(CLOD)).unwrap(),
            world.motion(body(BAR)).unwrap(),
        );
        let case = format!("g {}: {clod:?} {bar:?}", scale.g);
        assert!(
            (clod.linear_velocity.y + scale.g).abs() < 1.0e-3 * scale.g,
            "{case}"
        );
        assert!(
            (bar.linear_velocity - clod.linear_velocity).length() < 1.0e-3 * scale.g,
            "{case}"
        );
        assert!(
            (bar.position - clod.position).length() < 1.0e-2 * stretch,
            "{case}"
        );
        assert!(load(&world, 0).utilization < 1.0e-3, "{case}");
        // Pushed sideways where nothing weighs, the bar pulls the clod after
        // it, and the anchor carries the share of the push that moves the
        // clod: half of it, for a clod of the bar's mass.
        let (mut world, _) = clodded(scale, BodyKind::Dynamic, middle);
        let weight = weight(&world, scale);
        assert!(world.set_gravity(Vec2::ZERO));
        for step in 0..300 {
            let share = ((step + 1) as f32 / 120.0).min(1.0);
            world.clear_forces();
            assert!(world.apply_force(body(BAR), Vec2::new(share * weight, 0.0), true));
            world.step(DT);
        }
        let state = world.connection(anchor_id(0)).unwrap();
        assert!(
            (state.load.shear - 0.5 * weight).abs() < 1.0e-2 * weight,
            "g {}: {state:?}",
            scale.g
        );
        assert!(state.load.axial.abs() < 1.0e-2 * weight, "{state:?}");
        let (clod, bar) = (
            world.motion(body(CLOD)).unwrap(),
            world.motion(body(BAR)).unwrap(),
        );
        assert!(
            (clod.linear_velocity.x - bar.linear_velocity.x).abs() < 1.0e-2 * bar.linear_velocity.x,
            "g {}: {clod:?} {bar:?}",
            scale.g
        );
    }
}

#[test]
fn anchors_hold_toward_gravity_however_it_points() {
    for scale in SCALES {
        let (mut world, connection) = hung(scale, 0.0, &[-2.0, 2.0], 4.0);
        let weight = weight(&world, scale);
        // Gravity half a radian off straight down, toward the right.
        let tilt = 0.5_f32;
        let down = Vec2::new(tilt.sin(), -tilt.cos());
        assert!(world.set_gravity(down * scale.g));
        run(&mut world, 30.0, 60.0);
        // The two share the weight evenly, whichever way it pulls: along
        // their beams by its cosine, and across them, toward their
        // counterclockwise side, by its sine.
        for i in 0..2 {
            let load = load(&world, i);
            let case = format!("g {}, anchor {i}: {load:?}", scale.g);
            assert!(
                (load.axial - 0.5 * weight * tilt.cos()).abs() < 2.0e-3 * weight,
                "{case}"
            );
            assert!(
                (load.shear - 0.5 * weight * tilt.sin()).abs() < 2.0e-3 * weight,
                "{case}"
            );
            assert!((load.utilization - 0.125).abs() < 1.0e-3, "{case}");
        }
        // The bar gave the law's extension for its weight along gravity,
        // not straight down, and did not turn.
        let motion = world.motion(body(BAR)).unwrap();
        let stretch = 0.5 * weight / anchor_stiffness(&connection);
        let moved = motion.position - Vec2::new(0.0, -4.0 * scale.k);
        assert!(
            (moved - down * stretch).length() < 1.0e-2 * stretch,
            "g {}: {motion:?}",
            scale.g
        );
        assert!(motion.angle.abs() < 1.0e-4, "g {}: {motion:?}", scale.g);
    }
}

#[test]
fn an_anchor_handed_to_another_support_holds_from_it_as_it_held() {
    for scale in SCALES {
        let (mut world, connection) = hung(scale, 0.0, &[0.0], 4.0);
        let weight = weight(&world, scale);
        let stretch = weight / anchor_stiffness(&connection);
        run(&mut world, 30.0, 60.0);
        let lying = world.motion(body(BAR)).unwrap();
        let state = world.connection(anchor_id(0)).unwrap();
        // A clod well clear of the trunk and the bar takes the anchor where
        // it lies.
        clod(
            &mut world,
            scale,
            BodyKind::KinematicPosition,
            Vec2::new(-6.0, 3.0) * scale.k,
        );
        world.rebind_connection(anchor_id(0), body(CLOD)).unwrap();
        let handed = world.connection(anchor_id(0)).unwrap();
        assert_eq!(handed.root_body, body(CLOD));
        assert_eq!(
            (handed.body, handed.connection, handed.load, handed.failure),
            (state.body, state.connection, state.load, state.failure)
        );
        // It carries on from the give it had, so the bar does not stir.
        run(&mut world, 1.0, 60.0);
        let still = world.motion(body(BAR)).unwrap();
        let case = format!("g {}: {lying:?} {still:?}", scale.g);
        assert!(
            (still.position - lying.position).length() < 1.0e-3 * stretch,
            "{case}"
        );
        assert!(
            (load(&world, 0).axial - weight).abs() < 1.0e-3 * weight,
            "{case}"
        );
        // The trunk no longer holds it: taken away, it takes no anchor with
        // it, and the clod carries the bar wherever it goes.
        assert!(world.remove_entity(TRUNK));
        assert!(world.contains_joint(anchor_id(0)));
        let shift = Vec2::new(2.0, 1.0) * scale.k;
        carry(&mut world, shift, 0.0);
        run(&mut world, 30.0, 60.0);
        let carried = world.motion(body(BAR)).unwrap();
        assert!(
            (carried.position - lying.position - shift).length() < 2.0e-3 * stretch,
            "g {}: {carried:?}",
            scale.g
        );
        assert_eq!(world.identity_fault(), None);
        // A restored world holds the bar from the clod too.
        let bytes = world.snapshot_bytes().unwrap();
        let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        assert_eq!(observed_one(&restored), observed_one(&world));
        for _ in 0..60 {
            world.step(DT);
            restored.step(DT);
        }
        assert_eq!(observed_one(&restored), observed_one(&world));
    }
}

/// What a host sees of the bar and its one anchor. A snapshot keeps no
/// material's name.
fn observed_one(world: &PhysicsWorld) -> impl PartialEq + std::fmt::Debug {
    (
        world.motion(body(BAR)),
        world.connection(anchor_id(0)).map(|s| {
            (
                s.root_body,
                s.load,
                s.failure,
                s.deflection,
                s.connection.region().remaining_precise(),
            )
        }),
    )
}

#[test]
fn handing_on_an_anchor_is_refused_where_it_has_nothing_to_hand_on() {
    let scale = SCALES[0];
    let (mut world, _) = hung(scale, 0.0, &[0.0], 4.0);
    clod(
        &mut world,
        scale,
        BodyKind::Fixed,
        Vec2::new(-6.0, 3.0) * scale.k,
    );
    use ConnectionRejection as R;
    assert_eq!(
        world.rebind_connection(anchor_id(1), body(CLOD)),
        Err(R::Missing)
    );
    assert_eq!(
        world.rebind_connection(anchor_id(0), body(PhysicsId::new(9))),
        Err(R::Bodies)
    );
    assert_eq!(
        world.rebind_connection(anchor_id(0), body(BAR)),
        Err(R::Bodies)
    );
    assert_eq!(
        world.connection(anchor_id(0)).unwrap().root_body,
        body(TRUNK)
    );
    // One that let go has no joint to hand on.
    let (mut weak, _) = hung(scale, 0.0, &[0.0], 0.5);
    clod(
        &mut weak,
        scale,
        BodyKind::Fixed,
        Vec2::new(-6.0, 3.0) * scale.k,
    );
    run(&mut weak, 2.0, 60.0);
    assert!(weak.connection(anchor_id(0)).unwrap().failure.is_some());
    assert_eq!(
        weak.rebind_connection(anchor_id(0), body(CLOD)),
        Err(R::Exhausted)
    );
    assert_eq!(world.identity_fault(), None);
    assert_eq!(weak.identity_fault(), None);
}
