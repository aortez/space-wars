//! A weld reports the load its body puts on the root, charges nothing while
//! that load is safe, and severs once an overload exhausts it, on the same
//! step after a save.
use super::*;
use engine_material::{Beam, Connection, ConnectionLoad, FailureMode, Material};

pub(super) const DT: f32 = 1.0 / 60.0;
pub(super) const TRUNK: PhysicsId = PhysicsId::new(1);
pub(super) const BRANCH: PhysicsId = PhysicsId::new(2);
pub(super) const TWIN: PhysicsId = PhysicsId::new(3);

#[derive(Debug, Clone, Copy)]
pub(super) struct Scale {
    pub(super) g: f32,
    pub(super) length_unit: f32,
    /// World units per fixture unit.
    pub(super) k: f32,
}

/// Clock's scale, and metres, as for the tethers.
pub(super) const SCALES: [Scale; 2] = [
    Scale {
        g: 400.0,
        length_unit: 20.0,
        k: 20.0,
    },
    Scale {
        g: 9.81,
        length_unit: 1.0,
        k: 0.5,
    },
];

pub(super) fn body(entity: PhysicsId) -> BodyId {
    BodyId::new(entity, BodyRole::PRIMARY)
}

pub(super) fn weld_id(entity: PhysicsId) -> JointId {
    JointId::new(entity, JointRole::new(1))
}

/// A fixed trunk, one unit wide and four high, centered on the origin.
pub(super) fn world(scale: Scale, iterations: usize) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -scale.g),
        length_unit: scale.length_unit,
        solver_iterations: iterations,
        ..PhysicsWorldConfig::default()
    });
    assert!(world.insert_body(
        body(TRUNK),
        BodySpec {
            kind: BodyKind::Fixed,
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(TRUNK, ColliderRole::PRIMARY, 0),
            0.5 * scale.k,
            2.0 * scale.k,
        )],
    ));
    world
}

/// A branch four units long and half a unit high whose end meets the trunk's
/// side at mid-height, to the right for `side` 1 and the left for -1.
fn grow(world: &mut PhysicsWorld, scale: Scale, entity: PhysicsId, side: f32) {
    grow_at(world, scale, entity, side * 2.5);
}

/// A branch like [`grow`]'s, centered `x` units right of the trunk's center.
fn grow_at(world: &mut PhysicsWorld, scale: Scale, entity: PhysicsId, x: f32) {
    assert!(world.insert_body(
        body(entity),
        BodySpec {
            position: Vec2::new(x * scale.k, 0.0),
            ..BodySpec::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(entity, ColliderRole::PRIMARY, 0),
            2.0 * scale.k,
            0.25 * scale.k,
        )],
    ));
}

/// The branch's own square section, over its whole length.
pub(super) fn beam(scale: Scale) -> Beam {
    Beam::new(0.5 * scale.k, 0.5 * scale.k, 4.0 * scale.k)
}

fn material(strength: f32, toughness: f32) -> Material {
    Material {
        name: "test",
        density: 1.0,
        stiffness: 1.0e3 * strength,
        strength,
        toughness,
        damping: 0.0,
        mode: FailureMode::Ductile,
    }
}

/// The weight of a branch and the moment it puts on its root, at its center
/// two units out.
fn weight_and_moment(world: &PhysicsWorld, scale: Scale, entity: PhysicsId) -> (f32, f32) {
    let weight = world.body_mass(body(entity)).unwrap() * scale.g;
    (weight, weight * 2.0 * scale.k)
}

fn weld(
    world: &mut PhysicsWorld,
    scale: Scale,
    entity: PhysicsId,
    side: f32,
    material: Material,
) -> Result<(), ConnectionRejection> {
    world.insert_connection(
        weld_id(entity),
        ConnectionSpec {
            root_body: body(TRUNK),
            body: body(entity),
            root: Vec2::new(side * 0.5 * scale.k, 0.0),
            axis: Vec2::new(side, 0.0),
            connection: Connection::new(material, beam(scale)).unwrap(),
            bends: false,
        },
    )
}

/// The law's charge for one step of `moment` alone, uncapped by capacity.
fn charge_per_step(strength: f32, beam: Beam, moment: f32) -> f32 {
    let probe = Connection::new(material(strength, 1.0e15), beam).unwrap();
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

/// A branch held at half its safe stress whose weld lasts `steps` steps of
/// its weight.
fn overloaded(scale: Scale, steps: f32) -> PhysicsWorld {
    let mut world = world(scale, 4);
    grow(&mut world, scale, BRANCH, 1.0);
    let (_, moment) = weight_and_moment(&world, scale, BRANCH);
    let strength = 0.5 * moment / beam(scale).section_modulus();
    let toughness = steps * charge_per_step(strength, beam(scale), moment) / beam(scale).volume();
    weld(
        &mut world,
        scale,
        BRANCH,
        1.0,
        material(strength, toughness),
    )
    .unwrap();
    world
}

#[test]
fn a_resting_branch_reports_its_weight_times_its_reach_and_costs_nothing() {
    for scale in SCALES {
        for (rate, iterations) in [(60.0, 4), (120.0, 4), (240.0, 4), (60.0, 1), (60.0, 8)] {
            let mut world = world(scale, iterations);
            grow(&mut world, scale, BRANCH, 1.0);
            let (weight, moment) = weight_and_moment(&world, scale, BRANCH);
            let stress = moment / beam(scale).section_modulus();
            weld(&mut world, scale, BRANCH, 1.0, material(4.0 * stress, 1.0)).unwrap();
            let case = format!("g {} at {rate} Hz, {iterations} iterations", scale.g);
            // Ten seconds, long enough for the branch to fall asleep.
            for step in 0..(10.0 * rate) as usize {
                world.step(1.0 / rate);
                let load = world.connection(weld_id(BRANCH)).unwrap().load;
                assert_eq!(load.charged, 0.0, "{case}, step {step}: {load:?}");
                // From the first step, within rounding. The branch reaches
                // right, so its weight bends the root clockwise and presses it
                // down.
                assert!(
                    (load.moment + moment).abs() < 1.0e-4 * moment,
                    "{case}, step {step}: {load:?} for {moment}"
                );
                assert!(
                    (load.shear + weight).abs() < 1.0e-4 * weight,
                    "{case}, step {step}: {load:?} for {weight}"
                );
                assert!(
                    load.axial.abs() < 1.0e-4 * weight,
                    "{case}, step {step}: {load:?}"
                );
                assert!(
                    (load.utilization - 0.25).abs() < 0.25e-4,
                    "{case}, step {step}: {load:?}"
                );
            }
            // A sleeping weld keeps its last reading.
            let handle = world.body_handle(body(BRANCH)).unwrap();
            assert!(world.raw.bodies[handle].is_sleeping(), "{case}");
            let state = world.connection(weld_id(BRANCH)).unwrap();
            let region = state.connection.region();
            assert_eq!(region.remaining_precise(), f64::from(region.capacity()));
            assert_eq!(state.failure, None);
            assert!(world.contains_joint(weld_id(BRANCH)));
        }
    }
}

#[test]
fn a_hanging_body_pulls_its_root_along_the_beam() {
    for scale in SCALES {
        for rate in [60.0, 240.0] {
            let mut world = world(scale, 4);
            assert!(world.insert_body(
                body(BRANCH),
                BodySpec {
                    position: Vec2::new(0.0, -3.0 * scale.k),
                    ..BodySpec::default()
                },
                &[ColliderSpec::cuboid(
                    ColliderId::new(BRANCH, ColliderRole::PRIMARY, 0),
                    0.25 * scale.k,
                    scale.k,
                )],
            ));
            let weight = world.body_mass(body(BRANCH)).unwrap() * scale.g;
            world
                .insert_connection(
                    weld_id(BRANCH),
                    ConnectionSpec {
                        root_body: body(TRUNK),
                        body: body(BRANCH),
                        root: Vec2::new(0.0, -2.0 * scale.k),
                        axis: Vec2::new(0.0, -2.0),
                        connection: Connection::new(material(1.0e6, 1.0), beam(scale)).unwrap(),
                        bends: false,
                    },
                )
                .unwrap();
            for step in 0..(2.0 * rate) as usize {
                world.step(1.0 / rate);
                let load = world.connection(weld_id(BRANCH)).unwrap().load;
                if step < 5 {
                    continue;
                }
                let case = format!("g {} at {rate} Hz, step {step}: {load:?}", scale.g);
                assert!((load.axial - weight).abs() < 0.01 * weight, "{case}");
                assert!(load.shear.abs() < 0.01 * weight, "{case}");
                assert!(load.moment.abs() < 0.01 * weight * scale.k, "{case}");
            }
        }
    }
}

#[test]
fn each_weld_of_a_chain_carries_everything_beyond_it() {
    for scale in SCALES {
        let mut world = world(scale, 4);
        grow(&mut world, scale, BRANCH, 1.0);
        grow_at(&mut world, scale, TWIN, 6.5);
        let (inner, inner_moment) = weight_and_moment(&world, scale, BRANCH);
        let (outer, outer_moment) = weight_and_moment(&world, scale, TWIN);
        let strong = material(1.0e9, 1.0);
        weld(&mut world, scale, BRANCH, 1.0, strong).unwrap();
        // The outer branch grows from the inner one's tip, so its root body
        // moves.
        world
            .insert_connection(
                weld_id(TWIN),
                ConnectionSpec {
                    root_body: body(BRANCH),
                    body: body(TWIN),
                    root: Vec2::new(4.5 * scale.k, 0.0),
                    axis: Vec2::X,
                    connection: Connection::new(strong, beam(scale)).unwrap(),
                    bends: false,
                },
            )
            .unwrap();
        // The outer weight acts six units from the trunk.
        let carried = inner_moment + outer * 6.0 * scale.k;
        for step in 0..120 {
            world.step(DT);
            // Released unloaded, the two welds take up the load together
            // over a few steps: their first reading is about half of it, and
            // they ring to within a thousandth from the 25th step.
            if step < 30 {
                continue;
            }
            let root = world.connection(weld_id(BRANCH)).unwrap().load;
            let tip = world.connection(weld_id(TWIN)).unwrap().load;
            let case = format!("g {}, step {step}: {root:?} {tip:?}", scale.g);
            assert!(
                (tip.moment + outer_moment).abs() < 1.0e-3 * outer_moment,
                "{case}"
            );
            assert!((tip.shear + outer).abs() < 1.0e-3 * outer, "{case}");
            assert!((root.moment + carried).abs() < 1.0e-3 * carried, "{case}");
            assert!(
                (root.shear + inner + outer).abs() < 1.0e-3 * (inner + outer),
                "{case}"
            );
        }
    }
}

#[test]
fn a_spinning_pair_pulls_the_branch_outward_with_no_moment() {
    for scale in SCALES {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig {
            length_unit: scale.length_unit,
            ..PhysicsWorldConfig::default()
        });
        // A free hub, heavy but not fixed, with a branch reaching right.
        assert!(world.insert_body(
            body(TRUNK),
            BodySpec::default(),
            &[ColliderSpec {
                density: 20.0,
                ..ColliderSpec::cuboid(
                    ColliderId::new(TRUNK, ColliderRole::PRIMARY, 0),
                    0.5 * scale.k,
                    2.0 * scale.k,
                )
            }],
        ));
        grow(&mut world, scale, BRANCH, 1.0);
        let hub = world.body_mass(body(TRUNK)).unwrap();
        let mass = world.body_mass(body(BRANCH)).unwrap();
        // Both turn about their shared center of mass, as one piece.
        let center = mass * 2.5 * scale.k / (hub + mass);
        let spin = 2.0;
        assert!(world.set_velocity(body(TRUNK), Vec2::new(0.0, -spin * center), spin, true));
        assert!(world.set_velocity(
            body(BRANCH),
            Vec2::new(0.0, spin * (2.5 * scale.k - center)),
            spin,
            true
        ));
        weld(&mut world, scale, BRANCH, 1.0, material(1.0e9, 1.0)).unwrap();
        let pull = mass * spin * spin * (2.5 * scale.k - center);
        for step in 0..120 {
            world.step(DT);
            if step < 5 {
                continue;
            }
            let load = world.connection(weld_id(BRANCH)).unwrap().load;
            let case = format!("g {}, step {step}: {load:?} for {pull}", scale.g);
            assert!((load.axial - pull).abs() < 0.01 * pull, "{case}");
            assert!(load.shear.abs() < 0.01 * pull, "{case}");
            assert!(load.moment.abs() < 0.01 * pull * 2.0 * scale.k, "{case}");
        }
    }
}

#[test]
fn an_overload_exhausts_the_weld_on_schedule_and_the_branch_falls() {
    for scale in SCALES {
        // Long enough for the branch to fall asleep before the weld gives.
        let mut world = overloaded(scale, 300.0);
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
        for step in 1..=420_u32 {
            world.step(DT);
            if step == 280 {
                let handle = world.body_handle(body(BRANCH)).unwrap();
                assert!(world.raw.bodies[handle].is_sleeping(), "g {}", scale.g);
            }
            let state = world.connection(weld_id(BRANCH)).unwrap();
            if severed.is_none() {
                charged += f64::from(state.load.charged);
            }
            if world.severed_connections().is_empty() {
                continue;
            }
            assert_eq!(world.severed_connections(), [weld_id(BRANCH)]);
            assert_eq!(severed, None, "severed once");
            severed = Some((step, world.motion(body(BRANCH)).unwrap().position));
            assert!(state.connection.failed());
            assert!(state.failure.is_some_and(|f| f.excess >= 0.0));
            assert!(state.load.utilization > 1.9, "{state:?}");
            assert!(!world.contains_joint(weld_id(BRANCH)));
        }
        let (step, position) = severed.expect("the overload exhausts the weld");
        assert!(step.abs_diff(300) <= 1, "g {}: step {step}", scale.g);
        assert!(
            (charged - capacity).abs() <= 1.0e-5 * capacity,
            "{charged} of {capacity}"
        );
        // Free of the weld, it wakes and falls nearly freely past the trunk's
        // face.
        let elapsed = (420 - step) as f32 * DT;
        let fall = position.y - world.motion(body(BRANCH)).unwrap().position.y;
        assert!(
            fall > 0.4 * scale.g * elapsed * elapsed,
            "g {}: fell {fall} in {elapsed} s",
            scale.g
        );
        assert!(world.severed_connections().is_empty());
        assert_eq!(world.identity_fault(), None);

        // The severed identity stays taken until the host removes it.
        assert_eq!(
            weld(&mut world, scale, BRANCH, 1.0, material(1.0e6, 1.0)),
            Err(ConnectionRejection::Taken)
        );
        assert!(!world.insert_tether(
            weld_id(BRANCH),
            TetherSpec {
                anchor_body: body(TRUNK),
                anchor: Vec2::ZERO,
                body: body(BRANCH),
                attachment: Vec2::ZERO,
                length: 100.0 * scale.k,
            },
        ));
        assert!(world.remove_connection(weld_id(BRANCH)));
        assert!(world.connection(weld_id(BRANCH)).is_none());
        assert!(!world.remove_connection(weld_id(BRANCH)));
        assert_eq!(world.identity_fault(), None);
    }
}

/// What a host sees of the branch and its weld. A save keeps a material's law
/// parameters but not its name, which is presentation only.
fn observed(world: &PhysicsWorld) -> impl PartialEq + std::fmt::Debug {
    let state = world.connection(weld_id(BRANCH)).unwrap();
    let region = state.connection.region();
    (
        world.motion(body(BRANCH)),
        state.load,
        state.failure,
        region.remaining_precise(),
        region.material().identity(),
        world.severed_connections().to_vec(),
    )
}

#[test]
fn a_restored_or_cloned_world_severs_on_the_same_step_with_the_same_motion() {
    let scale = SCALES[0];
    let mut world = overloaded(scale, 90.0);
    for _ in 0..60 {
        world.step(DT);
    }
    let bytes = world.snapshot_bytes().unwrap();
    let mut restored = PhysicsWorld::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
    let mut cloned = world.clone();
    let mut severed = 0;
    for step in 0..90 {
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

#[test]
fn welds_that_fail_together_are_reported_in_identity_order() {
    let scale = SCALES[0];
    let mut world = world(scale, 4);
    grow(&mut world, scale, BRANCH, 1.0);
    grow(&mut world, scale, TWIN, -1.0);
    let (_, moment) = weight_and_moment(&world, scale, BRANCH);
    let weak = material(0.5 * moment / beam(scale).section_modulus(), 1.0e-9);
    // Inserted in the reverse order.
    weld(&mut world, scale, TWIN, -1.0, weak).unwrap();
    weld(&mut world, scale, BRANCH, 1.0, weak).unwrap();
    world.step(DT);
    assert_eq!(
        world.severed_connections(),
        [weld_id(BRANCH), weld_id(TWIN)]
    );
    // The twin reaches left, so it bends the root the other way.
    let twin = world.connection(weld_id(TWIN)).unwrap().load;
    let branch = world.connection(weld_id(BRANCH)).unwrap().load;
    assert!(
        twin.moment > 0.0 && branch.moment < 0.0,
        "{twin:?} {branch:?}"
    );
    assert_eq!(world.identity_fault(), None);
}

#[test]
fn a_connection_needs_a_free_identity_two_bodies_a_direction_and_capacity() {
    let scale = SCALES[0];
    let mut world = world(scale, 4);
    grow(&mut world, scale, BRANCH, 1.0);
    let strong = material(1.0e6, 1.0);
    let spec = ConnectionSpec {
        root_body: body(TRUNK),
        body: body(BRANCH),
        root: Vec2::new(0.5 * scale.k, 0.0),
        axis: Vec2::X,
        connection: Connection::new(strong, beam(scale)).unwrap(),
        bends: false,
    };
    let rope = JointId::new(BRANCH, JointRole::new(2));
    assert!(world.insert_tether(
        rope,
        TetherSpec {
            anchor_body: body(TRUNK),
            anchor: Vec2::ZERO,
            body: body(BRANCH),
            attachment: Vec2::ZERO,
            length: 10.0 * scale.k,
        },
    ));
    let mut exhausted = spec.connection;
    exhausted.absorb(exhausted.region().capacity()).unwrap();
    let other = JointId::new(BRANCH, JointRole::new(3));
    for (id, spec, rejection) in [
        (rope, spec, ConnectionRejection::Taken),
        (
            other,
            ConnectionSpec {
                body: body(PhysicsId::new(99)),
                ..spec
            },
            ConnectionRejection::Bodies,
        ),
        (
            other,
            ConnectionSpec {
                root_body: body(BRANCH),
                ..spec
            },
            ConnectionRejection::Bodies,
        ),
        (
            other,
            ConnectionSpec {
                axis: Vec2::ZERO,
                ..spec
            },
            ConnectionRejection::Geometry,
        ),
        (
            other,
            ConnectionSpec {
                root: Vec2::new(f32::NAN, 0.0),
                ..spec
            },
            ConnectionRejection::Geometry,
        ),
        (
            other,
            ConnectionSpec {
                axis: Vec2::new(f32::INFINITY, 0.0),
                ..spec
            },
            ConnectionRejection::Geometry,
        ),
        (
            other,
            ConnectionSpec {
                connection: exhausted,
                bends: false,
                ..spec
            },
            ConnectionRejection::Exhausted,
        ),
    ] {
        assert_eq!(
            world.insert_connection(id, spec),
            Err(rejection),
            "{spec:?}"
        );
    }
    assert!(world.connection(rope).is_none() && world.connection(other).is_none());
    assert!(world.tether(rope).is_some() && !world.contains_joint(other));
    world.insert_connection(other, spec).unwrap();
    assert_eq!(
        world.insert_connection(other, spec),
        Err(ConnectionRejection::Taken)
    );
    // A weld is not a rope.
    assert!(world.tether(other).is_none() && !world.remove_tether(other));
    assert_eq!(world.identity_fault(), None);
}

#[test]
fn a_connection_leaves_with_either_body_and_holds_both_in_their_world() {
    let scale = SCALES[0];
    let config = PhysicsWorldConfig::default();
    let mut world = world(scale, 4);
    grow(&mut world, scale, BRANCH, 1.0);
    weld(&mut world, scale, BRANCH, 1.0, material(1.0e6, 1.0)).unwrap();
    let mut elsewhere = PhysicsWorld::new(config);
    assert!(!world.transfer_entity(TRUNK, &mut elsewhere));
    assert!(!world.transfer_entity(BRANCH, &mut elsewhere));
    assert!(world.remove_entity(BRANCH));
    assert!(world.connection(weld_id(BRANCH)).is_none());
    assert!(!world.contains_joint(weld_id(BRANCH)));
    assert_eq!(world.identity_fault(), None);

    // A severed record still holds its bodies until the host removes it.
    grow(&mut world, scale, BRANCH, 1.0);
    let (_, moment) = weight_and_moment(&world, scale, BRANCH);
    let weak = material(0.5 * moment / beam(scale).section_modulus(), 1.0e-9);
    weld(&mut world, scale, BRANCH, 1.0, weak).unwrap();
    world.step(DT);
    assert_eq!(world.severed_connections(), [weld_id(BRANCH)]);
    assert!(!world.transfer_entity(BRANCH, &mut elsewhere));
    assert!(world.remove_connection(weld_id(BRANCH)));
    assert!(world.transfer_entity(BRANCH, &mut elsewhere));
    assert_eq!(world.identity_fault(), None);
    assert_eq!(elsewhere.identity_fault(), None);
}

#[test]
fn removing_a_live_connection_frees_the_body_without_a_failure() {
    let scale = SCALES[0];
    let mut world = world(scale, 4);
    grow(&mut world, scale, BRANCH, 1.0);
    weld(&mut world, scale, BRANCH, 1.0, material(1.0e6, 1.0)).unwrap();
    for _ in 0..30 {
        world.step(DT);
    }
    let held = world.motion(body(BRANCH)).unwrap().position;
    assert!(world.remove_connection(weld_id(BRANCH)));
    assert!(!world.contains_joint(weld_id(BRANCH)));
    for _ in 0..30 {
        world.step(DT);
        assert!(world.severed_connections().is_empty());
    }
    assert!(world.motion(body(BRANCH)).unwrap().position.y < held.y - 0.1 * scale.g * 0.25);
    assert_eq!(world.identity_fault(), None);
}
