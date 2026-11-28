//! An anchor gives by the law's elastic extension for the force it carries
//! and charges that whole force, whichever way it points. Anchors that hold
//! one body share its load by where each holds. An overloaded anchor lets go,
//! and the rest carry on or follow it, on the same step after a save.
use super::connection::{DT, SCALES, Scale, TRUNK, body, world};
use super::*;
use crate::world::connection::{anchor_stiffness, hinge_stiffness};
use engine_material::{Beam, Connection, FailureMode, Material};

const BAR: PhysicsId = PhysicsId::new(2);

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
