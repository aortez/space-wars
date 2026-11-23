//! #326's reference cases on real physics. Each case's fixture bodies are
//! built in a world with their masses, inertias and surfaces, and struck or
//! pressed as the case does it. Each charge must be the case's own outcome.
//!
//! Some cases are not here. The connection cases, beam_connections and
//! fixed_step's moments, load welds, not contacts. failure_allocation's cases
//! are a host's choices of where a failure's excess goes. resting_support's
//! last press takes a measured plastic displacement, which charges never
//! measure.

use super::*;
use crate::world::ColliderMassProperties;
use engine_material::fixtures::{
    self, Body as Fixture, COMMON_TIME, GRAVITY, STEP_RATES, TICK, boulder, branch, landing_ship,
    planet, shell, ship, spaceling, stone, tank,
};
use engine_material::{SustainedLoad, sustained};

/// How far short of touching a striker starts, `wu`.
const GAP: f32 = 0.0005;
/// A movable fixture's collider is a ball this big, whatever radius its
/// surface declares, `wu`.
const BALL: f32 = 0.5;

/// The surface the host keeps behind a fixture body.
fn surface(fixture: &Fixture) -> ContactSurface {
    ContactSurface {
        region: fixture.region(),
        thickness: fixture.thickness,
        depth: Depth::Patch(fixture.extent),
        radius: fixture.radius,
    }
}

/// Inserts `fixture` as body `n` at `at`, moving at `velocity`, with the host
/// keeping its surface. A movable fixture is a ball with the fixture's mass
/// and inertia, its center of mass `lever` along y from the ball's center, so
/// a strike along x meets it that far off center. An immovable one is a fixed
/// wall whose face is at `at`, facing -x.
fn insert(
    world: &mut PhysicsWorld,
    host: &mut Host,
    n: u64,
    fixture: &Fixture,
    at: Vec2,
    velocity: Vec2,
    lever: f32,
) -> ContactSource {
    let (spec, shape) = match fixture.mass {
        Some(mass) => {
            let mut shape = ball(n, BALL, 1.0);
            shape.mass_properties = Some(ColliderMassProperties {
                center: Vec2::Y * lever,
                mass,
                inertia: fixture.inertia.unwrap(),
            });
            let spec = BodySpec {
                position: at,
                linear_velocity: velocity,
                can_sleep: false,
                ..Default::default()
            };
            (spec, shape)
        }
        None => {
            let spec = BodySpec {
                kind: BodyKind::Fixed,
                position: at + Vec2::X * 5.0,
                ..Default::default()
            };
            (spec, cuboid(n, 0, 5.0, 5.0))
        }
    };
    let source = add(world, host, n, spec, &[shape], fixture.material, None);
    host.surfaces.insert(source, surface(fixture));
    source
}

/// Where a striker of `target` at the origin starts.
fn start(target: &Fixture) -> Vec2 {
    let reach = if target.mass.is_some() {
        2.0 * BALL
    } else {
        BALL
    };
    -Vec2::X * (reach + GAP)
}

/// Fixture `a`, as body 1, moving at `velocity` toward fixture `b`, as body
/// 2, at rest at the origin. `a` meets `b` `lever` off its own center of mass.
fn strike(
    host: &mut Host,
    a: &Fixture,
    b: &Fixture,
    velocity: Vec2,
    lever: f32,
) -> (PhysicsWorld, [ContactSource; 2]) {
    let mut world = world(Vec2::ZERO);
    let sources = [
        insert(&mut world, host, 1, a, start(b), velocity, lever),
        insert(&mut world, host, 2, b, Vec2::ZERO, Vec2::ZERO, 0.0),
    ];
    (world, sources)
}

/// The case's outcome: `load` between `regions` as they stand, where fixtures
/// `a` and `b` meet.
fn case(a: &Fixture, b: &Fixture, regions: [&Region; 2], load: Impact) -> Outcome {
    impact(regions[0], regions[1], fixtures::geometry(a, b), load).unwrap()
}

/// Asserts that `charge` priced the case's `expected` outcome, and that the
/// host absorbed what it consumed from the surfaces it had `before`.
fn assert_case(
    label: &str,
    host: &Host,
    before: &BTreeMap<ContactSource, ContactSurface>,
    charge: &ContactCharge,
    expected: &Outcome,
) {
    let ContactCharge::Charged {
        sources, outcome, ..
    } = charge
    else {
        panic!("{label}: {charge:?}");
    };
    let measures = |outcome: &Outcome| {
        let ledger = &outcome.ledger;
        [
            ledger.available,
            ledger.peak_force,
            ledger.peak_stress[0],
            ledger.peak_stress[1],
            outcome.consumed[0],
            outcome.consumed[1],
        ]
    };
    for (actual, case) in measures(outcome).into_iter().zip(measures(expected)) {
        assert!(
            close(actual, case, 1.0e-6),
            "{label}: {outcome:?} vs {expected:?}"
        );
    }
    assert_eq!(outcome.yielded, expected.yielded, "{label}");
    assert_eq!(outcome.failed, expected.failed, "{label}");
    for side in 0..2 {
        let mut region = before[&sources[side]].region;
        region.absorb(outcome.consumed[side]).unwrap();
        assert_eq!(host.surfaces[&sources[side]].region, region, "{label}");
    }
}

/// Parts body 1 from body 2 and stops both, waits out their episode, then
/// sets body 1 moving at `speed` along x again from `from`. Three steps of
/// `DT` outlast capture's separation grace, 1/60 s.
fn again(world: &mut PhysicsWorld, from: Vec2, speed: f32) {
    for (n, at) in [(1, from - Vec2::X), (2, Vec2::ZERO)] {
        if world.body_kind(body(n)) == Some(BodyKind::Dynamic) {
            assert!(world.set_pose(body(n), at, 0.0, true));
            assert!(world.set_velocity(body(n), Vec2::ZERO, 0.0, true));
        }
    }
    for _ in 0..3 {
        world.step(DT);
    }
    assert!(world.set_pose(body(1), from, 0.0, true));
    assert!(world.set_velocity(body(1), Vec2::X * speed, 0.0, true));
}

#[test]
fn safe_bumps_cost_nothing_however_often_they_repeat() {
    // Each pair bumps a thousand times at half its safe-limit speed. Every
    // bump is its own impact, and none costs anything.
    for (a, b) in [
        (branch(), tank()),
        (stone(), ship()),
        (spaceling(), branch()),
        (shell(), tank()),
        (ship(), planet()),
    ] {
        let speed = 0.5 * fixtures::safe_limit_speed(&a, &b);
        let label = format!("{} on {} at {speed}", a.name, b.name);
        let mut host = Host::default();
        let (mut world, _) = strike(&mut host, &a, &b, Vec2::X * speed, 0.0);
        let fresh = host.surfaces.clone();
        let expected = case(
            &a,
            &b,
            [&a.region(), &b.region()],
            fixtures::head_on(&a, &b, speed),
        );
        assert_eq!(expected.consumed, [0.0; 2], "{label}");
        for _ in 0..1000 {
            let charges = first_charges(&mut world, &mut host);
            assert_eq!(charges.len(), 1, "{label}: {charges:?}");
            assert_case(&label, &host, &fresh, &charges[0], &expected);
            again(&mut world, start(&b), speed);
        }
        assert!(host.absorbed.is_empty(), "{label}");
    }
}

#[test]
fn single_strikes_are_priced_as_their_cases_price_them() {
    // Gentle and harder landings, wood against armor, the same stone on
    // tissue and on wood, two hulls of one material, and a shell with more
    // energy than the plate can take. Each strike is fresh.
    let mut strikes = Vec::new();
    for speed in [1.0, 1.6, 2.2, 13.3, 43.2, 100.0] {
        strikes.push(("gentle landings", landing_ship(), planet(), speed));
    }
    for speed in [15.0, 40.0, 80.0, 120.0, 240.0] {
        strikes.push(("wood vs armor", branch(), tank(), speed));
    }
    for target in [spaceling(), branch()] {
        strikes.push(("tissue vs wood", stone(), target, 10.0));
    }
    for speed in [20.0, 60.0] {
        strikes.push(("equal strength tie", ship(), ship(), speed));
    }
    for speed in [600.0, 1800.0] {
        strikes.push(("severe armor failure", shell(), tank(), speed));
    }
    for (name, a, b, speed) in strikes {
        let label = format!("{name}: {} on {} at {speed}", a.name, b.name);
        let mut host = Host::default();
        let (mut world, _) = strike(&mut host, &a, &b, Vec2::X * speed, 0.0);
        let fresh = host.surfaces.clone();
        let charges = first_charges(&mut world, &mut host);
        assert_eq!(charges.len(), 1, "{label}: {charges:?}");
        let expected = case(
            &a,
            &b,
            [&a.region(), &b.region()],
            fixtures::head_on(&a, &b, speed),
        );
        assert_case(&label, &host, &fresh, &charges[0], &expected);
    }
}

/// Fires fresh `a`s at `speed` into one `b` until `b` fails, and returns how
/// many it took. Each strike is priced as the case prices it, on what is left
/// of `b`.
fn fire_until_failure(a: &Fixture, b: &Fixture, speed: f32, limit: usize) -> usize {
    let mut world = world(Vec2::ZERO);
    let mut host = Host::default();
    // The plate's identity sorts after the shells', as the case's slots do.
    let plate = insert(&mut world, &mut host, 1000, b, Vec2::ZERO, Vec2::ZERO, 0.0);
    let mut fired = 0;
    while !host.surfaces[&plate].region.failed() {
        fired += 1;
        assert!(fired <= limit, "{} still stands", b.name);
        let n = fired as u64;
        let shot = insert(&mut world, &mut host, n, a, start(b), Vec2::X * speed, 0.0);
        let before = host.surfaces.clone();
        let charges = first_charges(&mut world, &mut host);
        assert_eq!(charges.len(), 1, "hit {fired}: {charges:?}");
        let regions = [&a.region(), &before[&plate].region];
        let expected = case(a, b, regions, fixtures::head_on(a, b, speed));
        let label = format!("hit {fired}");
        assert_case(&label, &host, &before, &charges[0], &expected);
        // Clear the shell away and stop the plate for the next.
        assert!(world.remove_entity(PhysicsId::new(n)));
        host.surfaces.remove(&shot);
        world.set_pose(body(1000), Vec2::ZERO, 0.0, true);
        world.set_velocity(body(1000), Vec2::ZERO, 0.0, true);
    }
    fired
}

#[test]
fn fresh_shells_wear_a_plate_down_hit_by_hit_until_it_fails() {
    let (hits, ..) = fixtures::hits_to_failure(&shell(), &tank(), 300.0, 40);
    assert_eq!(hits, 18);
    assert_eq!(fire_until_failure(&shell(), &tank(), 300.0, 40), hits);
}

#[test]
fn twice_the_plate_takes_twice_the_hits_and_twice_its_thickness_holds() {
    let plate = tank().with_volume(2.0 * tank().volume);
    let (hits, ..) = fixtures::hits_to_failure(&shell(), &plate, 300.0, 200);
    assert_eq!(hits, 36);
    assert_eq!(fire_until_failure(&shell(), &plate, 300.0, 200), hits);
    // A strike that dents the plate dents nothing when the plate is twice as
    // thick.
    let speed = 1.2 * fixtures::safe_limit_speed(&shell(), &tank());
    let mut dents = Vec::new();
    for thickness in [1.0, 2.0] {
        let plate = tank().with_thickness(thickness);
        let mut host = Host::default();
        let (mut world, _) = strike(&mut host, &shell(), &plate, Vec2::X * speed, 0.0);
        let fresh = host.surfaces.clone();
        let charges = first_charges(&mut world, &mut host);
        let load = fixtures::head_on(&shell(), &plate, speed);
        let expected = case(&shell(), &plate, [&shell().region(), &plate.region()], load);
        let label = format!("thickness {thickness}");
        assert_case(&label, &host, &fresh, &charges[0], &expected);
        dents.push(charged(&charges[0]).1.consumed[1]);
    }
    assert!(dents[0] > 0.0 && dents[1] == 0.0, "{dents:?}");
}

#[test]
fn a_glancing_strike_is_priced_on_its_normal_approach() {
    let (a, b) = (ship(), planet());
    let mass = fixtures::head_on(&a, &b, 0.0).effective_mass;
    let mut energies = Vec::new();
    for angle in [0.0_f32, 30.0, 60.0, 80.0] {
        let incidence = angle.to_radians();
        let velocity = Vec2::new(libm::cosf(incidence), libm::sinf(incidence)) * 20.0;
        let mut host = Host::default();
        let (mut world, _) = strike(&mut host, &a, &b, velocity, 0.0);
        let fresh = host.surfaces.clone();
        let charges = first_charges(&mut world, &mut host);
        let load = Impact {
            closing_speed: 20.0 * libm::cosf(incidence),
            effective_mass: mass,
        };
        let expected = case(&a, &b, [&a.region(), &b.region()], load);
        let label = format!("incidence {angle}");
        assert_case(&label, &host, &fresh, &charges[0], &expected);
        energies.push(charged(&charges[0]).1.ledger.available);
    }
    assert!(
        energies.windows(2).all(|pair| pair[1] < pair[0]),
        "{energies:?}"
    );
}

#[test]
fn an_off_center_strike_is_priced_on_the_mass_its_lever_arm_leaves() {
    // The boulder meets the tank 0 to 3 off its center of mass, so part of
    // its approach turns it.
    let (a, b) = (boulder(), tank());
    let mut energies = Vec::new();
    for lever in [0.0, 1.0, 2.0, 3.0] {
        let striker = ContactBody::rotating(a.mass.unwrap(), a.inertia.unwrap(), lever);
        let load = Impact {
            closing_speed: 3.0,
            effective_mass: effective_mass(striker, b.contact()).unwrap(),
        };
        let mut host = Host::default();
        let (mut world, _) = strike(&mut host, &a, &b, Vec2::X * 3.0, lever);
        let fresh = host.surfaces.clone();
        let charges = first_charges(&mut world, &mut host);
        let expected = case(&a, &b, [&a.region(), &b.region()], load);
        let label = format!("lever {lever}");
        assert_case(&label, &host, &fresh, &charges[0], &expected);
        energies.push(charged(&charges[0]).1.ledger.available);
        let spin = world.motion(body(1)).unwrap().angular_velocity;
        assert_eq!(spin == 0.0, lever == 0.0, "lever {lever}: {spin}");
    }
    assert!(
        energies.windows(2).all(|pair| pair[1] < pair[0]),
        "{energies:?}"
    );
}

#[test]
fn a_strike_on_a_seam_between_colliders_is_priced_once() {
    // The shell strikes the tank where its face is split into colliders, so
    // two of them meet it at one point. The strike is the case's one contact,
    // not one per collider.
    let (a, b) = (shell(), tank());
    let load = fixtures::head_on(&a, &b, 300.0);
    let expected = case(&a, &b, [&a.region(), &b.region()], load);
    for parts in [1_u16, 2, 4] {
        let mut world = world(Vec2::ZERO);
        let mut host = Host::default();
        insert(
            &mut world,
            &mut host,
            1,
            &a,
            start(&b),
            Vec2::X * 300.0,
            0.0,
        );
        let half = 1.0 / f32::from(parts);
        let pieces: Vec<_> = (0..parts)
            .map(|part| {
                let mut piece = cuboid(2, part, BALL, half);
                piece.local_position.y = -1.0 + (2.0 * f32::from(part) + 1.0) * half;
                piece.mass_properties = Some(ColliderMassProperties {
                    center: Vec2::ZERO,
                    mass: b.mass.unwrap() / f32::from(parts),
                    inertia: b.inertia.unwrap() / f32::from(parts),
                });
                piece
            })
            .collect();
        let resting = BodySpec {
            can_sleep: false,
            ..Default::default()
        };
        let plate = add(&mut world, &mut host, 2, resting, &pieces, b.material, None);
        host.surfaces.insert(plate, surface(&b));
        let fresh = host.surfaces.clone();
        let charges = first_charges(&mut world, &mut host);
        assert_eq!(charges.len(), 1, "{parts} parts: {charges:?}");
        let label = format!("{parts} parts");
        assert_case(&label, &host, &fresh, &charges[0], &expected);
        let meeting: BTreeSet<_> = world.loading_batches()[0]
            .contacts
            .iter()
            .map(|record| record.colliders[1])
            .collect();
        assert_eq!(meeting.len(), parts.min(2) as usize, "{parts} parts");
    }
}

/// A world with the case's gravity in which a box 2 wide, of `mass`, rests
/// flat on a fixed wood plank, as the tank rests in the case. Returns the
/// world, the host and the plank's source.
fn pressed(mass: f32) -> (PhysicsWorld, Host, ContactSource) {
    let mut world = world(-Vec2::Y * GRAVITY);
    let mut host = Host::default();
    let wood = floor(&mut world, &mut host, branch().material, Depth::Patch(None));
    let block = resting_box(&mut world, &mut host, 2, mass / 2.0, false);
    // The surfaces are the fixtures', but the plank's top and the box's face
    // are flat.
    for (source, fixture) in [(wood, branch()), (block, tank())] {
        let surface = ContactSurface {
            radius: None,
            ..surface(&fixture)
        };
        host.surfaces.insert(source, surface);
    }
    (world, host, wood)
}

/// The case's press of `force` on the plank for a step of `duration`.
fn press(force: f32, duration: f32) -> SustainedLoad {
    SustainedLoad {
        force,
        width: 2.0,
        section: branch().section(2.0),
        duration,
        measured_plastic_displacement: None,
    }
}

/// Steps a box of `force`'s weight on the plank at `dt` until the plank fails
/// or `limit` steps pass, pricing each step's support against the case's
/// press. Returns the work each step consumed from the plank.
fn crush(force: f32, dt: f32, limit: usize) -> Vec<f32> {
    let (mut world, mut host, wood) = pressed(force / GRAVITY);
    let mut works = Vec::new();
    while !host.surfaces[&wood].region.failed() && works.len() < limit {
        let before = host.surfaces[&wood].region;
        world.step(dt);
        assert_eq!(world.charge_contact_impacts(&mut host), Ok(Vec::new()));
        let charges = world.charge_contact_supports(&mut host).unwrap();
        let [charge] = charges.as_slice() else {
            panic!("step {}: {charges:?}", works.len());
        };
        let (carried, width, duration, outcomes) = supported(charge);
        assert!(close(carried, force, 1.0e-6), "{carried} vs {force}");
        assert!(close(width, 2.0, 1.0e-6), "{width}");
        assert!(close(duration, dt, 1.0e-6), "{duration} vs {dt}");
        let expected = sustained(&before, press(force, dt)).unwrap();
        let work = outcomes[0].consumed[0];
        assert!(
            close(work, expected.consumed[0], 1.0e-5),
            "step {}: {work} vs {expected:?}",
            works.len()
        );
        works.push(work);
    }
    works
}

/// How many ticks of the case's press of `force` at `dt` fail the plank.
fn ticks_to_failure(force: f32, dt: f32) -> usize {
    let mut plank = branch().region();
    let mut ticks = 0;
    while !plank.failed() {
        assert!(ticks < 1000, "the plank holds {force}");
        fixtures::press(&mut plank, press(force, dt));
        ticks += 1;
    }
    ticks
}

#[test]
fn a_resting_load_costs_nothing_and_an_overload_fails_the_plank_in_its_time() {
    // The tank's weight rests on the plank for a thousand ticks. The press of
    // 1.5 times the plank's safe stress fails it on the case's tick, and the
    // press of 1.05 times a tick late. The solver's force is a part in ten
    // million short, the overstress makes that 1.7 parts in a million of each
    // tick's work, and over the case's ticks that leaves 3.9e-4 of the plank.
    let rest = tank().mass.unwrap() * GRAVITY;
    let works = crush(rest, TICK, 1000);
    assert_eq!(works.len(), 1000);
    assert!(works.iter().all(|&work| work == 0.0), "{works:?}");
    for (force, expected, late) in [(6300.0, 12, 1), (9000.0, 2, 0)] {
        assert_eq!(ticks_to_failure(force, TICK), expected);
        assert_eq!(crush(force, TICK, 100).len(), expected + late, "{force}");
    }
}

#[test]
fn an_overload_wears_the_plank_alike_at_every_step_rate() {
    // The 1.05x press at 30, 60, 120 and 240 Hz consumes the same work by
    // 1/30 s. The case fails the plank at 0.2 s at every rate, and the solver
    // a tick after it.
    for rate in STEP_RATES {
        let dt = 1.0 / rate;
        let works = crush(6300.0, dt, 1000);
        let common = (rate * COMMON_TIME).round() as usize;
        let by_then: f32 = works[..common].iter().sum();
        assert!(close(by_then, 40.0, 1.0e-5), "{rate} Hz: {by_then}");
        let expected = ticks_to_failure(6300.0, dt);
        assert_eq!(expected, (0.2 * rate).round() as usize, "{rate} Hz");
        assert_eq!(works.len(), expected + 1, "{rate} Hz");
    }
}
