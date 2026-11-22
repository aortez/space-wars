//! Weapon sources in a real physics world: shots swept through moss and
//! stopped by a plate, blasts, beams, and the once-only rules across pauses,
//! replays and duplicate reports.
use engine_core::Vec2;
use engine_core::plants::{Config, Environment, Exposure, Plants, Role, Support, Tissue, TissueId};
use engine_material::{
    ARMOR, ContactBody, ContactGeometry, Face, Impact, MaterialError, Outcome, Part, Pulses,
    Region, SHELL, SOFT_TISSUE, Section, SourceError, Target, TissueIntegrity, TissueMaterial,
    TissueTable, WOOD, blast_reach, deposit, effective_mass, impact, patch_width_from_radii,
};
use engine_rapier::world::loading::response::ResponseRejection;
use engine_rapier::world::loading::{CaptureConfig, ContactSource, RegionId, RegionToken};
use engine_rapier::world::weapon::{
    BeamSpec, Blast, Exclusion, Pulse, Reachable, Recipient, Recipients, ShotSpec, WeaponError,
    WeaponEvent, Weapons, projectile_impact,
};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsId, PhysicsWorld, PhysicsWorldConfig,
};
use std::collections::BTreeMap;

const DT: f32 = 1.0 / 240.0;
const SPEED: f32 = 300.0;
const SHOT_RADIUS: f32 = 0.25;
const SHOT_MASS: f32 = 0.5;
const FIRER: u64 = 1;
const SHOT: u64 = 2;
const PLATE: u64 = 10;
/// The key the host keeps a plate's region under, apart from the blades'.
const HULL: u64 = 1000;
const RAIL: u64 = 20;
const PANE: u64 = 21;
/// A ball and a rover whose regions the host keeps, under their own keys.
const BALL: u64 = 30;
const BALL_HULL: u64 = 1001;
const ROVER: u64 = 40;
const ROVER_HULL: u64 = 1002;
/// Blades of moss stand at x = 2.5, 3.5 and 4.5, and the plate's face at 7.5.
const BLADES: [f32; 3] = [2.5, 3.5, 4.5];
const BLADE_RADIUS: f32 = 0.1;
const BLADE: Section = Section::new(0.2, 0.1);
const PLATE_SECTION: Section = Section::new(1.0, 0.5);
const PAYLOAD: Blast = Blast {
    energy: 5000.0,
    radius: 6.0,
    height: 1.0,
};

struct Meadow;

impl Environment for Meadow {
    fn exposure(&self, _: &Tissue) -> Exposure {
        Exposure {
            dirt_contact: true,
            surface_contact: true,
            nutrients: 1000,
            water: 1000,
            light: 1000,
        }
    }

    fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
        true
    }

    fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
        true
    }
}

/// The host: three blades of moss, kept by the tissue adapter, the regions it
/// keeps for bodies, the circles it admits to fronts and the recipients its
/// policy shields.
struct Field {
    integrity: TissueIntegrity,
    blades: Vec<TissueId>,
    circles: Vec<Reachable>,
    bodies: BTreeMap<u64, (PhysicsId, Region)>,
    shielded: Vec<Recipient>,
    /// The section the host gives every blade.
    section: Section,
}

impl Field {
    fn new(blades: &[f32]) -> Self {
        let mut plants = Plants::new(Config::default(), 3).unwrap();
        let ids: Vec<TissueId> = (0..blades.len() as i32)
            .map(|x| {
                plants
                    .seed(1, Role::Moss, 1, [x, 0], None, [0; 3], &Meadow)
                    .unwrap()
            })
            .collect();
        let soft = TissueMaterial {
            material: SOFT_TISSUE,
            volume_per_size: 0.02,
        };
        let table = TissueTable {
            identity: 1,
            roles: [
                TissueMaterial {
                    material: WOOD,
                    ..soft
                },
                soft,
                soft,
                soft,
            ],
        };
        let circles = ids
            .iter()
            .zip(blades)
            .map(|(id, x)| Reachable {
                recipient: Recipient::Tissue(*id),
                center: Vec2::new(*x, 0.0),
                radius: BLADE_RADIUS,
                thickness: BLADE.thickness,
            })
            .collect();
        Field {
            integrity: TissueIntegrity::new(table, &plants).unwrap(),
            blades: ids,
            circles,
            bodies: BTreeMap::new(),
            shielded: Vec::new(),
            section: BLADE,
        }
    }

    /// The capacity left in the region the host keeps for a body.
    fn left(&self, key: u64) -> f64 {
        self.bodies[&key].1.remaining_precise()
    }

    /// Admits a plate's circles to fronts: a column of four along it.
    fn with_plate(self, recipient: Recipient, x: f32) -> Self {
        [-1.5, -0.5, 0.5, 1.5].into_iter().fold(self, |field, y| {
            field.with_circle(recipient, Vec2::new(x, y), 0.5)
        })
    }

    /// Admits one circle of a body, as thick as every front.
    fn with_circle(mut self, recipient: Recipient, center: Vec2, radius: f32) -> Self {
        self.circles.push(Reachable {
            recipient,
            center,
            radius,
            thickness: 1.0,
        });
        self
    }
}

impl Recipients for Field {
    fn near(&self, center: Vec2, radius: f32) -> Vec<Reachable> {
        self.circles
            .iter()
            .filter(|circle| {
                // A circle the host cannot place is admitted, for the weapons
                // to refuse.
                let gap = circle.center.distance_to(center) - circle.radius;
                gap <= radius || gap.is_nan()
            })
            .copied()
            .collect()
    }

    fn region(&self, key: u64) -> Option<(Region, Section)> {
        if let Some((_, region)) = self.bodies.get(&key) {
            return Some((*region, PLATE_SECTION));
        }
        Some((*self.integrity.region(key)?, self.section))
    }

    fn absorb(&mut self, key: u64, work: f32) {
        match self.bodies.get_mut(&key) {
            Some((_, region)) => {
                region.absorb(work).unwrap();
            }
            None => {
                self.integrity.charge(key, work).unwrap();
            }
        }
    }

    fn body(&self, body: BodyId) -> Option<u64> {
        self.bodies
            .iter()
            .find(|(_, (entity, _))| *entity == body.entity)
            .map(|(key, _)| *key)
    }

    fn protected(&self, recipient: Recipient) -> bool {
        self.shielded.contains(&recipient)
    }
}

fn body(entity: u64) -> BodyId {
    BodyId::new(PhysicsId::new(entity), BodyRole::PRIMARY)
}

fn collider(entity: u64, part: u16) -> ColliderId {
    ColliderId::new(PhysicsId::new(entity), ColliderRole::PRIMARY, part)
}

fn world() -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    world
}

/// A fixed plate at x = 8, of the given parts' centers and half heights. Its
/// region has `material` registered for it, if any.
fn plate(world: &mut PhysicsWorld, material: Option<Region>, parts: &[(f32, f32)]) -> Recipient {
    let colliders: Vec<ColliderSpec> = parts
        .iter()
        .enumerate()
        .map(|(part, (y, half))| ColliderSpec {
            local_position: Vec2::new(0.0, *y),
            ..ColliderSpec::cuboid(collider(PLATE, part as u16), 0.5, *half)
        })
        .collect();
    assert!(world.insert_body(
        body(PLATE),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(8.0, 0.0),
            ..Default::default()
        },
        &colliders,
    ));
    register(world, material, parts.len())
}

/// Binds the plate's `parts` colliders to its region, with `material`
/// registered for it, if any.
fn register(world: &mut PhysicsWorld, material: Option<Region>, parts: usize) -> Recipient {
    let token = world
        .create_loading_region(
            RegionId {
                entity: PhysicsId::new(PLATE),
                part: 0,
            },
            body(PLATE),
        )
        .unwrap();
    if let Some(region) = material {
        world
            .register_loading_material(token, region, PLATE_SECTION)
            .unwrap();
    }
    for part in 0..parts {
        world
            .bind_loading_collider(
                collider(PLATE, part as u16),
                ContactSource {
                    region: token,
                    patch: part as u32,
                },
            )
            .unwrap();
    }
    match material {
        Some(_) => Recipient::Region(token),
        None => Recipient::Unmodeled(body(PLATE)),
    }
}

fn armor() -> Region {
    Region::new(ARMOR, 4.0).unwrap()
}

/// The shape of `plate`'s one part.
fn slab() -> ColliderSpec {
    ColliderSpec::cuboid(collider(PLATE, 0), 0.5, 2.0)
}

/// A fixed body of `entity` at `position`, of `shape` and with no loading
/// region, whose armor the host keeps under `key`.
fn hosted(
    world: &mut PhysicsWorld,
    field: &mut Field,
    entity: u64,
    key: u64,
    position: Vec2,
    shape: ColliderSpec,
) -> Recipient {
    assert!(world.insert_body(
        body(entity),
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            ..Default::default()
        },
        &[shape],
    ));
    let entity = PhysicsId::new(entity);
    field.bodies.insert(key, (entity, armor()));
    Recipient::Body { key, entity }
}

/// A plate's body at `x`, kept as `hosted` keeps one, under `HULL`.
fn hull(world: &mut PhysicsWorld, field: &mut Field, x: f32, shape: ColliderSpec) -> Recipient {
    hosted(world, field, PLATE, HULL, Vec2::new(x, 0.0), shape)
}

fn token(recipient: Recipient) -> RegionToken {
    match recipient {
        Recipient::Region(token) => token,
        other => panic!("not a region: {other:?}"),
    }
}

/// A shot's body at `from`, flying at `velocity`.
fn shot_body(world: &mut PhysicsWorld, entity: u64, from: Vec2, velocity: Vec2) -> BodyId {
    let area = std::f32::consts::PI * SHOT_RADIUS * SHOT_RADIUS;
    assert!(world.insert_body(
        body(entity),
        BodySpec {
            position: from,
            linear_velocity: velocity,
            can_sleep: false,
            ccd_enabled: true,
            ..Default::default()
        },
        &[ColliderSpec {
            density: SHOT_MASS / area,
            ..ColliderSpec::ball(collider(entity, 0), SHOT_RADIUS)
        }],
    ));
    body(entity)
}

fn spec(entity: u64, blast: Option<Blast>) -> ShotSpec {
    ShotSpec {
        id: entity,
        owner: Some(FIRER),
        body: body(entity),
        region: Region::new(SHELL, 0.05).unwrap(),
        section: Section::new(0.5, 0.1),
        radius: SHOT_RADIUS,
        lifetime: 1.0,
        blast,
        protected: None,
    }
}

/// Fires a shot from the origin along +x.
fn fire(world: &mut PhysicsWorld, weapons: &mut Weapons, blast: Option<Blast>) {
    shot_body(world, SHOT, Vec2::ZERO, Vec2::new(SPEED, 0.0));
    weapons.launch(world, spec(SHOT, blast)).unwrap();
}

/// Steps until no shot flies, returning every event in order. Each step is
/// preceded by `pauses` paused steps, which must report nothing.
fn run(
    world: &mut PhysicsWorld,
    weapons: &mut Weapons,
    field: &mut Field,
    pauses: usize,
) -> Vec<WeaponEvent> {
    let mut events = Vec::new();
    for _ in 0..240 {
        for _ in 0..pauses {
            let prepared = weapons.prepare(world, 0.0);
            world.step(0.0);
            weapons.settle(prepared, world, 0.0, field);
            assert!(weapons.take_events().is_empty());
        }
        let prepared = weapons.prepare(world, DT);
        world.step(DT);
        weapons.settle(prepared, world, DT, field);
        events.extend(weapons.take_events());
        if weapons.shots().next().is_none() {
            return events;
        }
    }
    panic!("the shot never retired");
}

fn delivered(events: &[WeaponEvent], part: Part, recipient: Recipient) -> Vec<Outcome> {
    events
        .iter()
        .filter_map(|event| match event {
            WeaponEvent::Delivered {
                part: p,
                recipient: r,
                outcome,
                ..
            } if *p == part && *r == recipient => Some(*outcome),
            _ => None,
        })
        .collect()
}

fn refusals(events: &[WeaponEvent]) -> Vec<(Part, Recipient, SourceError)> {
    events
        .iter()
        .filter_map(|event| match event {
            WeaponEvent::Refused {
                part,
                recipient,
                error,
                ..
            } => Some((*part, *recipient, *error)),
            _ => None,
        })
        .collect()
}

fn retired(events: &[WeaponEvent]) -> Vec<&WeaponEvent> {
    events
        .iter()
        .filter(|event| matches!(event, WeaponEvent::Retired { .. }))
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1.0e-6 * b.abs().max(1.0)
}

#[test]
fn a_shot_through_moss_charges_each_blade_once_and_stops_on_the_plate() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    // The last blade stands just in front of the plate, so the shot passes it
    // in the step it stops. It has a second, smaller circle.
    let mut field = Field::new(&[2.5, 3.5, 4.5, 7.0]);
    let last = field.circles[3];
    field.circles.push(Reachable {
        center: Vec2::new(7.0, 0.12),
        radius: 0.05,
        ..last
    });
    let blade = *field.integrity.region(field.blades[0]).unwrap();
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, None);
    let mass = world.body_mass(body(SHOT)).unwrap();
    let events = run(&mut world, &mut weapons, &mut field, 0);

    // Each blade is charged once with its own collision's energy, at the
    // shot's speed before any step's contact, on a face as wide as the blade
    // is, and cut.
    let load = Impact {
        closing_speed: SPEED,
        effective_mass: effective_mass(
            ContactBody::translating(mass),
            ContactBody::translating(blade.mass()),
        )
        .unwrap(),
    };
    let alone = deposit(&blade, Face::new(2.0 * BLADE_RADIUS, BLADE), load.energy()).unwrap();
    let mut kept = 0.0;
    for blade in &field.blades {
        let passes = delivered(&events, Part::Kinetic, Recipient::Tissue(*blade));
        assert_eq!(passes.len(), 1, "{events:#?}");
        assert!(passes[0].failed[0]);
        assert_eq!(passes[0].ledger.available, load.energy());
        assert_eq!(passes[0].ledger.peak_force, alone.ledger.peak_force);
        kept += f64::from(passes[0].ledger.irreversible[0] + passes[0].ledger.dissipated[0]);
    }
    let mut failures = field.integrity.take_failures();
    failures.sort();
    assert_eq!(failures, field.blades);
    // A blade a step ends on is reported again by the next step's sweep and
    // refused.
    for (part, recipient, error) in refusals(&events) {
        assert_eq!(part, Part::Kinetic);
        assert!(matches!(recipient, Recipient::Tissue(_)));
        assert!(matches!(error, SourceError::Duplicate(_)));
    }

    // The plate is hit once, with all the shot's launch energy, since the
    // moss neither slows nor deflects it.
    let hits = delivered(&events, Part::Kinetic, plate);
    assert_eq!(hits.len(), 1);
    let launch = 0.5 * f64::from(mass) * f64::from(SPEED).powi(2);
    assert!(close(f64::from(hits[0].ledger.available), launch));
    // It is the impact law's outcome for the shot's curved face against the
    // plate, with the shot's mass against the fixed plate.
    let contact = ContactGeometry::new(
        patch_width_from_radii(SHOT_RADIUS, None).unwrap(),
        spec(SHOT, None).section,
        PLATE_SECTION,
    );
    let load = Impact {
        closing_speed: SPEED,
        effective_mass: mass,
    };
    assert_eq!(
        hits[0],
        impact(&spec(SHOT, None).region, &armor(), contact, load).unwrap()
    );
    assert!(events.iter().any(|event| matches!(
        event,
        WeaponEvent::Stopped { body, point, .. } if *body == body_of(plate) && (point.x - 7.5).abs() < 0.01
    )));
    // Only the source charged the plate: the contact response refused the
    // shot's arrival and charged nothing.
    let remaining = world.loading_material_remaining(token(plate)).unwrap();
    assert_eq!(
        remaining,
        armor().remaining_precise() - f64::from(hits[0].consumed[1])
    );
    let response = world.material_response_diagnostics();
    assert_eq!(response.applied_batches, 0);
    assert_eq!(
        response.last_rejection,
        Some(ResponseRejection::UnboundContact)
    );

    // The source retires once, its ledger holding the passes and the hit, and
    // its body leaves the world.
    let retired = retired(&events);
    assert_eq!(retired.len(), 1);
    let WeaponEvent::Retired { owner, ledger, .. } = retired[0] else {
        unreachable!()
    };
    assert_eq!(*owner, Some(FIRER));
    assert!(close(
        ledger.kinetic.delivered,
        kept + f64::from(hits[0].ledger.available)
    ));
    assert!(world.motion(body(SHOT)).is_none());
    assert!(weapons.source(SHOT).is_none());

    // The hit is exactly the one the shot makes without the moss.
    let mut bare = world_with_plate();
    let mut empty = Field::new(&[]);
    let mut weapons = Weapons::new();
    fire(&mut bare.0, &mut weapons, None);
    let events = run(&mut bare.0, &mut weapons, &mut empty, 0);
    assert_eq!(delivered(&events, Part::Kinetic, bare.1), hits);
}

fn world_with_plate() -> (PhysicsWorld, Recipient) {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    (world, plate)
}

fn body_of(recipient: Recipient) -> BodyId {
    match recipient {
        Recipient::Region(token) => BodyId::new(token.id.entity, BodyRole::PRIMARY),
        Recipient::Body { entity, .. } => BodyId::new(entity, BodyRole::PRIMARY),
        Recipient::Unmodeled(body) => body,
        Recipient::Tissue(_) => panic!("tissue has no body"),
    }
}

#[test]
fn a_shell_releases_its_blast_once_where_it_stops() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&BLADES).with_plate(plate, 8.0);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, Some(PAYLOAD));
    let events = run(&mut world, &mut weapons, &mut field, 0);

    // One release: the plate takes the directions it fills, and each blade
    // a share of what the blades nearer the blast leave.
    let blast = delivered(&events, Part::Payload, plate);
    assert_eq!(blast.len(), 1);
    let mut total = f64::from(blast[0].ledger.available);
    for blade in &field.blades {
        let shares = delivered(&events, Part::Payload, Recipient::Tissue(*blade));
        assert_eq!(shares.len(), 1);
        // The passes cut the blades already, so they take no more work.
        assert_eq!(shares[0].consumed[0], 0.0);
        total += f64::from(shares[0].ledger.available);
    }
    assert!(total <= f64::from(PAYLOAD.energy) * (1.0 + 1.0e-6));
    // The plate fills nearly half the directions around a blast at its face.
    assert!((0.4..0.5).contains(&(f64::from(blast[0].ledger.available) / 5000.0)));

    let WeaponEvent::Retired { ledger, .. } = retired(&events)[0] else {
        unreachable!()
    };
    assert!(close(ledger.payload.delivered, total));
    assert_eq!(ledger.payload.budget, 5000.0);
    // The plate carries both charges, each attributed to its part.
    let hit = delivered(&events, Part::Kinetic, plate);
    let remaining = world.loading_material_remaining(token(plate)).unwrap();
    let charged = f64::from(hit[0].consumed[1]) + f64::from(blast[0].consumed[0]);
    assert!(close(armor().remaining_precise() - remaining, charged));
    // The shell is gone; nothing can release it again.
    assert_eq!(
        weapons.detonate(&mut world, SHOT, &mut field),
        Err(WeaponError::UnknownSource)
    );
}

#[test]
fn hard_bodies_shadow_what_lies_behind_them() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    // A shell held at x = 6 bursts beside the plate. One blade lies behind
    // the plate and one in the open.
    // A host may list every body, the shell's own among them.
    let mut field = Field::new(&[10.0, 3.0]).with_plate(plate, 8.0).with_circle(
        Recipient::Unmodeled(body(SHOT)),
        Vec2::new(6.0, 0.0),
        SHOT_RADIUS,
    );
    let mut weapons = Weapons::new();
    shot_body(&mut world, SHOT, Vec2::new(6.0, 0.0), Vec2::ZERO);
    weapons.launch(&world, spec(SHOT, Some(PAYLOAD))).unwrap();
    weapons.detonate(&mut world, SHOT, &mut field).unwrap();
    let events = weapons.take_events();
    let [behind, open] = [field.blades[0], field.blades[1]];
    assert!(delivered(&events, Part::Payload, Recipient::Tissue(behind)).is_empty());
    assert_eq!(
        delivered(&events, Part::Payload, Recipient::Tissue(open)).len(),
        1
    );
    assert_eq!(delivered(&events, Part::Payload, plate).len(), 1);
    assert!(world.motion(body(SHOT)).is_none());
}

#[test]
fn a_beam_charges_the_moss_it_crosses_and_ends_on_the_plate() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&BLADES);
    let mut weapons = Weapons::new();
    let pulses = Pulses {
        energy: 100.0,
        count: 3,
    };
    weapons
        .arm_beam(BeamSpec {
            id: 7,
            owner: Some(FIRER),
            pulses,
            width: 0.5,
            height: 1.0,
            collision_groups: CollisionGroups::ALL,
            protected: None,
        })
        .unwrap();
    // The physics' query index is the last step's.
    world.step(DT);
    for _ in 0..3 {
        weapons
            .fire(&mut world, 7, east(100.0), &mut field)
            .unwrap();
    }
    let events = weapons.take_events();
    // Each blade takes a fifth of what reaches the part of the width it
    // covers, and the plate takes the rest.
    for (blade, coupling) in field.blades.iter().zip([0.08, 0.064, 0.0512]) {
        let pulses = delivered(&events, Part::Beam, Recipient::Tissue(*blade));
        assert_eq!(pulses.len(), 3);
        assert!(close(
            f64::from(pulses[0].ledger.available),
            100.0 * coupling
        ));
    }
    let blade = field.blades[0];
    let consumed: f64 = delivered(&events, Part::Beam, Recipient::Tissue(blade))
        .iter()
        .map(|outcome| f64::from(outcome.consumed[0]))
        .sum();
    assert!(consumed > 0.0);
    assert_eq!(
        field.integrity.region(blade).unwrap().remaining_precise(),
        Field::new(&BLADES)
            .integrity
            .region(blade)
            .unwrap()
            .remaining_precise()
            - consumed
    );
    let ends = delivered(&events, Part::Beam, plate);
    assert_eq!(ends.len(), 3);
    assert!(close(f64::from(ends[0].ledger.available), 80.48));
    assert!(events.iter().any(|event| matches!(
        event,
        WeaponEvent::Traced { end, body: Some(body), .. }
            if (end.x - 7.5).abs() < 1.0e-4 && *body == body_of(plate)
    )));
    let WeaponEvent::Retired { ledger, .. } = retired(&events)[0] else {
        unreachable!()
    };
    assert!(ledger.beam.delivered <= 300.0 * (1.0 + 1.0e-6));
    assert_eq!(
        weapons.fire(&mut world, 7, east(100.0), &mut field),
        Err(WeaponError::UnknownSource)
    );
}

#[test]
fn a_shot_hits_once_however_many_colliders_report_it() {
    let mut world = world();
    // Two parts of one plate meet where the shot strikes.
    let plate = plate(&mut world, Some(armor()), &[(1.0, 1.0), (-1.0, 1.0)]);
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    shot_body(&mut world, SHOT, Vec2::ZERO, Vec2::new(SPEED, 0.0));
    weapons.launch(&world, spec(SHOT, None)).unwrap();
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert_eq!(delivered(&events, Part::Kinetic, plate).len(), 1);
    let stops = events
        .iter()
        .filter(|event| matches!(event, WeaponEvent::Stopped { .. }))
        .count();
    assert_eq!(stops, 1);
    assert_eq!(
        refusals(&events),
        [(Part::Kinetic, plate, SourceError::Spent(Part::Kinetic))]
    );

    // A body appearing where the shot stopped, such as an occupant thrown
    // from what it hit, meets nothing: the shot is gone.
    shot_body(&mut world, 30, Vec2::new(7.2, 0.1), Vec2::new(-5.0, 0.0));
    for _ in 0..4 {
        let prepared = weapons.prepare(&world, DT);
        world.step(DT);
        weapons.settle(prepared, &mut world, DT, &mut field);
    }
    assert!(weapons.take_events().is_empty());
}

#[test]
fn the_shot_keeps_its_attribution_after_its_firer_dies() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&BLADES).with_plate(plate, 8.0);
    let mut weapons = Weapons::new();
    // The shot protects its firer, which dies after the first step.
    shot_body(&mut world, FIRER, Vec2::new(-3.0, 0.0), Vec2::ZERO);
    shot_body(&mut world, SHOT, Vec2::ZERO, Vec2::new(SPEED, 0.0));
    let spec = ShotSpec {
        protected: Some(PhysicsId::new(FIRER)),
        ..spec(SHOT, Some(PAYLOAD))
    };
    weapons.launch(&world, spec).unwrap();
    let prepared = weapons.prepare(&world, DT);
    world.step(DT);
    weapons.settle(prepared, &mut world, DT, &mut field);
    assert!(world.remove_entity(PhysicsId::new(FIRER)));
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert_eq!(delivered(&events, Part::Kinetic, plate).len(), 1);
    assert_eq!(delivered(&events, Part::Payload, plate).len(), 1);
    assert!(!events.iter().any(|event| matches!(
        event,
        WeaponEvent::Excluded {
            reason: Exclusion::Protected,
            ..
        }
    )));
    let WeaponEvent::Retired { owner, .. } = retired(&events)[0] else {
        unreachable!()
    };
    assert_eq!(*owner, Some(FIRER));
}

#[test]
fn paused_steps_change_nothing_and_a_replay_matches() {
    let play = |pauses| {
        let mut world = world();
        let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
        let mut field = Field::new(&BLADES).with_plate(plate, 8.0);
        let mut weapons = Weapons::new();
        fire(&mut world, &mut weapons, Some(PAYLOAD));
        let events = run(&mut world, &mut weapons, &mut field, pauses);
        let remaining = world.loading_material_remaining(token(plate));
        let blades: Vec<_> = field
            .blades
            .iter()
            .map(|id| field.integrity.region(*id).unwrap().remaining_precise())
            .collect();
        (events, remaining, blades)
    };
    let first = play(0);
    assert!(!first.0.is_empty());
    assert_eq!(play(0), first);
    assert_eq!(play(2), first);
}

/// A host that keeps its world in a cloneable state, such as one that copies
/// a game to look ahead, clones its weapons with the world. The clone plays on
/// as the original does.
#[test]
fn a_clone_of_the_world_and_its_weapons_plays_on_as_the_original() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&BLADES).with_plate(plate, 8.0);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, Some(PAYLOAD));
    // One step short of the moss leaves nothing charged, so a fresh field is
    // the same as the one the original keeps.
    let prepared = weapons.prepare(&world, DT);
    world.step(DT);
    weapons.settle(prepared, &mut world, DT, &mut field);
    assert!(weapons.take_events().is_empty());
    assert!(weapons.shots().next().is_some());
    let (mut twin, mut twin_weapons) = (world.clone(), weapons.clone());
    let mut twin_field = Field::new(&BLADES).with_plate(plate, 8.0);
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert!(!events.is_empty());
    assert_eq!(
        run(&mut twin, &mut twin_weapons, &mut twin_field, 0),
        events
    );
    assert_eq!(
        twin.loading_material_remaining(token(plate)),
        world.loading_material_remaining(token(plate))
    );
}

#[test]
fn a_shot_flies_on_when_its_host_replaces_its_body() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&BLADES).with_plate(plate, 8.0);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, Some(PAYLOAD));
    let prepared = weapons.prepare(&world, DT);
    world.step(DT);
    weapons.settle(prepared, &mut world, DT, &mut field);
    assert!(weapons.take_events().is_empty());
    let (mut twin, mut twin_weapons) = (world.clone(), weapons.clone());
    let mut twin_field = Field::new(&BLADES).with_plate(plate, 8.0);
    // The twin's host rebuilds the shot's body where it was, under its id.
    let motion = twin.motion(body(SHOT)).unwrap();
    assert!(twin.remove_entity(PhysicsId::new(SHOT)));
    shot_body(&mut twin, SHOT, motion.position, motion.linear_velocity);
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert!(!delivered(&events, Part::Kinetic, plate).is_empty());
    assert_eq!(
        run(&mut twin, &mut twin_weapons, &mut twin_field, 0),
        events
    );
}

#[test]
fn an_unspent_shot_retires_once_when_its_lifetime_ends() {
    let mut world = world();
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    shot_body(&mut world, SHOT, Vec2::ZERO, Vec2::new(SPEED, 0.0));
    weapons
        .launch(
            &world,
            ShotSpec {
                lifetime: 0.05,
                ..spec(SHOT, Some(PAYLOAD))
            },
        )
        .unwrap();
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert_eq!(events.len(), 1);
    let WeaponEvent::Retired { ledger, .. } = &events[0] else {
        panic!("{events:?}");
    };
    // A shot that outlives its fuse releases nothing.
    assert_eq!(ledger.kinetic.delivered, 0.0);
    assert_eq!(ledger.payload.delivered, 0.0);
    assert!(world.motion(body(SHOT)).is_none());
    assert!(weapons.take_events().is_empty());
}

#[test]
fn a_worn_plate_takes_what_it_has_left_and_a_failed_one_nothing() {
    let mut world = world();
    let worn = Region::with_remaining(ARMOR, 4.0, 10.0).unwrap();
    let plate = plate(&mut world, Some(worn), &[(0.0, 2.0)]);
    let mut field = Field::new(&[]);
    for shot in [SHOT, SHOT + 1] {
        let mut weapons = Weapons::new();
        shot_body(&mut world, shot, Vec2::ZERO, Vec2::new(SPEED, 0.0));
        weapons.launch(&world, spec(shot, None)).unwrap();
        let events = run(&mut world, &mut weapons, &mut field, 0);
        let hit = &delivered(&events, Part::Kinetic, plate)[0];
        let taken = if shot == SHOT { 10.0 } else { 0.0 };
        assert_eq!(hit.consumed[1], taken);
        assert!(hit.ledger.excess > 0.0);
        assert_eq!(world.loading_material_remaining(token(plate)), Some(0.0));
    }
}

#[test]
fn unmodeled_bodies_stop_a_shot_without_suppressing_its_blast() {
    let mut world = world();
    // Terrain and other bodies without a registered material.
    let ground = plate(&mut world, None, &[(0.0, 2.0)]);
    let mut field = Field::new(&BLADES).with_plate(ground, 8.0);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, Some(PAYLOAD));
    let mass = world.body_mass(body(SHOT)).unwrap();
    let events = run(&mut world, &mut weapons, &mut field, 0);
    let excluded: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            WeaponEvent::Excluded {
                part,
                recipient,
                reason,
                energy,
                ..
            } => Some((*part, *recipient, *reason, *energy)),
            _ => None,
        })
        .collect();
    assert_eq!(excluded.len(), 2);
    let launch = 0.5 * f64::from(mass) * f64::from(SPEED).powi(2);
    assert_eq!(
        (excluded[0].0, excluded[0].1, excluded[0].2),
        (Part::Kinetic, ground, Exclusion::Unmodeled)
    );
    assert!(close(f64::from(excluded[0].3), launch));
    assert_eq!(
        (excluded[1].0, excluded[1].1, excluded[1].2),
        (Part::Payload, ground, Exclusion::Unmodeled)
    );
    // The blades in front of the ground still take their blast shares.
    for blade in &field.blades {
        assert_eq!(
            delivered(&events, Part::Payload, Recipient::Tissue(*blade)).len(),
            1
        );
    }
}

#[test]
fn a_protected_firer_is_passed_and_reported_not_charged() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    // The firer stands between the muzzle and the plate, with armor of its own.
    let firer = shot_body(&mut world, FIRER, Vec2::new(1.0, 0.0), Vec2::ZERO);
    let token = world
        .create_loading_region(
            RegionId {
                entity: firer.entity,
                part: 0,
            },
            firer,
        )
        .unwrap();
    world
        .register_loading_material(token, armor(), PLATE_SECTION)
        .unwrap();
    world
        .bind_loading_collider(
            collider(FIRER, 0),
            ContactSource {
                region: token,
                patch: 0,
            },
        )
        .unwrap();
    let mut field =
        Field::new(&[]).with_circle(Recipient::Region(token), Vec2::new(1.0, 0.0), SHOT_RADIUS);
    let mut weapons = Weapons::new();
    weapons
        .arm_beam(BeamSpec {
            id: 7,
            owner: Some(FIRER),
            pulses: Pulses {
                energy: 100.0,
                count: 1,
            },
            width: 0.5,
            height: 1.0,
            collision_groups: CollisionGroups::ALL,
            protected: Some(firer.entity),
        })
        .unwrap();
    world.step(DT);
    weapons
        .fire(&mut world, 7, east(100.0), &mut field)
        .unwrap();
    // A blast beside it reports the firer's share without charging it.
    shot_body(&mut world, SHOT, Vec2::new(3.0, 0.0), Vec2::ZERO);
    weapons
        .launch(
            &world,
            ShotSpec {
                protected: Some(firer.entity),
                ..spec(SHOT, Some(PAYLOAD))
            },
        )
        .unwrap();
    weapons.detonate(&mut world, SHOT, &mut field).unwrap();
    let events = weapons.take_events();
    assert_eq!(delivered(&events, Part::Beam, plate).len(), 1);
    assert!(delivered(&events, Part::Payload, Recipient::Region(token)).is_empty());
    assert!(events.iter().any(|event| matches!(
        event,
        WeaponEvent::Excluded { part: Part::Payload, recipient, reason: Exclusion::Protected, energy, .. }
            if *recipient == Recipient::Region(token) && *energy > 0.0
    )));
    assert_eq!(
        world.loading_material_remaining(token),
        Some(armor().remaining_precise())
    );
}

/// A dynamic plate at x = 8, as heavy as `mass`, moving at `velocity`.
fn loose_plate(
    world: &mut PhysicsWorld,
    mass: f32,
    velocity: Vec2,
    lock_rotation: bool,
) -> Recipient {
    assert!(world.insert_body(
        body(PLATE),
        BodySpec {
            position: Vec2::new(8.0, 0.0),
            linear_velocity: velocity,
            lock_rotation,
            can_sleep: false,
            ..Default::default()
        },
        &[ColliderSpec {
            density: mass / 4.0,
            ..ColliderSpec::cuboid(collider(PLATE, 0), 0.5, 2.0)
        }],
    ));
    register(world, Some(armor()), 1)
}

#[test]
fn a_hit_is_measured_from_both_bodies_motion_and_inertia() {
    // A plate closing on the shot adds its speed, and its mass shares the
    // impact's.
    let mut world = world();
    let plate = loose_plate(&mut world, 20.0, Vec2::new(-10.0, 0.0), true);
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, None);
    let [m, big] = [SHOT, PLATE].map(|entity| f64::from(world.body_mass(body(entity)).unwrap()));
    let events = run(&mut world, &mut weapons, &mut field, 0);
    let hit = &delivered(&events, Part::Kinetic, plate)[0];
    let m_eff = m * big / (m + big);
    let expected = 0.5 * m_eff * (f64::from(SPEED) + 10.0).powi(2);
    assert!(close(f64::from(hit.ledger.available), expected));

    // A plate free to turn, struck off its center, gives way more easily.
    let mut world = crate::world();
    let plate = loose_plate(&mut world, 20.0, Vec2::ZERO, false);
    let inertia = f64::from(world.dynamic_body_inertia(body(PLATE)).unwrap());
    let mut weapons = Weapons::new();
    shot_body(&mut world, SHOT, Vec2::new(0.0, 1.0), Vec2::new(SPEED, 0.0));
    weapons.launch(&world, spec(SHOT, None)).unwrap();
    let events = run(&mut world, &mut weapons, &mut field, 0);
    let hit = &delivered(&events, Part::Kinetic, plate)[0];
    // The lever about the plate's center is 1 wu.
    let m_eff = 1.0 / (1.0 / m + 1.0 / big + 1.0 / inertia);
    let expected = 0.5 * m_eff * f64::from(SPEED).powi(2);
    assert!((f64::from(hit.ledger.available) / expected - 1.0).abs() < 1.0e-3);
}

#[test]
fn a_projectile_that_is_not_a_body_is_measured_as_a_shot_is() {
    let velocity = Vec2::new(SPEED, 0.0);
    let normal = Vec2::new(1.0, 0.0);
    let face = Vec2::new(7.5, 0.0);
    let measure = |world: &PhysicsWorld, mass: f32, velocity: Vec2, entity: u64, point: Vec2| {
        projectile_impact(
            world,
            mass,
            velocity,
            collider(entity, 0),
            None,
            normal,
            point,
        )
    };

    // A fixed plate doesn't give way, so the projectile's own mass meets it.
    let mut world = world();
    plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    assert_eq!(
        measure(&world, SHOT_MASS, velocity, PLATE, face),
        Some(Impact {
            closing_speed: SPEED,
            effective_mass: SHOT_MASS,
        })
    );
    // Moving away, it closes at zero. Without a positive mass or a body there
    // is no impact.
    let away = measure(&world, SHOT_MASS, Vec2::new(-SPEED, 0.0), PLATE, face).unwrap();
    assert_eq!(away.closing_speed, 0.0);
    assert_eq!(measure(&world, 0.0, velocity, PLATE, face), None);
    assert_eq!(measure(&world, SHOT_MASS, velocity, PLATE + 1, face), None);

    // A loose plate closing at 10 adds its speed, and its mass shares the
    // impact's. Motion the host recorded entering the step counts instead of
    // the plate's motion now.
    let mut world = crate::world();
    loose_plate(&mut world, 20.0, Vec2::new(-10.0, 0.0), true);
    let big = world.body_mass(body(PLATE)).unwrap();
    let load = measure(&world, SHOT_MASS, velocity, PLATE, face).unwrap();
    assert_eq!(load.closing_speed, SPEED + 10.0);
    let m_eff = SHOT_MASS * big / (SHOT_MASS + big);
    assert!((load.effective_mass / m_eff - 1.0).abs() < 1.0e-6);
    let mut recorded = world.motion(body(PLATE)).unwrap();
    recorded.linear_velocity = Vec2::new(-20.0, 0.0);
    let load = projectile_impact(
        &world,
        SHOT_MASS,
        velocity,
        collider(PLATE, 0),
        Some(recorded),
        normal,
        face,
    )
    .unwrap();
    assert_eq!(load.closing_speed, SPEED + 20.0);

    // Free to turn and struck 1 wu off its center, it also gives way by its
    // inertia.
    let mut world = crate::world();
    loose_plate(&mut world, 20.0, Vec2::ZERO, false);
    let inertia = world.dynamic_body_inertia(body(PLATE)).unwrap();
    let load = measure(&world, SHOT_MASS, velocity, PLATE, Vec2::new(7.5, 1.0)).unwrap();
    let m_eff = 1.0 / (1.0 / SHOT_MASS + 1.0 / big + 1.0 / inertia);
    assert!((load.effective_mass / m_eff - 1.0).abs() < 1.0e-6);
    // Spinning at 5 rad/s, the struck point 1 wu from its center comes at the
    // projectile at 5 wu/s.
    let mut spinning = world.motion(body(PLATE)).unwrap();
    spinning.angular_velocity = 5.0;
    let load = projectile_impact(
        &world,
        SHOT_MASS,
        velocity,
        collider(PLATE, 0),
        Some(spinning),
        normal,
        Vec2::new(7.5, 1.0),
    )
    .unwrap();
    assert_eq!(load.closing_speed, SPEED + 5.0);
}

#[test]
fn a_protected_shot_passes_its_firer_uncharged() {
    let mut world = world();
    let firer = shot_body(&mut world, FIRER, Vec2::new(1.0, 0.0), Vec2::ZERO);
    let token = world
        .create_loading_region(
            RegionId {
                entity: firer.entity,
                part: 0,
            },
            firer,
        )
        .unwrap();
    world
        .register_loading_material(token, armor(), PLATE_SECTION)
        .unwrap();
    world
        .bind_loading_collider(
            collider(FIRER, 0),
            ContactSource {
                region: token,
                patch: 0,
            },
        )
        .unwrap();
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    shot_body(
        &mut world,
        SHOT,
        Vec2::new(-2.0, 0.0),
        Vec2::new(SPEED, 0.0),
    );
    weapons
        .launch(
            &world,
            ShotSpec {
                lifetime: 0.1,
                protected: Some(firer.entity),
                ..spec(SHOT, None)
            },
        )
        .unwrap();
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert!(delivered(&events, Part::Kinetic, Recipient::Region(token)).is_empty());
    assert!(events.iter().any(|event| matches!(
        event,
        WeaponEvent::Excluded { part: Part::Kinetic, recipient, reason: Exclusion::Protected, .. }
            if *recipient == Recipient::Region(token)
    )));
    assert_eq!(
        world.loading_material_remaining(token),
        Some(armor().remaining_precise())
    );
}

#[test]
fn a_slow_shot_passes_moss_on_the_plate_and_bursts_where_it_stops() {
    // A shot whose contacts are predicted along its motion stops at the
    // plate's face within the step that reaches it, the step in which it
    // first reaches moss growing on the face.
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&[7.565]).with_plate(plate, 8.0);
    let mut weapons = Weapons::new();
    let speed = 30.0;
    let area = std::f32::consts::PI * SHOT_RADIUS * SHOT_RADIUS;
    assert!(world.insert_body(
        body(SHOT),
        BodySpec {
            position: Vec2::new(5.06, 0.0),
            linear_velocity: Vec2::new(speed, 0.0),
            can_sleep: false,
            soft_ccd_prediction: 1.0,
            ..Default::default()
        },
        &[ColliderSpec {
            density: SHOT_MASS / area,
            ..ColliderSpec::ball(collider(SHOT, 0), SHOT_RADIUS)
        }],
    ));
    weapons.launch(&world, spec(SHOT, Some(PAYLOAD))).unwrap();
    let mass = world.body_mass(body(SHOT)).unwrap();
    let blade = *field.integrity.region(field.blades[0]).unwrap();
    let events = run(&mut world, &mut weapons, &mut field, 0);

    // The moss sees the speed the shot arrived with, not the rest it came to.
    let pass = delivered(&events, Part::Kinetic, Recipient::Tissue(field.blades[0]));
    assert_eq!(pass.len(), 1);
    let load = Impact {
        closing_speed: speed,
        effective_mass: effective_mass(
            ContactBody::translating(mass),
            ContactBody::translating(blade.mass()),
        )
        .unwrap(),
    };
    assert_eq!(pass[0].ledger.available, load.energy());

    // The blast is centered where the step left the shot against the plate,
    // not where the step began, at x = 7.185, and divided there.
    let center = events
        .iter()
        .find_map(|event| match event {
            WeaponEvent::Burst { center, .. } => Some(*center),
            _ => None,
        })
        .unwrap();
    assert!((7.2..7.4).contains(&center.x) && center.y.abs() < 1.0e-4);
    let targets: Vec<Target> = field
        .near(center, PAYLOAD.radius)
        .iter()
        .map(|circle| Target {
            key: u64::from(circle.recipient != plate),
            center: circle.center,
            radius: circle.radius,
            thickness: circle.thickness,
        })
        .collect();
    let reaches = blast_reach(center, PAYLOAD.radius, PAYLOAD.height, &targets).unwrap();
    let share = delivered(&events, Part::Payload, plate);
    assert_eq!(
        share[0].ledger.available,
        PAYLOAD.energy * reaches[0].coupling
    );
}

#[test]
fn launches_need_a_free_id_and_a_free_dynamic_body() {
    let mut world = world();
    plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, None);
    let beam = BeamSpec {
        id: SHOT,
        owner: None,
        pulses: Pulses {
            energy: 1.0,
            count: 1,
        },
        width: 0.5,
        height: 1.0,
        collision_groups: CollisionGroups::ALL,
        protected: None,
    };
    shot_body(&mut world, 3, Vec2::new(0.0, 5.0), Vec2::ZERO);
    let refused = [
        weapons.launch(&world, spec(SHOT, None)),
        weapons.arm_beam(beam),
        // A second shot on the first one's body.
        weapons.launch(
            &world,
            ShotSpec {
                id: 3,
                ..spec(SHOT, None)
            },
        ),
        // The plate is fixed.
        weapons.launch(
            &world,
            ShotSpec {
                body: body(PLATE),
                ..spec(3, None)
            },
        ),
        weapons.launch(
            &world,
            ShotSpec {
                radius: 0.0,
                ..spec(3, None)
            },
        ),
        weapons.arm_beam(BeamSpec {
            id: 4,
            width: 0.0,
            ..beam
        }),
        // Smaller than the reach geometry accepts.
        weapons.launch(
            &world,
            ShotSpec {
                radius: 1.0e-7,
                ..spec(3, None)
            },
        ),
    ];
    let geometry = WeaponError::Source(SourceError::Material(MaterialError::InvalidGeometry));
    assert_eq!(
        refused,
        [
            Err(WeaponError::Exists),
            Err(WeaponError::Exists),
            Err(WeaponError::InvalidBody),
            Err(WeaponError::InvalidBody),
            Err(geometry),
            Err(geometry),
            Err(geometry),
        ]
    );
    // Retiring the shot would remove a second body that shares its entity.
    shot_body(&mut world, 5, Vec2::new(0.0, -5.0), Vec2::ZERO);
    assert!(world.insert_body(
        BodyId::new(PhysicsId::new(5), BodyRole::new(1)),
        BodySpec {
            position: Vec2::new(1.0, -5.0),
            ..Default::default()
        },
        &[ColliderSpec::ball(collider(5, 1), 0.1)],
    ));
    // The contact response would charge a bound shot's arrival too.
    shot_body(&mut world, 6, Vec2::new(0.0, -8.0), Vec2::ZERO);
    let bound = world
        .create_loading_region(
            RegionId {
                entity: PhysicsId::new(6),
                part: 0,
            },
            body(6),
        )
        .unwrap();
    world
        .bind_loading_collider(
            collider(6, 0),
            ContactSource {
                region: bound,
                patch: 0,
            },
        )
        .unwrap();
    for entity in [5, 6] {
        assert_eq!(
            weapons.launch(&world, spec(entity, None)),
            Err(WeaponError::InvalidBody)
        );
    }
    // A world that keeps no contact events could never stop a shot.
    let mut quiet = PhysicsWorld::new(PhysicsWorldConfig {
        collect_events: false,
        ..Default::default()
    });
    shot_body(&mut quiet, SHOT, Vec2::ZERO, Vec2::ZERO);
    assert_eq!(
        Weapons::new().launch(&quiet, spec(SHOT, None)),
        Err(WeaponError::NoContactEvents)
    );
    assert_eq!(weapons.shots().collect::<Vec<_>>(), [(SHOT, body(SHOT))]);
    assert!(weapons.source(3).is_none() && weapons.source(4).is_none());
    weapons.launch(&world, spec(3, None)).unwrap();
}

#[test]
fn a_touch_that_pushes_nothing_does_not_stop_a_shot() {
    // A rail runs under the shot's path, inside the contact prediction
    // distance, so the shot touches it all the way without pushing on it.
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    assert!(world.insert_body(
        body(RAIL),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(3.5, -SHOT_RADIUS - 0.2505),
            ..Default::default()
        },
        &[ColliderSpec::cuboid(collider(RAIL, 0), 3.0, 0.25)],
    ));
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, None);
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert_eq!(stops(&events), [(SHOT, body(PLATE))]);
    assert_eq!(delivered(&events, Part::Kinetic, plate).len(), 1);
}

fn stops(events: &[WeaponEvent]) -> Vec<(u64, BodyId)> {
    events
        .iter()
        .filter_map(|event| match event {
            WeaponEvent::Stopped { source, body, .. } => Some((*source, *body)),
            _ => None,
        })
        .collect()
}

#[test]
fn shots_in_one_step_each_meet_what_they_hit() {
    // Two shots strike one plate in the same step, and both charge it.
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    for (entity, y) in [(SHOT, -1.0), (3, 1.0)] {
        shot_body(&mut world, entity, Vec2::new(0.0, y), Vec2::new(SPEED, 0.0));
        weapons.launch(&world, spec(entity, None)).unwrap();
    }
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert_eq!(stops(&events), [(SHOT, body(PLATE)), (3, body(PLATE))]);
    let hits = delivered(&events, Part::Kinetic, plate);
    assert_eq!(hits.len(), 2);
    let remaining = world.loading_material_remaining(token(plate)).unwrap();
    let charged = hits
        .iter()
        .map(|hit| f64::from(hit.consumed[1]))
        .sum::<f64>();
    assert!(close(armor().remaining_precise() - remaining, charged));
    assert_eq!(retired(&events).len(), 2);

    // Two shells fired at each other meet halfway. Each stops on the other
    // and bursts, whichever is settled first.
    let mut world = crate::world();
    let mut weapons = Weapons::new();
    shot_body(&mut world, SHOT, Vec2::ZERO, Vec2::new(SPEED, 0.0));
    shot_body(&mut world, 3, Vec2::new(6.0, 0.0), Vec2::new(-SPEED, 0.0));
    for entity in [SHOT, 3] {
        weapons.launch(&world, spec(entity, Some(PAYLOAD))).unwrap();
    }
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert_eq!(stops(&events), [(SHOT, body(3)), (3, body(SHOT))]);
    let bursts = events
        .iter()
        .filter(|event| matches!(event, WeaponEvent::Burst { .. }))
        .count();
    assert_eq!(bursts, 2);
    for (id, other) in [(SHOT, 3), (3, SHOT)] {
        assert!(events.iter().any(|event| matches!(
            event,
            WeaponEvent::Excluded {
                source,
                part: Part::Kinetic,
                recipient,
                reason: Exclusion::Unmodeled,
                ..
            } if *source == id && *recipient == Recipient::Unmodeled(body(other))
        )));
    }
    assert!(world.motion(body(SHOT)).is_none() && world.motion(body(3)).is_none());
}

#[test]
fn a_hit_is_measured_where_the_target_was_when_struck() {
    // A plate free to turn slides across the shot's path, a whole wu each
    // step at this coarse step. Its lever about its center is measured where
    // the center was when the step found the contact, a wu from where it is
    // when the step ends.
    let dt = 1.0 / 30.0;
    let mut world = world();
    let plate = loose_plate(&mut world, 20.0, Vec2::new(0.0, 30.0), false);
    let inertia = f64::from(world.dynamic_body_inertia(body(PLATE)).unwrap());
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    shot_body(&mut world, SHOT, Vec2::new(0.0, 2.0), Vec2::new(SPEED, 0.0));
    weapons.launch(&world, spec(SHOT, None)).unwrap();
    let [m, big] = [SHOT, PLATE].map(|entity| f64::from(world.body_mass(body(entity)).unwrap()));
    for _ in 0..30 {
        let center = world.center_of_mass(body(PLATE)).unwrap();
        let prepared = weapons.prepare(&world, dt);
        world.step(dt);
        weapons.settle(prepared, &mut world, dt, &mut field);
        let events = weapons.take_events();
        let Some(point) = events.iter().find_map(|event| match event {
            WeaponEvent::Stopped { point, .. } => Some(*point),
            _ => None,
        }) else {
            continue;
        };
        let lever = f64::from(point.y - center.y);
        assert!(lever > 0.5, "{lever}");
        let m_eff = 1.0 / (1.0 / m + 1.0 / big + lever * lever / inertia);
        let expected = 0.5 * m_eff * f64::from(SPEED).powi(2);
        let hit = &delivered(&events, Part::Kinetic, plate)[0];
        assert!(
            (f64::from(hit.ledger.available) / expected - 1.0).abs() < 1.0e-3,
            "{} against {expected}",
            hit.ledger.available
        );
        return;
    }
    panic!("the shot never struck");
}

#[test]
fn a_driven_plate_brings_its_prescribed_speed_into_the_hit() {
    // A position-driven plate speeds up toward the shot each step, so the
    // velocity it ended the last step with is not the one it is driven at.
    let mut world = world();
    assert!(world.insert_body(
        body(PLATE),
        BodySpec {
            kind: BodyKind::KinematicPosition,
            position: Vec2::new(8.0, 0.0),
            ..Default::default()
        },
        &[ColliderSpec::cuboid(collider(PLATE, 0), 0.5, 2.0)],
    ));
    let plate = register(&mut world, Some(armor()), 1);
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, None);
    let m = f64::from(world.body_mass(body(SHOT)).unwrap());
    let mut x = 8.0;
    for step in 1..=60 {
        let speed = 10.0 * step as f32;
        x -= speed * DT;
        assert!(world.set_next_kinematic_pose(body(PLATE), Vec2::new(x, 0.0), 0.0));
        let prepared = weapons.prepare(&world, DT);
        world.step(DT);
        weapons.settle(prepared, &mut world, DT, &mut field);
        let events = weapons.take_events();
        if let Some(hit) = delivered(&events, Part::Kinetic, plate).first() {
            let expected = 0.5 * m * f64::from(SPEED + speed).powi(2);
            assert!(
                (f64::from(hit.ledger.available) / expected - 1.0).abs() < 1.0e-4,
                "{} against {expected}",
                hit.ledger.available
            );
            return;
        }
    }
    panic!("the shot never struck");
}

#[test]
fn a_shot_launched_after_prepare_sweeps_from_its_launch() {
    let mut world = world();
    plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    // A blade within the shot's first step, which a sweep from where the
    // step left the shot would miss.
    let mut field = Field::new(&[0.5]);
    let mut weapons = Weapons::new();
    let prepared = weapons.prepare(&world, DT);
    fire(&mut world, &mut weapons, None);
    world.step(DT);
    weapons.settle(prepared, &mut world, DT, &mut field);
    let events = weapons.take_events();
    let blade = Recipient::Tissue(field.blades[0]);
    assert_eq!(delivered(&events, Part::Kinetic, blade).len(), 1);
}

#[test]
fn a_source_can_be_retired_early() {
    let mut world = world();
    plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&BLADES);
    let mut weapons = Weapons::new();
    weapons
        .arm_beam(BeamSpec {
            pulses: Pulses {
                energy: 100.0,
                count: 3,
            },
            ..beam(7)
        })
        .unwrap();
    // The physics' query index is the last step's.
    world.step(DT);
    weapons
        .fire(&mut world, 7, east(100.0), &mut field)
        .unwrap();
    weapons.take_events();
    // A trigger released after one of three pulses.
    weapons.retire(&mut world, 7).unwrap();
    let events = weapons.take_events();
    let [WeaponEvent::Retired { ledger, .. }] = events.as_slice() else {
        panic!("{events:?}");
    };
    assert_eq!(ledger.beam.budget, 300.0);
    assert!((ledger.beam.delivered - 100.0).abs() < 1.0e-3);
    assert_eq!(
        weapons.fire(&mut world, 7, east(100.0), &mut field),
        Err(WeaponError::UnknownSource)
    );
    assert_eq!(
        weapons.retire(&mut world, 7),
        Err(WeaponError::UnknownSource)
    );
    // A dud leaves without a blast, and so does a shot that has none.
    fire(&mut world, &mut weapons, Some(PAYLOAD));
    weapons.retire(&mut world, SHOT).unwrap();
    let events = weapons.take_events();
    assert_eq!(retired(&events).len(), 1);
    assert_eq!(events.len(), 1);
    assert!(world.motion(body(SHOT)).is_none());
    fire(&mut world, &mut weapons, None);
    weapons.detonate(&mut world, SHOT, &mut field).unwrap();
    let events = weapons.take_events();
    assert_eq!(retired(&events).len(), 1);
    assert_eq!(events.len(), 1);
    assert!(world.motion(body(SHOT)).is_none());
}

/// A beam of one pulse of 100, 0.5 wide and 1 high.
fn beam(id: u64) -> BeamSpec {
    BeamSpec {
        id,
        owner: Some(FIRER),
        pulses: Pulses {
            energy: 100.0,
            count: 1,
        },
        width: 0.5,
        height: 1.0,
        collision_groups: CollisionGroups::ALL,
        protected: None,
    }
}

/// A pulse from the origin along +x, as far as 20, carrying `energy`.
fn east(energy: f32) -> Pulse {
    Pulse {
        start: Vec2::ZERO,
        direction: Vec2::new(1.0, 0.0),
        range: 20.0,
        energy,
    }
}

/// Arms a beam and fires one full pulse `east`.
fn fire_beam(
    world: &mut PhysicsWorld,
    weapons: &mut Weapons,
    spec: BeamSpec,
    field: &mut Field,
) -> Vec<WeaponEvent> {
    weapons.arm_beam(spec).unwrap();
    weapons
        .fire(world, spec.id, east(spec.pulses.energy), field)
        .unwrap();
    weapons.take_events()
}

#[test]
fn a_beam_passes_bodies_outside_its_collision_groups() {
    // A pane in group 2 stands before the plate.
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    assert!(world.insert_body(
        body(PANE),
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(5.0, 0.0),
            ..Default::default()
        },
        &[ColliderSpec {
            collision_groups: CollisionGroups::new(2, u32::MAX),
            ..ColliderSpec::cuboid(collider(PANE, 0), 0.1, 2.0)
        }],
    ));
    world.step(DT);
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    let mut traced = Vec::new();
    for (id, collision_groups) in [
        (7, CollisionGroups::ALL),
        (8, CollisionGroups::new(1, 1)),
        (9, CollisionGroups::NONE),
    ] {
        let spec = BeamSpec {
            collision_groups,
            ..beam(id)
        };
        let events = fire_beam(&mut world, &mut weapons, spec, &mut field);
        traced.extend(events.iter().filter_map(|event| match event {
            WeaponEvent::Traced { end, body, .. } => Some((end.x, *body)),
            _ => None,
        }));
        if id == 7 {
            // The pane has no material, so its energy is reported.
            assert!(events.contains(&WeaponEvent::Excluded {
                source: id,
                part: Part::Beam,
                recipient: Recipient::Unmodeled(body(PANE)),
                reason: Exclusion::Unmodeled,
                energy: 100.0,
            }));
        }
        if id == 8 {
            assert_eq!(delivered(&events, Part::Beam, plate).len(), 1);
        }
    }
    // The last beam meets nothing and runs out at its range.
    assert_eq!(
        traced,
        [
            (4.9, Some(body(PANE))),
            (7.5, Some(body(PLATE))),
            (20.0, None)
        ]
    );
}

#[test]
fn a_recipient_with_a_circle_the_geometry_refuses_is_refused() {
    // The host gives the middle blade a second circle with no thickness.
    let mut field = Field::new(&BLADES);
    let middle = Recipient::Tissue(field.blades[1]);
    field.circles.push(Reachable {
        thickness: 0.0,
        ..field.circles[1]
    });
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    world.step(DT);
    let mut weapons = Weapons::new();
    let geometry = SourceError::Material(MaterialError::InvalidGeometry);
    // The beam refuses it and reaches the rest.
    let events = fire_beam(&mut world, &mut weapons, beam(7), &mut field);
    assert_eq!(refusals(&events), [(Part::Beam, middle, geometry)]);
    assert!(delivered(&events, Part::Beam, middle).is_empty());
    for blade in [field.blades[0], field.blades[2]] {
        assert_eq!(
            delivered(&events, Part::Beam, Recipient::Tissue(blade)).len(),
            1
        );
    }
    assert_eq!(delivered(&events, Part::Beam, plate).len(), 1);
    // So does a shot's sweep.
    fire(&mut world, &mut weapons, None);
    let events = run(&mut world, &mut weapons, &mut field, 0);
    assert!(refusals(&events).contains(&(Part::Kinetic, middle, geometry)));
    assert!(delivered(&events, Part::Kinetic, middle).is_empty());
    assert_eq!(delivered(&events, Part::Kinetic, plate).len(), 1);
}

#[test]
fn tissue_the_host_cannot_charge_is_reported() {
    // The host admits tissue it keeps no material for: one circle on the
    // beam's path and one beside the plate, in the blast.
    let unknown = [Recipient::Tissue(900), Recipient::Tissue(901)];
    let mut field = Field::new(&[])
        .with_circle(unknown[0], Vec2::new(3.0, 0.0), 0.1)
        .with_circle(unknown[1], Vec2::new(6.5, 1.5), 0.1);
    let mut world = world();
    plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    world.step(DT);
    let mut weapons = Weapons::new();
    let excluded = |events: &[WeaponEvent], part: Part| -> Vec<(Recipient, f32)> {
        events
            .iter()
            .filter_map(|event| match event {
                WeaponEvent::Excluded {
                    part: excluded,
                    recipient,
                    reason: Exclusion::Unmodeled,
                    energy,
                    ..
                } if *excluded == part => Some((*recipient, *energy)),
                _ => None,
            })
            .collect()
    };
    // The beam crosses the first; the blast reaches both.
    let events = fire_beam(&mut world, &mut weapons, beam(7), &mut field);
    let beam = excluded(&events, Part::Beam);
    assert_eq!(beam.len(), 1);
    assert_eq!(beam[0].0, unknown[0]);
    assert!(beam[0].1 > 0.0);
    fire(&mut world, &mut weapons, Some(PAYLOAD));
    let events = run(&mut world, &mut weapons, &mut field, 0);
    let blast = excluded(&events, Part::Payload);
    assert_eq!(
        blast
            .iter()
            .map(|(recipient, _)| *recipient)
            .collect::<Vec<_>>(),
        unknown
    );
    assert!(blast.iter().all(|(_, energy)| *energy > 0.0));
}

#[test]
fn a_body_the_host_keeps_takes_the_hit_a_registered_region_would() {
    // The same shot strikes a plate whose armor the world keeps, and one whose
    // armor the host keeps.
    let (mut registered, mut kept) = (world(), world());
    let plate = plate(&mut registered, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&[]);
    let hull = hull(&mut kept, &mut field, 8.0, slab());
    let mut outcomes = Vec::new();
    for (world, recipient) in [(&mut registered, plate), (&mut kept, hull)] {
        world.step(DT);
        let mut weapons = Weapons::new();
        fire(world, &mut weapons, None);
        let events = run(world, &mut weapons, &mut field, 0);
        outcomes.push(delivered(&events, Part::Kinetic, recipient));
    }
    assert_eq!(outcomes[0].len(), 1);
    assert_eq!(outcomes[0], outcomes[1]);
    // Each keeper took the same work.
    let left = registered.loading_material_remaining(token(plate)).unwrap();
    assert!((field.left(HULL) - left).abs() < 1.0e-6);
    assert!(left < armor().remaining_precise());
}

#[test]
fn the_hosts_policy_shields_what_it_keeps() {
    // The host shields its plate and the middle blade. A shot through the
    // blades stops on the plate and bursts there.
    let mut world = world();
    let mut field = Field::new(&BLADES);
    let hull = hull(&mut world, &mut field, 8.0, slab());
    let mut field = field.with_plate(hull, 8.0);
    let middle = Recipient::Tissue(field.blades[1]);
    field.shielded = vec![hull, middle];
    world.step(DT);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, Some(PAYLOAD));
    let events = run(&mut world, &mut weapons, &mut field, 0);
    let shielded = |part| -> Vec<(Recipient, f32)> {
        events
            .iter()
            .filter_map(|event| match event {
                WeaponEvent::Excluded {
                    part: excluded,
                    recipient,
                    reason: Exclusion::Protected,
                    energy,
                    ..
                } if *excluded == part => Some((*recipient, *energy)),
                _ => None,
            })
            .collect()
    };
    // The shot passes the middle blade untouched, and the plate stops it.
    let kinetic = shielded(Part::Kinetic);
    assert_eq!(
        kinetic
            .iter()
            .map(|(recipient, _)| *recipient)
            .collect::<Vec<_>>(),
        [middle, hull]
    );
    assert!(kinetic.iter().all(|(_, energy)| *energy > 0.0));
    assert!(events.iter().any(|event| matches!(
        event,
        WeaponEvent::Stopped { body: stopped, .. } if *stopped == body(PLATE)
    )));
    let payload = shielded(Part::Payload);
    assert_eq!(
        payload
            .iter()
            .filter(|(recipient, _)| *recipient == hull)
            .count(),
        1
    );
    // Neither is charged, while the other blades are.
    for part in [Part::Kinetic, Part::Payload] {
        assert!(delivered(&events, part, hull).is_empty());
        assert!(delivered(&events, part, middle).is_empty());
    }
    assert_eq!(field.left(HULL), armor().remaining_precise());
    for blade in [field.blades[0], field.blades[2]] {
        assert_eq!(
            delivered(&events, Part::Kinetic, Recipient::Tissue(blade)).len(),
            1
        );
    }
}

#[test]
fn a_beam_stops_on_a_body_the_host_keeps() {
    // The host gives its plate no circles, so it takes all of a pulse that
    // reaches it.
    let mut world = world();
    let mut field = Field::new(&[]);
    let hull = hull(&mut world, &mut field, 8.0, slab());
    world.step(DT);
    let mut weapons = Weapons::new();
    let events = fire_beam(&mut world, &mut weapons, beam(7), &mut field);
    let outcomes = delivered(&events, Part::Beam, hull);
    assert_eq!(outcomes.len(), 1);
    assert!((outcomes[0].ledger.available - 100.0).abs() < 1.0e-3);
    assert!(outcomes[0].consumed[0] > 0.0);
    let charged = armor().remaining_precise() - field.left(HULL);
    assert!((charged - f64::from(outcomes[0].consumed[0])).abs() < 1.0e-3);
    // While the host shields it, it still stops the beam, which charges
    // nothing.
    field.shielded = vec![hull];
    let events = fire_beam(&mut world, &mut weapons, beam(8), &mut field);
    assert!(events.contains(&WeaponEvent::Excluded {
        source: 8,
        part: Part::Beam,
        recipient: hull,
        reason: Exclusion::Protected,
        energy: 100.0,
    }));
    assert!(events.contains(&WeaponEvent::Traced {
        source: 8,
        start: Vec2::ZERO,
        end: Vec2::new(7.5, 0.0),
        body: Some(body(PLATE)),
    }));
    assert!((charged - (armor().remaining_precise() - field.left(HULL))).abs() < 1.0e-9);
}

/// Whether any event delivers energy to `recipient` or reports some reaching
/// it.
fn reached(events: &[WeaponEvent], recipient: Recipient) -> bool {
    events.iter().any(|event| match event {
        WeaponEvent::Delivered { recipient: r, .. } => *r == recipient,
        WeaponEvent::Excluded {
            recipient: r,
            energy,
            ..
        } => *r == recipient && *energy > 0.0,
        _ => false,
    })
}

#[test]
fn a_beam_reaches_no_tissue_past_the_body_it_stops_on() {
    // A ball whose circle reaches past the beam's end at its face, x = 7.5,
    // and a blade beside it, behind the face, in the part of a 2 wide beam
    // the ball misses. The ball takes the half of the beam it covers, and the
    // beam ends at the face, so the blade is not reached.
    let mut world = world();
    let mut field = Field::new(&[]);
    let ball = ColliderSpec::ball(collider(PLATE, 0), 0.5);
    let hull = hull(&mut world, &mut field, 8.0, ball);
    let mut field = field.with_circle(hull, Vec2::new(8.0, 0.0), 0.5);
    let behind = Recipient::Tissue(99);
    field.circles.push(Reachable {
        recipient: behind,
        center: Vec2::new(8.3, 0.7),
        radius: 0.1,
        thickness: 1.0,
    });
    world.step(DT);
    let mut weapons = Weapons::new();
    let spec = BeamSpec {
        width: 2.0,
        ..beam(7)
    };
    let events = fire_beam(&mut world, &mut weapons, spec, &mut field);
    let shares = delivered(&events, Part::Beam, hull);
    assert_eq!(shares.len(), 1);
    assert!((shares[0].ledger.available - 50.0).abs() < 1.0e-3);
    assert!(!reached(&events, behind));
}

#[test]
fn a_beam_meets_the_bodies_across_its_path_before_the_one_it_stops_on() {
    // A 2 wide beam ends on the plate's face at x = 7.5. A ball the host
    // keeps stands off the ray at (5, 0.6), with a blade in its shadow. The
    // ball takes the part of the beam it covers, the blade nothing, and the
    // plate what passes the ball.
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    let mut field = Field::new(&[]);
    let at = Vec2::new(5.0, 0.6);
    let shape = ColliderSpec::ball(collider(BALL, 0), 0.3);
    let ball = hosted(&mut world, &mut field, BALL, BALL_HULL, at, shape);
    let mut field = field.with_circle(ball, at, 0.3);
    let shaded = Recipient::Tissue(98);
    field.circles.push(Reachable {
        recipient: shaded,
        center: Vec2::new(6.5, 0.6),
        radius: 0.1,
        thickness: 1.0,
    });
    world.step(DT);
    let mut weapons = Weapons::new();
    let spec = BeamSpec {
        width: 2.0,
        ..beam(7)
    };
    let events = fire_beam(&mut world, &mut weapons, spec, &mut field);
    let shares = delivered(&events, Part::Beam, ball);
    assert_eq!(shares.len(), 1);
    assert!((shares[0].ledger.available - 30.0).abs() < 1.0e-3);
    assert!(field.left(BALL_HULL) < armor().remaining_precise());
    let ends = delivered(&events, Part::Beam, plate);
    assert_eq!(ends.len(), 1);
    assert!((ends[0].ledger.available - 70.0).abs() < 1.0e-3);
    assert!(!reached(&events, shaded));
}

#[test]
fn a_region_over_several_bodies_of_an_entity_is_one_recipient() {
    // A rover's chassis at x = 8 and its wheel at x = 6 share one region,
    // and the host lists a circle for each. A 2 wide beam ends on the wheel.
    // The region is one recipient whichever of its bodies the ray meets, so
    // it takes, once, the part of the beam the two circles cover together:
    // the chassis's, which shades the wheel's.
    let mut world = world();
    let mut field = Field::new(&[]);
    let chassis = ColliderSpec::ball(collider(ROVER, 0), 0.5);
    let rover = hosted(
        &mut world,
        &mut field,
        ROVER,
        ROVER_HULL,
        Vec2::new(8.0, 0.0),
        chassis,
    );
    let wheel = BodyId::new(PhysicsId::new(ROVER), BodyRole::new(1));
    assert!(world.insert_body(
        wheel,
        BodySpec {
            kind: BodyKind::Fixed,
            position: Vec2::new(6.0, 0.0),
            ..Default::default()
        },
        &[ColliderSpec::ball(collider(ROVER, 1), 0.3)],
    ));
    let mut field = field
        .with_circle(rover, Vec2::new(8.0, 0.0), 0.5)
        .with_circle(rover, Vec2::new(6.0, 0.0), 0.3);
    world.step(DT);
    let mut weapons = Weapons::new();
    let spec = BeamSpec {
        width: 2.0,
        ..beam(7)
    };
    let events = fire_beam(&mut world, &mut weapons, spec, &mut field);
    assert!(events.iter().any(|event| matches!(
        event,
        WeaponEvent::Traced { body: Some(body), .. } if *body == wheel
    )));
    let shares = delivered(&events, Part::Beam, rover);
    assert_eq!(shares.len(), 1);
    assert!((shares[0].ledger.available - 50.0).abs() < 1.0e-3);
    // A pulse whose range ends before the chassis's circle begins meets only
    // the wheel's.
    weapons.arm_beam(BeamSpec { id: 8, ..spec }).unwrap();
    let short = Pulse {
        range: 7.0,
        ..east(100.0)
    };
    weapons.fire(&mut world, 8, short, &mut field).unwrap();
    let events = weapons.take_events();
    let shares = delivered(&events, Part::Beam, rover);
    assert_eq!(shares.len(), 1);
    assert!((shares[0].ledger.available - 30.0).abs() < 1.0e-3);
}

#[test]
fn a_beam_refuses_a_recipient_with_a_bad_circle_and_reaches_the_rest() {
    // The host lists the plate it keeps with a circle of infinite radius,
    // gives the first blade a second circle at no finite place, and lists a
    // blade with a negative radius on the path. Each is refused whole, and the
    // other blades still take their shares. Circles with no thickness beside
    // the path, behind the muzzle and behind the plate cannot be met, so they
    // are not refused.
    let mut world = world();
    let mut field = Field::new(&BLADES);
    let hull = hull(&mut world, &mut field, 8.0, slab());
    let mut field = field.with_circle(hull, Vec2::new(8.0, 0.0), f32::INFINITY);
    let first = Recipient::Tissue(field.blades[0]);
    field.circles.push(Reachable {
        center: Vec2::new(f32::NAN, 0.0),
        ..field.circles[0]
    });
    for (key, center) in [
        (902, Vec2::new(3.0, 5.0)),
        (903, Vec2::new(-0.2, 0.0)),
        (904, Vec2::new(15.0, 0.0)),
    ] {
        field.circles.push(Reachable {
            recipient: Recipient::Tissue(key),
            center,
            radius: 0.1,
            thickness: 0.0,
        });
    }
    let inside_out = Recipient::Tissue(905);
    field.circles.push(Reachable {
        recipient: inside_out,
        center: Vec2::new(5.0, 0.0),
        radius: -0.6,
        thickness: 1.0,
    });
    world.step(DT);
    let mut weapons = Weapons::new();
    let events = fire_beam(&mut world, &mut weapons, beam(7), &mut field);
    let geometry = SourceError::Material(MaterialError::InvalidGeometry);
    assert_eq!(
        refusals(&events),
        [
            (Part::Beam, hull, geometry),
            (Part::Beam, first, geometry),
            (Part::Beam, inside_out, geometry)
        ]
    );
    for blade in &field.blades[1..] {
        assert_eq!(
            delivered(&events, Part::Beam, Recipient::Tissue(*blade)).len(),
            1
        );
    }
    assert!(!reached(&events, first));
    assert!(!reached(&events, hull));
    assert_eq!(field.left(HULL), armor().remaining_precise());
}

#[test]
fn a_pulse_reaches_and_carries_what_the_host_gives_it() {
    let mut world = world();
    let plate = plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    world.step(DT);
    let mut field = Field::new(&[]);
    let mut weapons = Weapons::new();
    let spec = BeamSpec {
        pulses: Pulses {
            energy: 100.0,
            count: 2,
        },
        ..beam(7)
    };
    weapons.arm_beam(spec).unwrap();
    // The first pulse has grown to 5 and stops short of the plate's face.
    let short = Pulse {
        range: 5.0,
        ..east(100.0)
    };
    weapons.fire(&mut world, 7, short, &mut field).unwrap();
    let events = weapons.take_events();
    assert!(delivered(&events, Part::Beam, plate).is_empty());
    assert!(events.contains(&WeaponEvent::Traced {
        source: 7,
        start: Vec2::ZERO,
        end: Vec2::new(5.0, 0.0),
        body: None,
    }));
    // A pulse must reach somewhere and carry at most the beam's pulse energy,
    // and a refused one is not spent.
    let refused = |error| Err(WeaponError::Source(SourceError::Material(error)));
    let nowhere = Pulse {
        range: 0.0,
        ..east(100.0)
    };
    assert_eq!(
        weapons.fire(&mut world, 7, nowhere, &mut field),
        refused(MaterialError::InvalidGeometry)
    );
    for energy in [100.5, -1.0, f32::NAN] {
        assert_eq!(
            weapons.fire(&mut world, 7, east(energy), &mut field),
            refused(MaterialError::InvalidLoad)
        );
    }
    // The next carries 40, as a beam that weakens with range might, and the
    // plate takes all of it.
    weapons.fire(&mut world, 7, east(40.0), &mut field).unwrap();
    let events = weapons.take_events();
    let ends = delivered(&events, Part::Beam, plate);
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].ledger.available, 40.0);
    let WeaponEvent::Retired { ledger, .. } = retired(&events)[0] else {
        unreachable!()
    };
    assert_eq!(ledger.beam.budget, 200.0);
    assert_eq!(ledger.beam.delivered, 40.0);
}

#[test]
fn a_shielded_blade_already_passed_is_refused_as_a_duplicate() {
    // The first blade is passed in the step that ends on it, then shielded
    // before the next step's sweep reports it again. That pass is refused as
    // it would be unshielded, and nothing reaching the blade is reported.
    let mut world = world();
    plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    world.step(DT);
    let mut field = Field::new(&BLADES);
    let first = Recipient::Tissue(field.blades[0]);
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, None);
    let mut events = Vec::new();
    while delivered(&events, Part::Kinetic, first).is_empty() {
        let prepared = weapons.prepare(&world, DT);
        world.step(DT);
        weapons.settle(prepared, &mut world, DT, &mut field);
        events.extend(weapons.take_events());
    }
    field.shielded = vec![first];
    let later = run(&mut world, &mut weapons, &mut field, 0);
    assert!(refusals(&later).iter().any(|(part, recipient, error)| {
        *part == Part::Kinetic && *recipient == first && matches!(error, SourceError::Duplicate(_))
    }));
    assert!(!reached(&later, first));
}

#[test]
fn a_shielded_blade_on_a_face_the_geometry_refuses_is_refused() {
    // The host gives every blade a section with no thickness and shields the
    // middle one. Each pass is refused alike, and none reaches its blade.
    let mut world = world();
    plate(&mut world, Some(armor()), &[(0.0, 2.0)]);
    world.step(DT);
    let mut field = Field::new(&BLADES);
    field.section = Section::new(0.0, BLADE.depth);
    field.shielded = vec![Recipient::Tissue(field.blades[1])];
    let mut weapons = Weapons::new();
    fire(&mut world, &mut weapons, None);
    let events = run(&mut world, &mut weapons, &mut field, 0);
    let geometry = SourceError::Material(MaterialError::InvalidGeometry);
    for blade in &field.blades {
        let recipient = Recipient::Tissue(*blade);
        assert!(refusals(&events).contains(&(Part::Kinetic, recipient, geometry)));
        assert!(!reached(&events, recipient));
    }
}

#[test]
fn what_a_beam_cannot_deliver_is_reported_at_its_share() {
    // The host shields the plate it keeps and keeps no region for a blade in
    // front of it. Each is reported excluded with its share of the pulse, and
    // the shares add up to the pulse.
    let mut world = world();
    let mut field = Field::new(&[]);
    let hull = hull(&mut world, &mut field, 8.0, slab());
    let unknown = Recipient::Tissue(900);
    let mut field = field.with_circle(unknown, Vec2::new(3.0, 0.0), 0.1);
    field.shielded = vec![hull];
    world.step(DT);
    let mut weapons = Weapons::new();
    let events = fire_beam(&mut world, &mut weapons, beam(7), &mut field);
    let excluded = |recipient, reason| -> f32 {
        events
            .iter()
            .filter_map(|event| match event {
                WeaponEvent::Excluded {
                    recipient: r,
                    reason: why,
                    energy,
                    ..
                } if *r == recipient && *why == reason => Some(*energy),
                _ => None,
            })
            .sum()
    };
    let blade = excluded(unknown, Exclusion::Unmodeled);
    let plate = excluded(hull, Exclusion::Protected);
    assert!(blade > 0.0 && plate > blade);
    assert!((blade + plate - 100.0).abs() < 1.0e-3);
}

#[test]
fn a_stop_takes_only_what_its_circles_within_the_range_cover() {
    // The host lists a ball it keeps with a circle inside its collider, which
    // begins 0.1 past the surface the ray meets. A 2 wide pulse that ends
    // before the circle begins gives the ball nothing, and a longer one gives
    // it the part the circle covers.
    let mut world = world();
    let mut field = Field::new(&[]);
    let at = Vec2::new(8.0, 0.0);
    let shape = ColliderSpec::ball(collider(BALL, 0), 0.5);
    let ball = hosted(&mut world, &mut field, BALL, BALL_HULL, at, shape);
    let mut field = field.with_circle(ball, at, 0.4);
    world.step(DT);
    let mut weapons = Weapons::new();
    for (id, range, share) in [(7, 7.55, 0.0), (8, 20.0, 40.0)] {
        let spec = BeamSpec {
            width: 2.0,
            ..beam(id)
        };
        weapons.arm_beam(spec).unwrap();
        let pulse = Pulse {
            range,
            ..east(100.0)
        };
        weapons.fire(&mut world, id, pulse, &mut field).unwrap();
        let events = weapons.take_events();
        assert!(events.iter().any(|event| matches!(
            event,
            WeaponEvent::Traced { body: Some(body), .. } if *body == self::body(BALL)
        )));
        let taken: f32 = delivered(&events, Part::Beam, ball)
            .iter()
            .map(|outcome| outcome.ledger.available)
            .sum();
        assert!((taken - share).abs() < 1.0e-3, "{range}: {taken}");
    }
}
