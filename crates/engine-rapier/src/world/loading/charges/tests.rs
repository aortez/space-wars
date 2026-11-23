use super::*;
use engine_material::{ContactBody, HULL, Material, ROCK, SOFT_TISSUE, effective_mass};

const DT: f32 = 1.0 / 120.0;
const SECTION: Section = Section::new(1.0, 0.5);

fn body(n: u64) -> BodyId {
    BodyId::new(PhysicsId::new(n), BodyRole::PRIMARY)
}
fn collider(n: u64, part: u16) -> ColliderId {
    ColliderId::new(PhysicsId::new(n), ColliderRole::PRIMARY, part)
}

fn ball(n: u64, radius: f32, density: f32) -> ColliderSpec {
    let mut spec = ColliderSpec::ball(collider(n, 0), radius);
    spec.density = density;
    spec.friction = 0.0;
    spec
}
fn cuboid(n: u64, part: u16, half_width: f32, half_height: f32) -> ColliderSpec {
    let mut spec = ColliderSpec::cuboid(collider(n, part), half_width, half_height);
    spec.friction = 0.0;
    spec
}

/// The host's surfaces, charged in place, and the work each absorbed.
#[derive(Default)]
struct Host {
    surfaces: BTreeMap<ContactSource, ContactSurface>,
    protected: BTreeSet<ContactSource>,
    absorbed: Vec<(ContactSource, f32)>,
}

impl ContactSurfaces for Host {
    fn surface(&self, source: ContactSource) -> Option<ContactSurface> {
        self.surfaces.get(&source).copied()
    }
    fn absorb(&mut self, source: ContactSource, work: f32) {
        self.absorbed.push((source, work));
        self.surfaces
            .get_mut(&source)
            .unwrap()
            .region
            .absorb(work)
            .unwrap();
    }
    fn protected(&self, source: ContactSource) -> bool {
        self.protected.contains(&source)
    }
}

/// A world whose capture admits speculative arrivals.
fn world(gravity: Vec2) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity,
        collect_events: false,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_speculative_arrivals().unwrap();
    world
}

/// Body `n`, its colliders bound to one source on its own region, with the
/// host keeping a unit volume of `material` behind it.
fn add(
    world: &mut PhysicsWorld,
    host: &mut Host,
    n: u64,
    spec: BodySpec,
    colliders: &[ColliderSpec],
    material: Material,
    radius: Option<f32>,
) -> ContactSource {
    assert!(world.insert_body(body(n), spec, colliders));
    let id = RegionId {
        entity: PhysicsId::new(n),
        part: 0,
    };
    let source = ContactSource {
        region: world.create_loading_region(id, body(n)).unwrap(),
        patch: 0,
    };
    for collider in colliders {
        world.bind_loading_collider(collider.id, source).unwrap();
    }
    host.surfaces.insert(
        source,
        ContactSurface {
            region: Region::new(material, 1.0).unwrap(),
            section: SECTION,
            radius,
        },
    );
    source
}

/// Steps until a step prices something, and returns what it priced.
fn first_charges(world: &mut PhysicsWorld, host: &mut Host) -> Vec<ContactCharge> {
    for _ in 0..240 {
        world.step(DT);
        let charges = world.charge_contact_impacts(host).unwrap();
        if !charges.is_empty() {
            return charges;
        }
    }
    panic!("nothing was priced");
}

fn charged(charge: &ContactCharge) -> (f32, Outcome) {
    match charge {
        ContactCharge::Charged {
            closing_speed,
            outcome,
            ..
        } => (*closing_speed, *outcome),
        other => panic!("{other:?}"),
    }
}

/// The law's outcome for `energy` across a patch of `width` between the
/// surfaces the host started with.
fn law(
    fresh: &BTreeMap<ContactSource, ContactSurface>,
    sources: [ContactSource; 2],
    width: f32,
    energy: f32,
) -> Outcome {
    let [a, b] = sources.map(|s| fresh[&s]);
    let load = Impact {
        closing_speed: 1.0,
        effective_mass: 2.0 * energy,
    };
    impact(
        &a.region,
        &b.region,
        ContactGeometry::new(width, a.section, b.section),
        load,
    )
    .unwrap()
}

fn close(actual: f32, expected: f32, tolerance: f32) -> bool {
    (actual - expected).abs() <= tolerance * expected.abs().max(1.0e-6)
}

/// Asserts that `outcome` is the law's for `expected` energy, and that the
/// host absorbed each side's consumed work.
fn assert_priced(outcome: &Outcome, expected: &Outcome, host: &Host, sources: [ContactSource; 2]) {
    let available = expected.ledger.available;
    assert!(
        close(outcome.ledger.available, available, 1.0e-4),
        "{} vs {available}",
        outcome.ledger.available
    );
    for side in 0..2 {
        assert!(
            close(outcome.consumed[side], expected.consumed[side], 1.0e-3),
            "{outcome:?} vs {expected:?}"
        );
    }
    let absorbed: Vec<_> = (0..2)
        .filter(|&side| outcome.consumed[side] > 0.0)
        .map(|side| (sources[side], outcome.consumed[side]))
        .collect();
    assert_eq!(host.absorbed, absorbed);
}

/// Ball 1, of hull, closing at 20 on ball 2, of rock, 0.001 away.
fn balls(host: &mut Host, radii: [Option<f32>; 2]) -> (PhysicsWorld, [ContactSource; 2]) {
    let mut world = world(Vec2::ZERO);
    let moving = BodySpec {
        position: -Vec2::X * 1.001,
        linear_velocity: Vec2::X * 20.0,
        can_sleep: false,
        ..Default::default()
    };
    let resting = BodySpec {
        can_sleep: false,
        ..Default::default()
    };
    let sources = [
        add(
            &mut world,
            host,
            1,
            moving,
            &[ball(1, 0.5, 1.0)],
            HULL,
            radii[0],
        ),
        add(
            &mut world,
            host,
            2,
            resting,
            &[ball(2, 0.5, 3.0)],
            ROCK,
            radii[1],
        ),
    ];
    (world, sources)
}

#[test]
fn a_head_on_impact_is_priced_on_the_energy_its_approach_brings() {
    let mut host = Host::default();
    let (mut world, sources) = balls(&mut host, [Some(0.5); 2]);
    let fresh = host.surfaces.clone();
    let charges = first_charges(&mut world, &mut host);
    assert_eq!(charges.len(), 1, "{charges:?}");
    let (speed, outcome) = charged(&charges[0]);
    assert_eq!(speed, 20.0);
    let [a, b] = [1, 2].map(|n| world.body_mass(body(n)).unwrap());
    let energy = 0.5 * a * b / (a + b) * 20.0 * 20.0;
    let width = patch_width_from_radii(0.5, Some(0.5)).unwrap();
    let expected = law(&fresh, sources, width, energy);
    assert!(expected.consumed[0] > 0.0, "{expected:?}");
    assert_priced(&outcome, &expected, &host, sources);
}

#[test]
fn a_spinning_bar_is_priced_on_the_mass_its_end_brings() {
    // A bar spinning clockwise just above a floor strikes it with its right
    // end, which falls at 5 while the left end rises.
    let mut world = world(Vec2::ZERO);
    let mut host = Host::default();
    let mut bar = cuboid(1, 0, 0.5, 0.05);
    bar.density = 10.0;
    let spinning = BodySpec {
        angular_velocity: -10.0,
        can_sleep: false,
        ..Default::default()
    };
    let floor = BodySpec {
        kind: BodyKind::Fixed,
        position: -Vec2::Y * (0.05 + 0.001 + 0.5),
        ..Default::default()
    };
    let sources = [
        add(&mut world, &mut host, 1, spinning, &[bar], HULL, Some(0.05)),
        add(
            &mut world,
            &mut host,
            2,
            floor,
            &[cuboid(2, 0, 2.0, 0.5)],
            ROCK,
            None,
        ),
    ];
    let fresh = host.surfaces.clone();
    let charges = first_charges(&mut world, &mut host);
    assert_eq!(charges.len(), 1, "{charges:?}");
    let (speed, outcome) = charged(&charges[0]);
    assert!(close(speed, 5.0, 1.0e-5), "{speed}");
    let mass = world.body_mass(body(1)).unwrap();
    let inertia = world.dynamic_body_inertia(body(1)).unwrap();
    // The end is half a length out, and the floor's normal points down.
    let lever = -0.5;
    let reduced = effective_mass(
        ContactBody::rotating(mass, inertia, lever),
        ContactBody::IMMOVABLE,
    )
    .unwrap();
    assert!(reduced < 0.3 * mass, "{reduced} of {mass}");
    let width = patch_width_from_radii(0.05, None).unwrap();
    let expected = law(&fresh, sources, width, 0.5 * reduced * 5.0 * 5.0);
    assert!(expected.consumed[0] > 0.0, "{expected:?}");
    assert_priced(&outcome, &expected, &host, sources);
}

#[test]
fn only_the_points_a_substep_closes_take_its_load() {
    // A box falls at 0.12, tilted so that its left corner is 0.0004 above the
    // floor and its right corner 0.0016. A substep closes 0.001, so only the
    // left corner meets the floor, and the box turns about it.
    let mut world = world(Vec2::ZERO);
    let mut host = Host::default();
    let mut block = cuboid(1, 0, 0.5, 0.25);
    block.density = 4.0;
    let tilt = 0.0012_f32;
    let falling = BodySpec {
        position: Vec2::Y * (0.0004 + 0.25 * tilt.cos() + 0.5 * tilt.sin()),
        angle: tilt,
        linear_velocity: -Vec2::Y * 0.12,
        can_sleep: false,
        ..Default::default()
    };
    let floor = BodySpec {
        kind: BodyKind::Fixed,
        position: -Vec2::Y * 0.5,
        ..Default::default()
    };
    add(
        &mut world,
        &mut host,
        1,
        falling,
        &[block],
        HULL,
        Some(0.25),
    );
    add(
        &mut world,
        &mut host,
        2,
        floor,
        &[cuboid(2, 0, 2.0, 0.5)],
        ROCK,
        None,
    );
    world.step(DT);
    let contacts = &world.loading_batches()[0].contacts;
    assert_eq!(contacts.len(), 2, "{contacts:?}");
    assert!(
        contacts
            .iter()
            .all(|c| close(c.closing_speed, 0.12, 1.0e-5))
    );
    let charges = world.charge_contact_impacts(&mut host).unwrap();
    assert_eq!(charges.len(), 1, "{charges:?}");
    let (_, outcome) = charged(&charges[0]);
    let mass = world.body_mass(body(1)).unwrap();
    let inertia = world.dynamic_body_inertia(body(1)).unwrap();
    let reduced = effective_mass(
        ContactBody::rotating(mass, inertia, 0.5),
        ContactBody::IMMOVABLE,
    )
    .unwrap();
    let energy = 0.5 * reduced * 0.12 * 0.12;
    assert!(
        close(outcome.ledger.available, energy, 1.0e-2),
        "{} vs {energy}, of {}",
        outcome.ledger.available,
        0.5 * mass * 0.12 * 0.12
    );
}

#[test]
fn a_wall_takes_the_whole_approach_and_a_moving_support_brings_its_own() {
    // A CCD ball at 400 meets a fixed wall 1 away, and a kinematic slab at 3
    // meets a resting one. Neither the wall nor the kinematic slab gives way,
    // so each impact is priced on the dynamic body's whole mass at the closing
    // speed.
    for kinematic in [false, true] {
        let mut world = world(Vec2::ZERO);
        let mut host = Host::default();
        let (mover, speed, width, sources) = if kinematic {
            let mut slab = cuboid(1, 0, 0.5, 1.0);
            slab.density = 10.0;
            let resting = BodySpec {
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            };
            let pushing = BodySpec {
                kind: BodyKind::KinematicVelocity,
                position: Vec2::X * 1.001,
                linear_velocity: -Vec2::X * 3.0,
                ..Default::default()
            };
            let slabs = [
                add(&mut world, &mut host, 1, resting, &[slab], HULL, Some(1.0)),
                add(
                    &mut world,
                    &mut host,
                    2,
                    pushing,
                    &[cuboid(2, 0, 0.5, 1.0)],
                    ROCK,
                    None,
                ),
            ];
            (1, 3.0, patch_width_from_radii(1.0, None).unwrap(), slabs)
        } else {
            let flying = BodySpec {
                position: -Vec2::X * 1.5,
                linear_velocity: Vec2::X * 400.0,
                ccd_enabled: true,
                can_sleep: false,
                ..Default::default()
            };
            let wall = BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::X * 50.0,
                ..Default::default()
            };
            let pair = [
                add(
                    &mut world,
                    &mut host,
                    1,
                    flying,
                    &[ball(1, 0.5, 1.0)],
                    HULL,
                    Some(0.5),
                ),
                add(
                    &mut world,
                    &mut host,
                    2,
                    wall,
                    &[cuboid(2, 0, 50.0, 50.0)],
                    ROCK,
                    None,
                ),
            ];
            (1, 400.0, patch_width_from_radii(0.5, None).unwrap(), pair)
        };
        let fresh = host.surfaces.clone();
        let charges = first_charges(&mut world, &mut host);
        assert_eq!(charges.len(), 1, "kinematic {kinematic}: {charges:?}");
        let (closing, outcome) = charged(&charges[0]);
        assert!(close(closing, speed, 1.0e-5), "{closing}");
        let mass = world.body_mass(body(mover)).unwrap();
        let expected = law(&fresh, sources, width, 0.5 * mass * speed * speed);
        assert_priced(&outcome, &expected, &host, sources);
    }
}

#[test]
fn charges_do_not_depend_on_collider_partitions_or_insertion_order() {
    let mut first = None;
    for parts in [1, 2, 4] {
        for reverse in [false, true] {
            let mut world = world(Vec2::ZERO);
            let mut host = Host::default();
            let slab = |n: u64| -> Vec<ColliderSpec> {
                let mut specs: Vec<_> = (0..parts)
                    .map(|part| {
                        let mut spec = cuboid(n, part, 0.5, 1.0 / f32::from(parts));
                        spec.local_position.y =
                            -1.0 + (f32::from(part) + 0.5) * 2.0 / f32::from(parts);
                        spec.density = 10.0;
                        spec
                    })
                    .collect();
                if reverse {
                    specs.reverse();
                }
                specs
            };
            let moving = BodySpec {
                linear_velocity: Vec2::X * 3.0,
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            };
            let fixed = BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::X * 1.001,
                ..Default::default()
            };
            for n in if reverse { [2, 1] } else { [1, 2] } {
                let (spec, material, radius) = if n == 1 {
                    (moving, HULL, Some(1.0))
                } else {
                    (fixed, ROCK, None)
                };
                add(&mut world, &mut host, n, spec, &slab(n), material, radius);
            }
            let charges = first_charges(&mut world, &mut host);
            assert_eq!(charges.len(), 1, "{parts} parts: {charges:?}");
            let (_, outcome) = charged(&charges[0]);
            let mass = world.body_mass(body(1)).unwrap();
            assert!(
                close(outcome.ledger.available, 0.5 * mass * 9.0, 1.0e-4),
                "{parts} parts, reversed {reverse}: {outcome:?}"
            );
            let first = *first.get_or_insert(outcome);
            assert!(close(outcome.consumed[0], first.consumed[0], 1.0e-4));
            assert!(close(outcome.consumed[1], first.consumed[1], 1.0e-4));
        }
    }
}

#[test]
fn a_body_landing_on_two_surfaces_at_once_spends_its_motion_once() {
    // A box falls flat onto two floors that meet left of its center. Priced
    // alone, the right floor's impact would stop the whole box, and the left
    // one's would add more.
    let mut world = world(Vec2::ZERO);
    let mut host = Host::default();
    let falling = BodySpec {
        position: Vec2::Y * (0.25 + 0.001),
        linear_velocity: -Vec2::Y * 4.0,
        can_sleep: false,
        ..Default::default()
    };
    let floor = |x: f32| BodySpec {
        kind: BodyKind::Fixed,
        position: Vec2::new(x, -0.5),
        ..Default::default()
    };
    let mut block = cuboid(1, 0, 0.5, 0.25);
    block.density = 4.0;
    add(
        &mut world,
        &mut host,
        1,
        falling,
        &[block],
        HULL,
        Some(0.25),
    );
    add(
        &mut world,
        &mut host,
        2,
        floor(-1.6),
        &[cuboid(2, 0, 1.4, 0.5)],
        ROCK,
        None,
    );
    add(
        &mut world,
        &mut host,
        3,
        floor(1.4),
        &[cuboid(3, 0, 1.6, 0.5)],
        ROCK,
        None,
    );
    let charges = first_charges(&mut world, &mut host);
    assert_eq!(charges.len(), 2, "{charges:?}");
    let energies = charges.iter().map(|c| charged(c).1.ledger.available);
    assert!(energies.clone().all(|e| e > 0.0), "{charges:?}");
    let mass = world.body_mass(body(1)).unwrap();
    let total: f32 = energies.sum();
    assert!(
        close(total, 0.5 * mass * 16.0, 1.0e-3),
        "{total} vs {}",
        8.0 * mass
    );
}

#[test]
fn a_step_that_ccd_splits_prices_each_substeps_impacts_on_their_own_motion() {
    // A CCD ball at 400 bounces between two fixed walls without losing speed
    // and strikes each within one step. Rapier splits the step into substeps,
    // so the strikes are in different substeps' batches, priced in turn. Each
    // has its own budget, priced on the motion its substep brings: half the
    // ball's mass times 400 squared.
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        collect_events: false,
        max_ccd_substeps: 4,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_speculative_arrivals().unwrap();
    let mut host = Host::default();
    let mut bouncing = ball(1, 0.5, 1.0);
    bouncing.restitution = 1.0;
    let flying = BodySpec {
        linear_velocity: Vec2::X * 400.0,
        ccd_enabled: true,
        can_sleep: false,
        ..Default::default()
    };
    let moving = add(
        &mut world,
        &mut host,
        1,
        flying,
        &[bouncing],
        HULL,
        Some(0.5),
    );
    let walls = [(2, 1.5), (3, -1.5)].map(|(n, x)| {
        let wall = BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::X * x,
            ..Default::default()
        };
        let mut face = cuboid(n, 0, 0.5, 5.0);
        face.restitution = 1.0;
        add(&mut world, &mut host, n, wall, &[face], ROCK, None)
    });
    let fresh = host.surfaces.clone();
    let mass = world.body_mass(body(1)).unwrap();
    let charges = first_charges(&mut world, &mut host);
    let struck: Vec<_> = world
        .loading_batches()
        .iter()
        .filter(|b| {
            b.events
                .iter()
                .any(|e| e.transition == EpisodeTransition::Impact)
        })
        .collect();
    assert_eq!(struck.len(), 2, "{:?}", world.loading_batches());
    assert_eq!(charges.len(), 2, "{charges:?}");
    let width = patch_width_from_radii(0.5, None).unwrap();
    for (charge, wall) in charges.iter().zip(walls) {
        let (closing, outcome) = charged(charge);
        assert!(close(closing, 400.0, 1.0e-3), "{closing}");
        let sources = if moving < wall {
            [moving, wall]
        } else {
            [wall, moving]
        };
        let ContactCharge::Charged {
            sources: priced, ..
        } = charge
        else {
            unreachable!()
        };
        assert_eq!(*priced, sources);
        let expected = law(&fresh, sources, width, 0.5 * mass * 400.0 * 400.0);
        assert!(
            close(outcome.ledger.available, expected.ledger.available, 1.0e-3),
            "{outcome:?} vs {expected:?}"
        );
    }
}

#[test]
fn a_budget_beyond_its_points_leaves_each_impact_it_couples_unpriced() {
    // A bar falls flat onto posts 0.5 apart, each its own source, and strikes
    // them in one substep. They share the bar, so they share one budget, and
    // each post brings two points. Sixteen posts give the 32 points a budget
    // takes. Seventeen give 34, and each of their impacts is unpriced.
    for (posts, priced) in [(16, true), (17, false)] {
        let mut world = world(Vec2::ZERO);
        let mut host = Host::default();
        let falling = BodySpec {
            position: Vec2::Y * (0.25 + 0.001),
            linear_velocity: -Vec2::Y * 4.0,
            can_sleep: false,
            ..Default::default()
        };
        let bar = cuboid(1, 0, 4.5, 0.25);
        add(&mut world, &mut host, 1, falling, &[bar], HULL, Some(0.25));
        for i in 0..posts {
            let n = 2 + i as u64;
            let post = BodySpec {
                kind: BodyKind::Fixed,
                position: Vec2::new(-4.0 + 0.5 * i as f32, -0.5),
                ..Default::default()
            };
            let top = cuboid(n, 0, 0.1, 0.5);
            add(&mut world, &mut host, n, post, &[top], ROCK, None);
        }
        let charges = first_charges(&mut world, &mut host);
        assert_eq!(charges.len(), posts, "{charges:?}");
        for charge in &charges {
            match charge {
                ContactCharge::Charged { .. } if priced => {}
                ContactCharge::Unpriced {
                    energy: None,
                    reason: Unpriced::Budget(BudgetError::Limit),
                    ..
                } if !priced => {}
                other => panic!("{posts} posts: {other:?}"),
            }
        }
    }
}

#[test]
fn impacts_on_one_surface_read_it_after_the_charges_before_them() {
    // Two balls strike one floor in the same substep, and either alone would
    // consume two thirds of what the floor holds. The second is priced
    // against the floor the first left, so the floor absorbs what it held and
    // no more.
    let mut world = world(Vec2::ZERO);
    let mut host = Host::default();
    let floor = BodySpec {
        kind: BodyKind::Fixed,
        position: -Vec2::Y * 0.5,
        ..Default::default()
    };
    let ground = add(
        &mut world,
        &mut host,
        1,
        floor,
        &[cuboid(1, 0, 10.0, 0.5)],
        SOFT_TISSUE,
        None,
    );
    let mut balls = Vec::new();
    for (n, x) in [(2, -2.0), (3, 2.0)] {
        let falling = BodySpec {
            position: Vec2::new(x, 0.5 + 0.001),
            linear_velocity: -Vec2::Y * 20.0,
            can_sleep: false,
            ..Default::default()
        };
        balls.push(add(
            &mut world,
            &mut host,
            n,
            falling,
            &[ball(n, 0.5, 1.0)],
            HULL,
            Some(0.5),
        ));
    }
    // What one strike alone consumes of the floor.
    let mass = world.body_mass(body(2)).unwrap();
    let width = patch_width_from_radii(0.5, None).unwrap();
    let alone = law(
        &host.surfaces,
        [ground, balls[0]],
        width,
        0.5 * mass * 400.0,
    );
    let consumed = alone.consumed[0];
    assert!(consumed > 0.0, "{alone:?}");
    let capacity = 1.5 * consumed;
    let surface = host.surfaces.get_mut(&ground).unwrap();
    let full = surface.region.capacity();
    surface.region = Region::new(SOFT_TISSUE, capacity / full).unwrap();
    let charges = first_charges(&mut world, &mut host);
    assert_eq!(charges.len(), 2, "{charges:?}");
    let absorbed: f32 = host
        .absorbed
        .iter()
        .filter(|(source, _)| *source == ground)
        .map(|(_, work)| work)
        .sum();
    assert!(
        close(absorbed, capacity, 1.0e-3),
        "{absorbed} vs {capacity}"
    );
}

#[test]
fn each_impact_is_priced_once_and_charged_only_where_the_host_allows() {
    let mut host = Host::default();
    let (mut world, sources) = balls(&mut host, [Some(0.5); 2]);
    world.step(DT);
    let charges = world.charge_contact_impacts(&mut host).unwrap();
    assert_eq!(charges.len(), 1);
    let absorbed = host.absorbed.clone();
    assert!(!absorbed.is_empty());
    // Again after the same step, after a pause and after a step that carries
    // the contact on, nothing more is priced.
    assert_eq!(world.charge_contact_impacts(&mut host), Ok(Vec::new()));
    world.step(0.0);
    assert_eq!(world.charge_contact_impacts(&mut host), Ok(Vec::new()));
    world.step(DT);
    assert_eq!(world.charge_contact_impacts(&mut host), Ok(Vec::new()));
    assert_eq!(host.absorbed, absorbed);

    // A shielded side is priced as before, and only the other is charged.
    for side in 0..2 {
        let mut shielding = Host::default();
        let (mut world, sources_again) = balls(&mut shielding, [Some(0.5); 2]);
        assert_eq!(sources_again, sources);
        shielding.protected.insert(sources[side]);
        world.step(DT);
        let shielded = world.charge_contact_impacts(&mut shielding).unwrap();
        let ContactCharge::Charged {
            outcome, protected, ..
        } = &shielded[0]
        else {
            panic!("{shielded:?}");
        };
        assert_eq!(*protected, [side == 0, side == 1]);
        assert_eq!(*outcome, charged(&charges[0]).1);
        let others: Vec<_> = absorbed
            .iter()
            .filter(|(source, _)| *source != sources[side])
            .copied()
            .collect();
        assert_eq!(shielding.absorbed, others);
    }
}

#[test]
fn impacts_the_host_cannot_price_are_reported_with_their_energy() {
    let unpriced = |charges: &[ContactCharge]| match charges {
        [ContactCharge::Unpriced { energy, reason, .. }] => (*energy, *reason),
        other => panic!("{other:?}"),
    };
    let energy = {
        let mut host = Host::default();
        let (mut world, _) = balls(&mut host, [Some(0.5); 2]);
        world.step(DT);
        charged(&world.charge_contact_impacts(&mut host).unwrap()[0])
            .1
            .ledger
            .available
    };
    for case in ["undeclared", "retired", "flat"] {
        let mut host = Host::default();
        let radii = if case == "flat" {
            [None; 2]
        } else {
            [Some(0.5); 2]
        };
        let (mut world, sources) = balls(&mut host, radii);
        world.step(DT);
        match case {
            "undeclared" => {
                host.surfaces.remove(&sources[1]);
            }
            "retired" => assert!(world.retire_loading_region(sources[1].region)),
            _ => {}
        }
        let (reported, reason) = unpriced(&world.charge_contact_impacts(&mut host).unwrap());
        assert!(
            close(reported.unwrap(), energy, 1.0e-6),
            "{case}: {reported:?}"
        );
        let expected = match case {
            "undeclared" => Unpriced::Undeclared,
            "retired" => Unpriced::Retired,
            _ => Unpriced::Material(MaterialError::InvalidGeometry),
        };
        assert_eq!(reason, expected, "{case}");
        assert!(host.absorbed.is_empty(), "{case}");
    }
}

#[test]
fn a_share_too_small_for_the_law_costs_nothing() {
    let surface = |material, radius| ContactSurface {
        region: Region::new(material, 1.0).unwrap(),
        section: SECTION,
        radius,
    };
    let tiny = Share {
        energy: 1.0e-12,
        speed: 1.0,
    };
    assert_eq!(
        price(surface(HULL, Some(0.5)), surface(ROCK, None), tiny),
        Ok(Outcome::default())
    );
}

#[test]
fn charges_need_a_capture_that_admits_speculative_arrivals() {
    let mut host = Host::default();
    let mut bare = PhysicsWorld::new(PhysicsWorldConfig::default());
    assert_eq!(
        bare.charge_contact_impacts(&mut host),
        Err(LoadingError::Disabled)
    );
    let mut touching = PhysicsWorld::new(PhysicsWorldConfig::default());
    touching
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    assert_eq!(
        touching.charge_contact_impacts(&mut host),
        Err(LoadingError::ArrivalsDisabled)
    );
}

#[test]
fn falls_are_priced_with_gravity_damping_and_joints_and_motion_is_unchanged() {
    // A damped CCD ball bounces on fixed ground under gravity, while a
    // pendulum swings on a rope elsewhere.
    let build = || {
        let mut world = world(-Vec2::Y * 10.0);
        let mut host = Host::default();
        let mut bouncy = ball(1, 0.5, 1.0);
        bouncy.restitution = 0.5;
        let dropped = BodySpec {
            position: Vec2::Y * 2.5,
            linear_damping: 0.1,
            ccd_enabled: true,
            can_sleep: false,
            ..Default::default()
        };
        let ground = BodySpec {
            kind: BodyKind::Fixed,
            position: -Vec2::Y * 0.5,
            ..Default::default()
        };
        add(
            &mut world,
            &mut host,
            1,
            dropped,
            &[bouncy],
            HULL,
            Some(0.5),
        );
        add(
            &mut world,
            &mut host,
            2,
            ground,
            &[cuboid(2, 0, 10.0, 0.5)],
            ROCK,
            None,
        );
        let pivot = BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(5.0, 5.0),
            ..Default::default()
        };
        let bob = BodySpec {
            position: Vec2::new(6.0, 5.0),
            can_sleep: false,
            ..Default::default()
        };
        assert!(world.insert_body(body(3), pivot, &[ball(3, 0.1, 1.0)]));
        assert!(world.insert_body(body(4), bob, &[ball(4, 0.1, 1.0)]));
        assert!(world.insert_tether(
            JointId::new(PhysicsId::new(4), JointRole::PRIMARY),
            TetherSpec {
                anchor_body: body(3),
                anchor: Vec2::ZERO,
                body: body(4),
                attachment: Vec2::ZERO,
                length: 1.0,
            }
        ));
        (world, host)
    };
    let (mut charging, mut host) = build();
    let (mut plain, _) = build();
    let mut impacts = Vec::new();
    for _ in 0..360 {
        charging.step(DT);
        plain.step(DT);
        impacts.extend(charging.charge_contact_impacts(&mut host).unwrap());
        assert_eq!(charging.motion(body(1)), plain.motion(body(1)));
        assert_eq!(charging.motion(body(4)), plain.motion(body(4)));
    }
    assert!(impacts.len() >= 2, "{impacts:?}");
    let mass = charging.body_mass(body(1)).unwrap();
    let mut last = f32::INFINITY;
    for charge in &impacts {
        let (speed, outcome) = charged(charge);
        assert!(speed < last, "{impacts:?}");
        last = speed;
        let energy = 0.5 * mass * speed * speed;
        assert!(
            close(outcome.ledger.available, energy, 1.0e-4),
            "{} vs {energy}",
            outcome.ledger.available
        );
    }
    // The first approach is the fall's, short of its last substep's gravity.
    let (first, _) = charged(&impacts[0]);
    assert!(first < (2.0_f32 * 10.0 * 2.0).sqrt(), "{first}");
}
