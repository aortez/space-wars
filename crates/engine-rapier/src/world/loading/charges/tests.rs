use super::*;
use engine_material::{
    ContactBody, HULL, Material, ROCK, SOFT_TISSUE, effective_mass, shared_impact,
};

mod invariance;
mod motion;
mod reference;
mod supports;

const DT: f32 = 1.0 / 120.0;
const GRAVITY: f32 = 10.0;
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
    world_with(gravity, 1)
}

/// A world whose capture admits speculative arrivals, in which CCD splits a
/// step into at most `substeps`.
fn world_with(gravity: Vec2, substeps: usize) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity,
        collect_events: false,
        max_ccd_substeps: substeps,
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
    let region = RegionId {
        entity: PhysicsId::new(n),
        part: 0,
    };
    let ids = colliders.iter().map(|collider| collider.id);
    bind(world, host, region, body(n), ids, material, radius)
}

/// Binds body `id`'s `colliders` to one source on `region`, with the host
/// keeping a unit volume of `material` behind it.
fn bind(
    world: &mut PhysicsWorld,
    host: &mut Host,
    region: RegionId,
    id: BodyId,
    colliders: impl IntoIterator<Item = ColliderId>,
    material: Material,
    radius: Option<f32>,
) -> ContactSource {
    let source = ContactSource {
        region: world.create_loading_region(region, id).unwrap(),
        patch: 0,
    };
    for collider in colliders {
        world.bind_loading_collider(collider, source).unwrap();
    }
    host.surfaces.insert(
        source,
        ContactSurface {
            region: Region::new(material, 1.0).unwrap(),
            thickness: SECTION.thickness,
            depth: Depth::Fixed(SECTION.depth),
            radius,
        },
    );
    source
}

/// Prices a step's impacts and then its supports.
fn charges(world: &mut PhysicsWorld, host: &mut Host) -> (Vec<ContactCharge>, Vec<SupportCharge>) {
    let impacts = world.charge_contact_impacts(host).unwrap();
    let supports = world.charge_contact_supports(host).unwrap();
    (impacts, supports)
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
    let section = |surface: ContactSurface| {
        let depth = match surface.depth {
            Depth::Fixed(depth) => depth,
            Depth::Patch(extent) => engine_material::deformation_depth(width, extent).unwrap(),
        };
        Section::new(surface.thickness, depth)
    };
    impact(
        &a.region,
        &b.region,
        ContactGeometry::new(width, section(a), section(b)),
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

/// A fixed floor of `material`, its top at zero, declared flat.
fn floor(
    world: &mut PhysicsWorld,
    host: &mut Host,
    material: Material,
    depth: Depth,
) -> ContactSource {
    let ground = BodySpec {
        kind: BodyKind::Fixed,
        position: -Vec2::Y * 0.5,
        ..Default::default()
    };
    let source = add(
        world,
        host,
        1,
        ground,
        &[cuboid(1, 0, 10.0, 0.5)],
        material,
        None,
    );
    host.surfaces.get_mut(&source).unwrap().depth = depth;
    source
}

/// Body `n`, a hull box 2 wide and 1 tall of `density`, resting flat on the
/// floor.
fn resting_box(
    world: &mut PhysicsWorld,
    host: &mut Host,
    n: u64,
    density: f32,
    can_sleep: bool,
) -> ContactSource {
    let mut shape = cuboid(n, 0, 1.0, 0.5);
    shape.density = density;
    let at = BodySpec {
        position: Vec2::Y * 0.5,
        can_sleep,
        ..Default::default()
    };
    add(world, host, n, at, &[shape], HULL, None)
}

/// A priced support's force, width, duration and outcomes.
fn supported(charge: &SupportCharge) -> (f32, f32, f32, [Outcome; 2]) {
    match charge {
        SupportCharge::Charged {
            force,
            width,
            duration,
            outcomes,
            ..
        } => (*force, *width, *duration, *outcomes),
        other => panic!("{other:?}"),
    }
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
fn a_charge_reports_the_geometry_and_load_it_was_priced_with() {
    let mut host = Host::default();
    let (mut world, sources) = balls(&mut host, [Some(0.5); 2]);
    let fresh = host.surfaces.clone();
    let charges = first_charges(&mut world, &mut host);
    let ContactCharge::Charged {
        geometry,
        load,
        outcome,
        ..
    } = &charges[0]
    else {
        panic!("{charges:?}");
    };
    // Head on, the balls bring their reduced mass at their closing speed.
    let [a, b] = [1, 2].map(|n| world.body_mass(body(n)).unwrap());
    assert!(
        close(load.effective_mass, a * b / (a + b), 1.0e-4),
        "{load:?}"
    );
    assert!(close(load.closing_speed, 20.0, 1.0e-6), "{load:?}");
    let width = patch_width_from_radii(0.5, Some(0.5)).unwrap();
    assert!(close(geometry.width, width, 1.0e-6), "{geometry:?}");
    // The law on the fresh regions with them gives the charge's outcome.
    let [region_a, region_b] = sources.map(|source| fresh[&source].region);
    assert_eq!(impact(&region_a, &region_b, *geometry, *load), Ok(*outcome));
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
            // The slabs' faces meet across their height, 2, which is wider
            // than the patch the radius gives.
            (1, 3.0, 2.0, slabs)
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

/// A hull 3 wide, its center over 0, falling flat at 6 and spinning at
/// `spin` from 0.0005 above a rock half-space. The half-space's pieces, on one
/// source, span the intervals of `floor`.
fn landing(
    host: &mut Host,
    floor: &[[f32; 2]],
    spin: f32,
    radius: Option<f32>,
) -> (PhysicsWorld, [ContactSource; 2]) {
    let mut world = world(Vec2::ZERO);
    let falling = BodySpec {
        position: Vec2::Y * 0.2505,
        linear_velocity: -Vec2::Y * 6.0,
        angular_velocity: spin,
        can_sleep: false,
        ..Default::default()
    };
    let fixed = BodySpec {
        kind: BodyKind::Fixed,
        ..Default::default()
    };
    let parts: Vec<_> = (0..)
        .zip(floor)
        .map(|(part, &[low, high])| {
            let mut spec = cuboid(2, part, (high - low) / 2.0, 0.5);
            spec.local_position = Vec2::new((low + high) / 2.0, -0.5);
            spec
        })
        .collect();
    let hull = cuboid(1, 0, 1.5, 0.25);
    let sources = [
        add(&mut world, host, 1, falling, &[hull], HULL, radius),
        add(&mut world, host, 2, fixed, &parts, ROCK, None),
    ];
    host.surfaces.get_mut(&sources[1]).unwrap().depth = Depth::Patch(None);
    (world, sources)
}

/// `pieces` equal pieces across a floor 8 wide.
fn even(pieces: u16) -> Vec<[f32; 2]> {
    let piece = 8.0 / f32::from(pieces);
    (0..pieces)
        .map(|part| {
            let low = -4.0 + f32::from(part) * piece;
            [low, low + piece]
        })
        .collect()
}

#[test]
fn a_face_landing_flat_is_priced_across_its_span() {
    // The hull's points span its face, so the patch is 3 wide whether the
    // hull declares a corner's radius or none, and the floor deforms as deep
    // as the patch is wide.
    for radius in [Some(0.2), None] {
        let mut host = Host::default();
        let (mut world, sources) = landing(&mut host, &even(1), 0.0, radius);
        let fresh = host.surfaces.clone();
        let charges = first_charges(&mut world, &mut host);
        assert_eq!(charges.len(), 1, "{charges:?}");
        let (closing, outcome) = charged(&charges[0]);
        assert!(close(closing, 6.0, 1.0e-4), "{closing}");
        let mass = world.body_mass(body(1)).unwrap();
        let expected = law(&fresh, sources, 3.0, 0.5 * mass * 36.0);
        assert_priced(&outcome, &expected, &host, sources);
        assert!(
            close(
                outcome.ledger.peak_force,
                expected.ledger.peak_force,
                1.0e-3
            ),
            "{outcome:?} vs {expected:?}"
        );
    }
}

#[test]
fn a_face_across_many_colliders_is_priced_as_across_one() {
    // The hull lands spinning, so its ends strike at different speeds. On a
    // floor of 4 pieces, its face meets 2 of them, and they meet at its
    // middle. On a floor of 80, it meets 30 of them at two points each, more
    // points than a budget holds. Only the patch's two ends enter the budget,
    // and the pieces cover the face between them, so each landing is priced
    // as on a floor of one piece.
    let mut outcomes = Vec::new();
    for pieces in [1, 4, 80] {
        let mut host = Host::default();
        let (mut world, _) = landing(&mut host, &even(pieces), 1.0, Some(0.2));
        let charges = first_charges(&mut world, &mut host);
        assert_eq!(charges.len(), 1, "{pieces} pieces: {charges:?}");
        if pieces == 80 {
            let points: usize = world
                .loading_batches()
                .iter()
                .map(|b| b.contacts.len())
                .sum();
            assert!(points > shared_impact::MAX_CONTACTS, "{points}");
        }
        outcomes.push(charged(&charges[0]).1);
    }
    let one = outcomes[0];
    for many in &outcomes[1..] {
        assert!(close(many.ledger.available, one.ledger.available, 1.0e-4));
        assert!(close(many.ledger.peak_force, one.ledger.peak_force, 1.0e-4));
        for side in 0..2 {
            assert!(
                close(many.consumed[side], one.consumed[side], 1.0e-4),
                "{many:?} vs {one:?}"
            );
        }
    }
}

#[test]
fn a_face_across_a_gap_is_priced_across_what_it_meets() {
    // The hull's face spans 3, but a gap 1 wide under its middle leaves it
    // meeting the floor across 2.
    let mut host = Host::default();
    let floor = [[-4.0, -0.5], [0.5, 4.0]];
    let (mut world, sources) = landing(&mut host, &floor, 0.0, Some(0.2));
    let fresh = host.surfaces.clone();
    let charges = first_charges(&mut world, &mut host);
    assert_eq!(charges.len(), 1, "{charges:?}");
    let (_, outcome) = charged(&charges[0]);
    let mass = world.body_mass(body(1)).unwrap();
    let expected = law(&fresh, sources, 2.0, 0.5 * mass * 36.0);
    assert_priced(&outcome, &expected, &host, sources);
    assert!(
        close(
            outcome.ledger.peak_force,
            expected.ledger.peak_force,
            1.0e-3
        ),
        "{outcome:?} vs {expected:?}"
    );
}

#[test]
fn ends_closer_than_a_thousandth_of_the_length_unit_are_one_point() {
    // A flat hull lands with 0.05 of its face over a flat ledge. At a length
    // unit of 1, the overlap is a patch 0.05 wide. At 100, its ends are one
    // point, which flat surfaces don't price.
    for (unit, priced) in [(1.0, true), (100.0, false)] {
        let mut host = Host::default();
        let (mut world, _) = landing(&mut host, &[[1.45, 4.0]], 0.0, None);
        assert!(world.set_length_unit(unit));
        let charges = first_charges(&mut world, &mut host);
        assert_eq!(charges.len(), 1, "{charges:?}");
        match &charges[0] {
            ContactCharge::Charged { .. } if priced => {}
            ContactCharge::Unpriced {
                reason: Unpriced::Material(MaterialError::InvalidGeometry),
                ..
            } if !priced => {}
            other => panic!("length unit {unit}: {other:?}"),
        }
    }
}

#[test]
fn a_patch_keeps_its_two_ends_and_covers_only_where_its_pieces_meet() {
    let point = |group, piece: u16, x: f32, normal: Vec2| PatchPoint {
        group,
        piece: ([collider(1, 0), collider(2, piece)], [None, None]),
        point: Vec2::new(x, 0.0),
        normal,
    };
    let tilted = |degrees: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Vec2::new(-sin, cos)
    };
    let points = [
        // Group 0 meets one face on three pieces: two that abut, and one past
        // a gap. Their points span 4 but cover 3. A fourth piece meets it at
        // one point, closer to its end than the coincident length, which
        // joins it.
        point(0, 0, 1.0, Vec2::Y),
        point(0, 0, 0.0, Vec2::Y),
        point(0, 1, 1.0, Vec2::Y),
        point(0, 1, 2.0, Vec2::Y),
        point(0, 2, 4.0, Vec2::Y),
        point(0, 2, 3.0, Vec2::Y),
        point(0, 4, 2.0005, Vec2::Y),
        // A point 2 degrees off the face's normal is on the face, and one 3
        // degrees off is a patch of its own.
        point(0, 3, 1.5, tilted(2.0)),
        point(0, 3, 6.0, tilted(3.0)),
        // Group 1 meets one piece at two points closer than the coincident
        // length, which are one point.
        point(1, 0, 5.0, Vec2::Y),
        point(1, 0, 5.0005, Vec2::Y),
        // Group 2 meets two pieces at a point each, 0.05 apart.
        point(2, 0, 7.0, Vec2::Y),
        point(2, 1, 7.05, Vec2::Y),
    ];
    let (kept, coverages) = patches(&points, 3, 1.0e-3);
    assert_eq!(kept, [4, 1, 8, 10, 12, 11]);
    let flat = ContactSurface {
        region: Region::new(ROCK, 1.0).unwrap(),
        thickness: 1.0,
        depth: Depth::Fixed(0.5),
        radius: None,
    };
    let curved = ContactSurface {
        radius: Some(0.4),
        ..flat
    };
    // Between flat surfaces, a point covers nothing.
    let narrow = width(&flat, &flat, &coverages[0]).unwrap();
    assert!(close(narrow, 3.0005, 1.0e-6), "{narrow}");
    for points in &coverages[1..] {
        assert_eq!(
            width(&flat, &flat, points),
            Err(MaterialError::InvalidGeometry)
        );
    }
    // With a radius, a point covers the patch the radii give, and so does
    // nothing at all. Points closer than that share what they cover.
    let patch = patch_width_from_radii(0.4, None).unwrap();
    let wide = width(&curved, &flat, &coverages[0]).unwrap();
    assert!(close(wide, 3.0005 + patch, 1.0e-6), "{wide}");
    assert_eq!(width(&curved, &flat, &coverages[1]), Ok(patch));
    let pair = width(&curved, &flat, &coverages[2]).unwrap();
    assert!(close(pair, patch + 0.05, 1.0e-5), "{pair}");
    assert_eq!(width(&curved, &flat, &Coverage::new()), Ok(patch));
}

#[test]
fn a_body_landing_on_two_surfaces_at_once_spends_its_motion_once() {
    // A box falls flat onto two floors that meet left of its center. Priced
    // alone, the right floor's impact would stop the whole box, and the left
    // one's would add more. Split in two halves, the box is priced the same.
    let mut priced = Vec::new();
    for parts in [1, 2] {
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
        let half = 0.5 / f32::from(parts);
        let blocks: Vec<_> = (0..parts)
            .map(|part| {
                let mut block = cuboid(1, part, half, 0.25);
                block.local_position.x = -0.5 + (2.0 * f32::from(part) + 1.0) * half;
                block.density = 4.0;
                block
            })
            .collect();
        add(&mut world, &mut host, 1, falling, &blocks, HULL, Some(0.25));
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
        let energies: Vec<_> = charges
            .iter()
            .map(|c| charged(c).1.ledger.available)
            .collect();
        assert!(energies.iter().all(|&e| e > 0.0), "{charges:?}");
        let mass = world.body_mass(body(1)).unwrap();
        let total: f32 = energies.iter().sum();
        assert!(
            close(total, 0.5 * mass * 16.0, 1.0e-3),
            "{total} vs {}",
            8.0 * mass
        );
        priced.push(energies);
    }
    for (whole, split) in priced[0].iter().zip(&priced[1]) {
        assert!(close(*split, *whole, 1.0e-3), "{priced:?}");
    }
}

#[test]
fn a_step_that_ccd_splits_prices_each_substeps_impacts_on_their_own_motion() {
    // A CCD ball at 400 bounces between two fixed walls without losing speed
    // and strikes each within one step. Rapier splits the step into substeps,
    // so the strikes are in different substeps' batches, priced in turn. Each
    // has its own budget, priced on the motion its substep brings: half the
    // ball's mass times 400 squared. The ball also moves at 200 along the
    // walls. A later strike priced about where the ball's center was a
    // substep earlier would gain a lever arm and be priced too low.
    let mut world = world_with(Vec2::ZERO, 4);
    let mut host = Host::default();
    let mut bouncing = ball(1, 0.5, 1.0);
    bouncing.restitution = 1.0;
    let flying = BodySpec {
        linear_velocity: Vec2::new(400.0, 200.0),
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
        thickness: SECTION.thickness,
        depth: Depth::Fixed(SECTION.depth),
        radius,
    };
    let tiny = Share {
        energy: 1.0e-12,
        speed: 1.0,
        ..Default::default()
    };
    let (_, load, outcome) = price(surface(HULL, Some(0.5)), surface(ROCK, None), &tiny).unwrap();
    assert_eq!(outcome, Outcome::default());
    assert!(load.effective_mass < MIN_EFFECTIVE_MASS, "{load:?}");
}

#[test]
fn a_patch_deep_surface_is_as_deep_as_the_patch_is_wide_up_to_its_extent() {
    let surface = |depth| ContactSurface {
        region: Region::new(ROCK, 1.0).unwrap(),
        thickness: 1.0,
        depth,
        radius: None,
    };
    let capped = surface(Depth::Patch(Some(0.1)));
    for (width, depth) in [(0.05, 0.05), (1.0, 0.1)] {
        assert_eq!(
            section(&capped, width),
            section(&surface(Depth::Fixed(depth)), width)
        );
    }
    assert_eq!(
        section(&surface(Depth::Patch(None)), 1.0),
        section(&surface(Depth::Fixed(1.0)), 1.0)
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
