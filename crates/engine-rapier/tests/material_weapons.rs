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
    BeamSpec, Blast, Exclusion, Reachable, Recipient, Recipients, ShotSpec, WeaponError,
    WeaponEvent, Weapons,
};
use engine_rapier::world::{
    BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, CollisionGroups,
    PhysicsId, PhysicsWorld, PhysicsWorldConfig,
};

const DT: f32 = 1.0 / 240.0;
const SPEED: f32 = 300.0;
const SHOT_RADIUS: f32 = 0.25;
const SHOT_MASS: f32 = 0.5;
const FIRER: u64 = 1;
const SHOT: u64 = 2;
const PLATE: u64 = 10;
const RAIL: u64 = 20;
const PANE: u64 = 21;
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

/// The host: three blades of moss, kept by the tissue adapter, and the
/// circles it admits to fronts.
struct Field {
    integrity: TissueIntegrity,
    blades: Vec<TissueId>,
    circles: Vec<Reachable>,
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
        }
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
            .filter(|circle| circle.center.distance_to(center) - circle.radius <= radius)
            .copied()
            .collect()
    }

    fn tissue(&self, key: u64) -> Option<(Region, Section)> {
        Some((*self.integrity.region(key)?, BLADE))
    }

    fn absorb(&mut self, key: u64, work: f32) {
        self.integrity.charge(key, work).unwrap();
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
            range: 20.0,
            collision_groups: CollisionGroups::ALL,
            protected: None,
        })
        .unwrap();
    // The physics' query index is the last step's.
    world.step(DT);
    for _ in 0..3 {
        weapons
            .fire(&mut world, 7, Vec2::ZERO, Vec2::new(1.0, 0.0), &mut field)
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
        weapons.fire(&mut world, 7, Vec2::ZERO, Vec2::new(1.0, 0.0), &mut field),
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
            range: 20.0,
            collision_groups: CollisionGroups::ALL,
            protected: Some(firer.entity),
        })
        .unwrap();
    world.step(DT);
    weapons
        .fire(&mut world, 7, Vec2::ZERO, Vec2::new(1.0, 0.0), &mut field)
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
        range: 1.0,
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
    let east = Vec2::new(1.0, 0.0);
    weapons
        .fire(&mut world, 7, Vec2::ZERO, east, &mut field)
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
        weapons.fire(&mut world, 7, Vec2::ZERO, east, &mut field),
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

/// A beam of one pulse of 100, 0.5 wide and 1 high, with a range of 20.
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
        range: 20.0,
        collision_groups: CollisionGroups::ALL,
        protected: None,
    }
}

/// Arms a beam and fires one pulse from the origin along +x.
fn fire_beam(
    world: &mut PhysicsWorld,
    weapons: &mut Weapons,
    spec: BeamSpec,
    field: &mut Field,
) -> Vec<WeaponEvent> {
    weapons.arm_beam(spec).unwrap();
    weapons
        .fire(world, spec.id, Vec2::ZERO, Vec2::new(1.0, 0.0), field)
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
