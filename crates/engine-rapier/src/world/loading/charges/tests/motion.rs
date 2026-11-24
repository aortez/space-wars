//! Charges where bodies and their supports move, turn, split or hang
//! together: a common velocity, moving and turning supports, a CCD shell, a
//! fragment, bounces, a stack, a cart and the rover.

use super::*;
use crate::rover::{RoverAssembly, RoverSpawnPose, RoverSpec};
use engine_material::WOOD;

/// Steps once, and prices the step's impacts and then its supports.
fn step(world: &mut PhysicsWorld, host: &mut Host) -> (Vec<ContactCharge>, Vec<SupportCharge>) {
    world.step(DT);
    let impacts = world.charge_contact_impacts(host).unwrap();
    let supports = world.charge_contact_supports(host).unwrap();
    (impacts, supports)
}

/// The force of a step's one support.
fn force(supports: &[SupportCharge]) -> f32 {
    let [support] = supports else {
        panic!("{supports:?}");
    };
    supported(support).0
}

/// Body 1, a kinematic rock platform 10 wide with friction, its top at zero,
/// moving at `velocity` and turning at `spin` about its center, 0.5 below
/// its top.
fn platform(world: &mut PhysicsWorld, host: &mut Host, velocity: Vec2, spin: f32) {
    let mut plank = cuboid(1, 0, 5.0, 0.5);
    plank.friction = 1.0;
    let moving = BodySpec {
        kind: BodyKind::KinematicVelocity,
        position: -Vec2::Y * 0.5,
        linear_velocity: velocity,
        angular_velocity: spin,
        ..Default::default()
    };
    add(world, host, 1, moving, &[plank], ROCK, None);
}

/// Body 2, a hull box 2 wide and 1 tall with friction, resting on the
/// platform and moving with it at `velocity` and `spin`. Returns its mass.
fn rider(world: &mut PhysicsWorld, host: &mut Host, velocity: Vec2, spin: f32) -> f32 {
    let mut shape = cuboid(2, 0, 1.0, 0.5);
    shape.friction = 1.0;
    let riding = BodySpec {
        position: Vec2::Y * 0.5,
        linear_velocity: velocity,
        angular_velocity: spin,
        can_sleep: false,
        ..Default::default()
    };
    add(world, host, 2, riding, &[shape], HULL, None);
    world.body_mass(body(2)).unwrap()
}

#[test]
fn a_common_velocity_changes_no_charge() {
    // The head-on strike of `balls`, and a box resting on a platform that
    // carries it, each with the whole scene moving at a common velocity. The
    // strike is priced exactly alike. The box's support carries its weight
    // to within f32 rounding at the scene's speed: exactly at rest, and to
    // 3.2e-5 at 39 and 7.6e-4 at 331.
    let mut first = None;
    for (shift, tolerance) in [
        (Vec2::ZERO, 0.0),
        (Vec2::new(37.0, -12.0), 1.0e-4),
        (Vec2::new(-300.0, 140.0), 1.0e-3),
    ] {
        let mut host = Host::default();
        let (mut struck, _) = balls(&mut host, [Some(0.5); 2]);
        for n in [1, 2] {
            let velocity = struck.motion(body(n)).unwrap().linear_velocity;
            assert!(struck.set_velocity(body(n), velocity + shift, 0.0, true));
        }
        let charges = first_charges(&mut struck, &mut host);
        assert_eq!(charges.len(), 1, "{shift:?}: {charges:?}");
        let (speed, outcome) = charged(&charges[0]);
        let (first_speed, first_outcome) = *first.get_or_insert((speed, outcome));
        assert_eq!(speed, first_speed, "{shift:?}");
        assert_eq!(outcome, first_outcome, "{shift:?}");

        let mut world = world(-Vec2::Y * GRAVITY);
        let mut host = Host::default();
        platform(&mut world, &mut host, shift, 0.0);
        let weight = GRAVITY * rider(&mut world, &mut host, shift, 0.0);
        for tick in 0..60 {
            let (impacts, supports) = step(&mut world, &mut host);
            assert_eq!(impacts, Vec::new(), "{shift:?}, tick {tick}");
            let carried = force(&supports);
            assert!(
                close(carried, weight, tolerance),
                "{shift:?}, tick {tick}: {carried} vs {weight}"
            );
        }
    }
}

#[test]
fn a_turning_support_strikes_with_the_velocity_of_its_point() {
    // A paddle 4 long starts turning at 3 about its center and strikes a
    // resting ball 1.5 from its center, at 4.5. The paddle gives no way, so
    // the strike is priced on the ball's whole mass. A paddle turned by its
    // velocity and one moved by its pose strike alike, in a whole step and in
    // one that a CCD bullet elsewhere splits. The bullet reaches its wall in
    // the second step, so the paddle starts turning then.
    let spin = 3.0;
    for kind in [BodyKind::KinematicVelocity, BodyKind::KinematicPosition] {
        for substeps in [1, 4] {
            let label = format!("{kind:?}, {substeps} substeps");
            let mut world = world_with(Vec2::ZERO, substeps);
            let mut host = Host::default();
            let still = BodySpec {
                kind,
                ..Default::default()
            };
            let paddle = add(
                &mut world,
                &mut host,
                1,
                still,
                &[cuboid(1, 0, 2.0, 0.1)],
                ROCK,
                None,
            );
            let resting = BodySpec {
                position: Vec2::new(1.5, 0.6005),
                can_sleep: false,
                ..Default::default()
            };
            let struck = add(
                &mut world,
                &mut host,
                2,
                resting,
                &[ball(2, 0.5, 1.0)],
                HULL,
                Some(0.5),
            );
            let wall = BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(26.0, 5.0),
                ..Default::default()
            };
            assert!(world.insert_body(body(3), wall, &[cuboid(3, 0, 0.5, 3.0)]));
            let bullet = BodySpec {
                position: Vec2::new(20.0, 5.0),
                linear_velocity: Vec2::X * 400.0,
                ccd_enabled: true,
                can_sleep: false,
                ..Default::default()
            };
            assert!(world.insert_body(body(4), bullet, &[ball(4, 0.1, 1.0)]));
            let fresh = host.surfaces.clone();
            let quiet = step(&mut world, &mut host);
            assert_eq!(quiet, (Vec::new(), Vec::new()), "{label}");
            let turned = if kind == BodyKind::KinematicVelocity {
                world.set_velocity(body(1), Vec2::ZERO, spin, true)
            } else {
                world.set_next_kinematic_pose(body(1), Vec2::ZERO, spin * DT)
            };
            assert!(turned, "{label}");
            world.step(DT);
            let split = world.loading_batches().len() > 1;
            assert_eq!(split, substeps > 1, "{label}");
            let charges = world.charge_contact_impacts(&mut host).unwrap();
            assert_eq!(charges.len(), 1, "{label}: {charges:?}");
            let (closing, outcome) = charged(&charges[0]);
            assert!(close(closing, 4.5, 1.0e-5), "{label}: {closing}");
            let mass = world.body_mass(body(2)).unwrap();
            let width = patch_width_from_radii(0.5, None).unwrap();
            let sources = [paddle, struck];
            let expected = law(&fresh, sources, width, 0.5 * mass * 4.5 * 4.5);
            assert_priced(&outcome, &expected, &host, sources);
        }
    }
}

#[test]
fn a_moving_support_is_priced_at_what_holds_its_load() {
    // A box rests on a platform that starts rising at 1, then speeds up at 5
    // and slows down at 5. Its support carries the box's mass times gravity
    // and the platform's acceleration. The step the platform jumps to 1 is
    // folded into the box's open episode, as Limits says, so it is priced
    // neither as an impact nor, since its points approach, as a support.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    platform(&mut world, &mut host, Vec2::ZERO, 0.0);
    let mass = rider(&mut world, &mut host, Vec2::ZERO, 0.0);
    let mut speed = 0.0;
    for tick in 0..240 {
        let acceleration = match tick {
            0..120 => 0.0,
            120..180 => 5.0,
            _ => -5.0,
        };
        if tick == 60 {
            speed = 1.0;
        }
        speed += acceleration * DT;
        assert!(world.set_velocity(body(1), Vec2::Y * speed, 0.0, true));
        let (impacts, supports) = step(&mut world, &mut host);
        assert_eq!(impacts, Vec::new(), "tick {tick}");
        if tick == 60 {
            assert_eq!(supports, Vec::new());
            continue;
        }
        let carried = force(&supports);
        let expected = mass * (GRAVITY + acceleration);
        assert!(
            close(carried, expected, 1.0e-4),
            "tick {tick}: {carried} vs {expected}"
        );
    }
}

#[test]
fn a_tilting_support_carries_the_weight_along_its_normal() {
    // A plank tilts at 0.5 about its center, and a box rides it, its center 1
    // above the plank's. The support carries the share of the box's weight
    // along the plank's normal, less what swings the box about the plank's
    // center, a fortieth of it. The solve sees the turn a step late, so the
    // first step carries the whole weight.
    let (spin, arm) = (0.5, 1.0);
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    platform(&mut world, &mut host, Vec2::ZERO, spin);
    let mass = rider(&mut world, &mut host, -Vec2::X * spin * arm, spin);
    for tick in 0..48 {
        let angle = world.motion(body(1)).unwrap().angle;
        let (impacts, supports) = step(&mut world, &mut host);
        assert_eq!(impacts, Vec::new(), "tick {tick}");
        let expected = if tick == 0 {
            mass * GRAVITY
        } else {
            mass * (GRAVITY * angle.cos() - spin * spin * arm)
        };
        let carried = force(&supports);
        assert!(
            close(carried, expected, 1.0e-4),
            "tick {tick}: {carried} vs {expected}"
        );
    }
}

/// A shell at 400, with CCD or not, 1.4 short of a free box. Returns the
/// world, the host and the shell's and the box's sources.
fn shell_and_box(ccd: bool) -> (PhysicsWorld, Host, [ContactSource; 2]) {
    let mut world = world_with(Vec2::ZERO, 4);
    let mut host = Host::default();
    let shot = BodySpec {
        position: -Vec2::X * 2.0,
        linear_velocity: Vec2::X * 400.0,
        ccd_enabled: ccd,
        can_sleep: false,
        ..Default::default()
    };
    let shell = add(
        &mut world,
        &mut host,
        1,
        shot,
        &[ball(1, 0.1, 8.0)],
        HULL,
        Some(0.1),
    );
    let resting = BodySpec {
        can_sleep: false,
        ..Default::default()
    };
    let target = add(
        &mut world,
        &mut host,
        2,
        resting,
        &[cuboid(2, 0, 0.5, 0.5)],
        ROCK,
        None,
    );
    (world, host, [shell, target])
}

#[test]
fn a_ccd_shell_striking_a_free_body_is_priced_on_their_reduced_mass() {
    // A shell at 400 reaches a free box within a step. Without CCD it passes
    // through the box unpriced. With CCD it stops at the box's face, and the
    // next step, which CCD splits, strikes the box. The strike is priced on
    // the shell's and the box's reduced mass.
    let (mut world, mut host, _) = shell_and_box(false);
    for _ in 0..3 {
        assert_eq!(step(&mut world, &mut host), (Vec::new(), Vec::new()));
    }
    let past = world.motion(body(1)).unwrap().position.x;
    assert!(past > 0.6, "{past}");
    let still = world.motion(body(2)).unwrap().linear_velocity;
    assert_eq!(still, Vec2::ZERO);

    let (mut world, mut host, sources) = shell_and_box(true);
    let fresh = host.surfaces.clone();
    let charges = first_charges(&mut world, &mut host);
    assert!(world.loading_batches().len() > 1, "the step wasn't split");
    assert_eq!(charges.len(), 1, "{charges:?}");
    let (closing, outcome) = charged(&charges[0]);
    assert!(close(closing, 400.0, 1.0e-6), "{closing}");
    let [a, b] = [1, 2].map(|n| world.body_mass(body(n)).unwrap());
    let energy = 0.5 * a * b / (a + b) * 400.0 * 400.0;
    let width = patch_width_from_radii(0.1, None).unwrap();
    let expected = law(&fresh, sources, width, energy);
    assert_priced(&outcome, &expected, &host, sources);
}

#[test]
fn each_bounce_is_priced_on_the_floor_the_bounces_before_it_left() {
    // A heavy ball strikes a wooden floor at 12 and bounces, coming back at
    // under half its speed each time, since Rapier averages its restitution
    // with the floor's. Each bounce is its own impact, priced on the ball's
    // whole mass at the speed it arrives at, against the floor as the bounces
    // before it left it. The first two leave the floor less than the third
    // would take from a fresh one, so the third takes what is left, and the
    // floor fails.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    let ground = floor(&mut world, &mut host, WOOD, Depth::Fixed(SECTION.depth));
    host.surfaces.get_mut(&ground).unwrap().region = Region::new(WOOD, 9.4).unwrap();
    let mut heavy = ball(2, 0.5, 8.0);
    heavy.restitution = 0.9;
    let falling = BodySpec {
        position: Vec2::Y * 0.6,
        linear_velocity: -Vec2::Y * 12.0,
        can_sleep: false,
        ..Default::default()
    };
    let striker = add(&mut world, &mut host, 2, falling, &[heavy], HULL, Some(0.5));
    let fresh = host.surfaces.clone();
    let mass = world.body_mass(body(2)).unwrap();
    let width = patch_width_from_radii(0.5, None).unwrap();
    let sources = [ground, striker];
    let mut speeds = Vec::new();
    let mut wear = Vec::new();
    for _ in 0..1200 {
        let before = host.surfaces.clone();
        let (impacts, _) = step(&mut world, &mut host);
        for charge in &impacts {
            let (speed, outcome) = charged(charge);
            let label = format!("bounce {} at {speed}", speeds.len());
            let energy = 0.5 * mass * speed * speed;
            let expected = law(&before, sources, width, energy);
            let available = outcome.ledger.available;
            assert!(
                close(available, expected.ledger.available, 1.0e-5),
                "{label}: {available} vs {expected:?}"
            );
            for side in 0..2 {
                assert!(
                    close(outcome.consumed[side], expected.consumed[side], 1.0e-5),
                    "{label}: {outcome:?} vs {expected:?}"
                );
            }
            assert_eq!(outcome.failed, expected.failed, "{label}");
            if speeds.len() == 2 {
                let whole = law(&fresh, sources, width, energy).consumed[0];
                let left = before[&ground].region.remaining();
                assert!(whole > left, "{label}: {whole} vs {left}");
                let took = outcome.consumed[0];
                assert!(close(took, left, 1.0e-5), "{label}: {took} vs {left}");
            }
            speeds.push(speed);
            wear.push(outcome.consumed[0]);
        }
    }
    assert!(
        speeds.windows(2).all(|pair| pair[1] < pair[0]),
        "{speeds:?}"
    );
    assert!(speeds.len() > 3, "{speeds:?}");
    assert!(wear[..2].iter().all(|&work| work > 0.0), "{wear:?}");
    assert!(wear[3..].iter().all(|&work| work == 0.0), "{wear:?}");
    let left = host.surfaces[&ground].region;
    assert!(left.failed(), "{left:?}");
    let lost = fresh[&ground].region.remaining_precise() - left.remaining_precise();
    let total: f64 = wear.iter().map(|&work| f64::from(work)).sum();
    assert!((lost - total).abs() <= 1.0e-6 * total, "{lost} vs {total}");
}

#[test]
fn a_fragment_split_off_in_flight_is_priced_on_its_own_mass_and_region() {
    // A bar of two halves flies at 5 toward a wall, and the host breaks it
    // into two bodies, each with half the bar's region. The halves touch at
    // the cut, but share the bar's motion, so the split itself costs nothing.
    // The front half strikes the wall on its own mass, as Limits says of a
    // body pushed through a contact already open: the back half's push
    // through the cut isn't in the strike's budget.
    let mut world = world(Vec2::ZERO);
    let mut host = Host::default();
    let mut halves = [cuboid(1, 0, 1.0, 0.2), cuboid(1, 1, 1.0, 0.2)];
    halves[0].local_position.x = -1.0;
    halves[1].local_position.x = 1.0;
    let flying = BodySpec {
        linear_velocity: Vec2::X * 5.0,
        can_sleep: false,
        ..Default::default()
    };
    let bar = add(&mut world, &mut host, 1, flying, &halves, HULL, Some(0.2));
    let fixed = BodySpec {
        kind: BodyKind::Fixed,
        position: Vec2::X * 4.0,
        ..Default::default()
    };
    let wall = add(
        &mut world,
        &mut host,
        2,
        fixed,
        &[cuboid(2, 0, 1.0, 5.0)],
        ROCK,
        None,
    );
    for _ in 0..3 {
        assert_eq!(step(&mut world, &mut host), (Vec::new(), Vec::new()));
    }
    let motion = world.motion(body(1)).unwrap();
    let whole = host.surfaces.remove(&bar).unwrap();
    assert!(world.remove_entity(PhysicsId::new(1)));
    assert!(!world.loading_region_is_live(bar.region));
    let (front, back) = whole.region.split(0.5).unwrap();
    let mut pieces = Vec::new();
    for (n, offset, region) in [(3, -1.0, back), (4, 1.0, front)] {
        let piece = BodySpec {
            position: motion.position + Vec2::X * offset,
            linear_velocity: motion.linear_velocity,
            can_sleep: false,
            ..Default::default()
        };
        let source = add(
            &mut world,
            &mut host,
            n,
            piece,
            &[cuboid(n, 0, 1.0, 0.2)],
            HULL,
            Some(0.2),
        );
        host.surfaces.get_mut(&source).unwrap().region = region;
        pieces.push(source);
    }
    let fresh = host.surfaces.clone();
    let mut cut = Vec::new();
    let mut struck = Vec::new();
    for tick in 0..120 {
        let (impacts, _) = step(&mut world, &mut host);
        for event in world
            .loading_batches()
            .iter()
            .flat_map(|batch| &batch.events)
        {
            if event.sources == [pieces[0], pieces[1]] {
                cut.push((tick, *event));
            }
        }
        struck.extend(impacts);
    }
    let (_, opened) = cut[0];
    assert_eq!(opened.transition, EpisodeTransition::Support, "{cut:?}");
    assert_eq!(opened.closing_speed, 0.0, "{cut:?}");
    assert_eq!(struck.len(), 1, "{struck:?}");
    let ContactCharge::Charged { sources, .. } = struck[0] else {
        panic!("{struck:?}");
    };
    assert_eq!(sources, [wall, pieces[1]]);
    let (closing, outcome) = charged(&struck[0]);
    assert!(close(closing, 5.0, 1.0e-6), "{closing}");
    let mass = world.body_mass(body(4)).unwrap();
    // The half's face meets the wall across its height.
    let width = 0.4;
    let expected = law(&fresh, sources, width, 0.5 * mass * 5.0 * 5.0);
    assert_priced(&outcome, &expected, &host, sources);
}

#[test]
fn a_stack_rests_each_weight_on_the_support_below_it() {
    // Three boxes stand on one another on a floor. Once the stack settles,
    // each support carries the weight of every box above it.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    let mut levels = vec![floor(
        &mut world,
        &mut host,
        ROCK,
        Depth::Fixed(SECTION.depth),
    )];
    for (level, n) in [2_u64, 3, 4].into_iter().enumerate() {
        let at = BodySpec {
            position: Vec2::Y * (0.5 + level as f32),
            can_sleep: false,
            ..Default::default()
        };
        let shape = cuboid(n, 0, 1.0, 0.5);
        levels.push(add(&mut world, &mut host, n, at, &[shape], HULL, None));
    }
    let weight = GRAVITY * world.body_mass(body(2)).unwrap();
    for _ in 0..120 {
        step(&mut world, &mut host);
    }
    for tick in 0..120 {
        let (impacts, supports) = step(&mut world, &mut host);
        assert_eq!(impacts, Vec::new(), "tick {tick}");
        assert_eq!(supports.len(), 3, "tick {tick}: {supports:?}");
        for support in &supports {
            let SupportCharge::Charged { sources, force, .. } = support else {
                panic!("tick {tick}: {supports:?}");
            };
            let below = levels.iter().position(|s| *s == sources[0]).unwrap();
            assert_eq!(sources[1], levels[below + 1], "tick {tick}");
            let carried = (3 - below) as f32 * weight;
            assert!(
                close(*force, carried, 1.0e-5),
                "tick {tick}, level {below}: {force} vs {carried}"
            );
        }
    }
}

/// The force of the support between `sources`, among a step's supports.
fn force_between(supports: &[SupportCharge], sources: [ContactSource; 2]) -> Option<f32> {
    supports.iter().find_map(|support| match support {
        SupportCharge::Charged {
            sources: between,
            force,
            ..
        } if *between == sources => Some(*force),
        _ => None,
    })
}

#[test]
fn a_box_on_a_cart_that_speeds_up_rests_its_weight_on_the_cart() {
    // A box rests on a cart on a frictionless floor. Once they settle, a force
    // speeds the cart up at 2, and friction carries the box along. The box's
    // support carries its weight. Once the cart slides along the floor faster
    // than a support's points may move, its own support isn't priced.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    let ground = floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    let mut cart = cuboid(2, 0, 2.0, 0.25);
    cart.density = 4.0;
    let rolling = BodySpec {
        position: Vec2::Y * 0.25,
        can_sleep: false,
        ..Default::default()
    };
    let carrier = add(&mut world, &mut host, 2, rolling, &[cart], HULL, None);
    let mut shape = cuboid(3, 0, 0.5, 0.5);
    shape.friction = 1.0;
    let riding = BodySpec {
        position: Vec2::Y,
        can_sleep: false,
        ..Default::default()
    };
    let rider = add(&mut world, &mut host, 3, riding, &[shape], HULL, None);
    let [cart_mass, box_mass] = [2, 3].map(|n| world.body_mass(body(n)).unwrap());
    let weight = GRAVITY * box_mass;
    for _ in 0..120 {
        step(&mut world, &mut host);
    }
    assert!(world.apply_force(body(2), Vec2::X * 2.0 * (cart_mass + box_mass), true));
    for tick in 0..240 {
        let (_, supports) = step(&mut world, &mut host);
        let carried = force_between(&supports, [carrier, rider]);
        assert!(
            carried.is_some_and(|carried| close(carried, weight, 1.0e-5)),
            "tick {tick}: {carried:?} vs {weight}"
        );
        let speed = world.motion(body(2)).unwrap().linear_velocity.x;
        if speed > 0.2 {
            let sliding = force_between(&supports, [ground, carrier]);
            assert_eq!(sliding, None, "tick {tick}");
        }
    }
    let speed = world.motion(body(2)).unwrap().linear_velocity.x;
    assert!(close(speed, 2.0 * 240.0 * DT, 1.0e-3), "{speed}");
}

#[test]
fn a_rover_lands_on_its_wheels_and_rests_its_weight_on_them() {
    // The rover's wheels hang from its chassis on sprung slots, and each of
    // its three bodies has its own region. Dropped 2 onto rock, each wheel
    // lands on its own mass, since joints aren't in an impact's budget. Once
    // it settles, the wheels' supports carry the whole rover's weight through
    // the slots.
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    let ground = floor(&mut world, &mut host, ROCK, Depth::Fixed(SECTION.depth));
    let entity = PhysicsId::new(5);
    let spec = RoverSpec::default();
    let spawn = RoverSpawnPose {
        wheel_center: Vec2::Y * (2.0 + spec.wheel_radius),
        up_angle: std::f32::consts::FRAC_PI_2,
    };
    let rover = RoverAssembly::insert_at(&mut world, entity, spawn, spec).unwrap();
    let mut parts = Vec::new();
    for (part, id) in rover.bodies().into_iter().enumerate() {
        let region = RegionId {
            entity,
            part: part as u32,
        };
        let colliders: Vec<_> = world
            .collider_ids()
            .filter(|&collider| world.collider_body(collider) == Some(id))
            .collect();
        let radius = (part > 0).then_some(spec.wheel_radius);
        let source = bind(&mut world, &mut host, region, id, colliders, HULL, radius);
        parts.push(source);
    }
    let masses = rover.bodies().map(|id| world.body_mass(id).unwrap());
    let weight = GRAVITY * masses.iter().sum::<f32>();
    let mut landings = Vec::new();
    let mut settled = Vec::new();
    for _ in 0..360 {
        let (impacts, supports) = step(&mut world, &mut host);
        landings.extend(impacts);
        settled = supports;
    }
    assert_eq!(landings.len(), 2, "{landings:?}");
    let mut wheels = BTreeSet::new();
    for landing in &landings {
        let ContactCharge::Charged {
            sources,
            closing_speed,
            outcome,
            ..
        } = landing
        else {
            panic!("{landings:?}");
        };
        assert_eq!(sources[0], ground);
        let wheel = parts.iter().position(|s| *s == sources[1]).unwrap();
        assert!(wheel > 0, "the chassis struck the floor");
        wheels.insert(wheel);
        let energy = 0.5 * masses[wheel] * closing_speed * closing_speed;
        assert!(
            close(outcome.ledger.available, energy, 1.0e-5),
            "wheel {wheel}: {} vs {energy}",
            outcome.ledger.available
        );
    }
    assert_eq!(wheels.len(), 2, "{landings:?}");
    assert_eq!(settled.len(), 2, "{settled:?}");
    let carried: f32 = settled.iter().map(|support| supported(support).0).sum();
    assert!(close(carried, weight, 1.0e-4), "{carried} vs {weight}");
}
