//! Weapon source budgets (#329): finite kinetic work, one payload release,
//! bounded beam pulses, recipients reached once, capacity refusal, and refused
//! calls that change nothing.

use engine_material::fixtures::{Body, geometry, head_on, shell, tank};
use engine_material::{
    ARMOR, ContactBody, ContactGeometry, Face, FailureMode, Impact, LEDGER_TOLERANCE, Material,
    MaterialError, Outcome, Part, Projectile, Pulses, Region, SOFT_TISSUE, Section, Share, Source,
    SourceError, SourceSpec, WOOD, deposit, effective_mass, impact,
};

/// Spacewars' cannon shell speed, `wu / s`.
const SPEED: f32 = 300.0;
const PAYLOAD: f32 = 5000.0;
const OWNER: u64 = 7;

/// A grass blade: a sliver of soft tissue.
fn blade() -> Body {
    Body {
        name: "blade",
        mass: Some(0.02),
        inertia: None,
        radius: Some(0.05),
        material: SOFT_TISSUE,
        volume: 0.02,
        thickness: 0.2,
        extent: Some(0.1),
    }
}

/// The face of `recipient` that `projectile` loads.
fn face(projectile: &Body, recipient: &Body) -> Face {
    let contact = geometry(projectile, recipient);
    Face::new(contact.width, contact.b)
}

/// The tank's armor plate facing a blast or a beam across `width`.
fn plate_face(width: f32) -> Face {
    Face::new(width, tank().section(width))
}

fn projectile() -> Projectile {
    let shell = shell();
    Projectile {
        region: shell.region(),
        mass: shell.mass.unwrap(),
        speed: SPEED,
    }
}

fn spec(id: u64) -> SourceSpec {
    SourceSpec {
        id,
        owner: Some(OWNER),
        projectile: None,
        payload: None,
        beam: None,
    }
}

fn shot() -> Source {
    Source::new(SourceSpec {
        projectile: Some(projectile()),
        ..spec(1)
    })
    .unwrap()
}

fn charge() -> Source {
    Source::new(SourceSpec {
        payload: Some(PAYLOAD),
        ..spec(2)
    })
    .unwrap()
}

fn beam(energy: f32, count: u32) -> Source {
    Source::new(SourceSpec {
        beam: Some(Pulses { energy, count }),
        ..spec(3)
    })
    .unwrap()
}

fn everything(id: u64, pulses: u32) -> Source {
    Source::new(SourceSpec {
        projectile: Some(projectile()),
        payload: Some(PAYLOAD),
        beam: Some(Pulses {
            energy: 10.0,
            count: pulses,
        }),
        ..spec(id)
    })
    .unwrap()
}

/// The shell's hard hit on the tank's plate at `speed`.
fn hard_hit(speed: f32) -> (ContactGeometry, Impact) {
    (
        geometry(&shell(), &tank()),
        head_on(&shell(), &tank(), speed),
    )
}

/// The work a pass left in its recipient.
fn kept(outcome: &Outcome) -> f64 {
    f64::from(outcome.ledger.irreversible[0]) + f64::from(outcome.ledger.dissipated[0])
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1.0e-9 * b.abs().max(1.0)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1.0e-4 * b.abs().max(1.0e-6)
}

#[test]
fn grass_takes_a_bounded_share_and_leaves_the_hard_hit_unchanged() {
    let blade = blade();
    let face = face(&shell(), &blade);
    let capacity = f64::from(blade.region().capacity());
    // A blade gets what a collision with its own mass makes available, not a
    // copy of the shot's energy.
    let collision = Impact {
        closing_speed: SPEED,
        effective_mass: effective_mass(
            ContactBody::translating(projectile().mass),
            ContactBody::translating(blade.region().mass()),
        )
        .unwrap(),
    }
    .energy();
    let mut source = shot();
    // The design note's figures: the shell carries 22,500, and a blade sees
    // 865 of it and keeps 5.5, its capacity of 1.2 and 4.3 damped.
    let budget = source.ledger().kinetic.budget;
    assert_eq!(budget, 22_500.0);
    assert!((collision - 865.0).abs() < 0.5, "{collision}");
    assert!((capacity - 1.2).abs() < 1.0e-6, "{capacity}");
    let mut kept_total = 0.0;
    for key in 0..40 {
        let outcome = source.pass(key, &blade.region(), face, SPEED).unwrap();
        assert_eq!(outcome.ledger.available, collision);
        // The shot cuts the blade, which takes its whole capacity and no more.
        assert!(outcome.failed[0]);
        assert_eq!(f64::from(outcome.consumed[0]), capacity);
        // What the blade could not take stays with the shot.
        assert_eq!(outcome.ledger.excess, 0.0);
        assert!(outcome.ledger.failure_motion > 0.0);
        assert!(outcome.ledger.closes(LEDGER_TOLERANCE));
        assert!((kept(&outcome) - 5.5).abs() < 0.05);
        // Half the budget would take more than 2,000 blades.
        assert!(0.5 * budget / kept(&outcome) > 2000.0);
        kept_total += kept(&outcome);
    }
    let ledger = source.ledger().kinetic;
    assert_eq!(ledger.budget, projectile().energy());
    assert_eq!(ledger.delivered, kept_total);
    assert!(near(ledger.absorbed, 40.0 * capacity));
    assert!(ledger.delivered < ledger.budget);

    // The shot flies on unchanged, so its hard hit is the ordinary one.
    assert_eq!(source.projectile(), Some(&projectile()));
    let (contact, load) = hard_hit(SPEED);
    let outcome = source.hit(100, &tank().region(), contact, load).unwrap();
    assert_eq!(
        outcome,
        impact(&shell().region(), &tank().region(), contact, load).unwrap()
    );
    let ledger = source.ledger().kinetic;
    assert_eq!(ledger.delivered, kept_total + f64::from(load.energy()));
    assert!(near(
        ledger.absorbed,
        40.0 * capacity + f64::from(outcome.consumed[1])
    ));
    // The shell is spent on the plate, which is not. The work the shell could
    // not take is the part's excess too.
    assert!(outcome.failed[0] && !outcome.failed[1]);
    assert!(outcome.ledger.excess > 0.0);
    assert_eq!(ledger.excess, f64::from(outcome.ledger.excess));
    // The shell keeps the damage its hit did.
    let mut struck = shell().region();
    struck.absorb(outcome.consumed[0]).unwrap();
    assert_eq!(source.projectile().unwrap().region, struck);
}

#[test]
fn a_crowd_cannot_draw_more_than_the_kinetic_budget() {
    let blade = blade();
    let face = face(&shell(), &blade);
    let mut source = shot();
    let budget = source.ledger().kinetic.budget;
    for key in 0..10_000 {
        let left = budget - source.ledger().kinetic.delivered;
        let outcome = source.pass(key, &blade.region(), face, SPEED).unwrap();
        // No pass gets more than the budget has left.
        assert!(f64::from(outcome.ledger.available) <= left);
        assert!(source.ledger().kinetic.delivered <= budget);
    }
    // The passes have spent the budget, so the last blades got next to nothing.
    assert!(source.ledger().kinetic.delivered > 0.999 * budget);
    let outcome = source.pass(10_000, &blade.region(), face, SPEED).unwrap();
    assert!(f64::from(outcome.ledger.available) < 1.0e-3 * budget);
    // The shot still makes its ordinary hard hit.
    let (contact, load) = hard_hit(SPEED);
    let outcome = source.hit(10_001, &tank().region(), contact, load).unwrap();
    assert_eq!(outcome.ledger.available, load.energy());
}

#[test]
fn brittle_twigs_take_the_budget_and_no_more() {
    // Dead twigs are brittle: a yielding twig keeps all the work it is
    // charged until the shot cuts it, so the last twig keeps its whole share.
    // At this slow shot, what the budget has left for that share would round
    // up in `f32`.
    let speed = 24.386;
    let twig = Body {
        name: "twig",
        material: WOOD,
        mass: Some(0.012),
        ..blade()
    };
    let face = face(&shell(), &twig);
    let mut source = Source::new(SourceSpec {
        projectile: Some(Projectile {
            speed,
            ..projectile()
        }),
        ..spec(1)
    })
    .unwrap();
    let budget = source.ledger().kinetic.budget;
    for key in 0..200 {
        let left = budget - source.ledger().kinetic.delivered;
        let outcome = source.pass(key, &twig.region(), face, speed).unwrap();
        assert!(f64::from(outcome.consumed[0]) <= left);
        assert!(source.ledger().kinetic.delivered <= budget);
    }
    // The twigs took the budget, short of it by less than the last share's
    // rounding.
    let left = budget - source.ledger().kinetic.delivered;
    assert!((0.0..1.0e-6 * budget).contains(&left), "{left}");
    let outcome = source.pass(200, &twig.region(), face, speed).unwrap();
    assert!(f64::from(outcome.ledger.available) <= left);
}

#[test]
fn a_shot_reaches_each_recipient_once_and_hits_once() {
    let blade = blade();
    let face = face(&shell(), &blade);
    let (contact, load) = hard_hit(SPEED);
    let mut source = shot();
    source.pass(1, &blade.region(), face, SPEED).unwrap();
    // Another report of the same blade, such as an overlapping sweep.
    let before = source.clone();
    assert_eq!(
        source.pass(1, &blade.region(), face, SPEED),
        Err(SourceError::Duplicate(1))
    );
    assert_eq!(source, before);
    source.hit(2, &tank().region(), contact, load).unwrap();
    // A detonating shell's direct-hit and generic contact callbacks both
    // report its hit, and only the first spends the kinetic part.
    let before = source.clone();
    for key in [2, 3] {
        assert_eq!(
            source.hit(key, &tank().region(), contact, load),
            Err(SourceError::Spent(Part::Kinetic))
        );
    }
    assert_eq!(
        source.pass(4, &blade.region(), face, SPEED),
        Err(SourceError::Spent(Part::Kinetic))
    );
    assert_eq!(source, before);
}

#[test]
fn a_payload_releases_once_and_a_crowd_divides_it() {
    let plate = tank().region();
    let face = plate_face(1.0);
    // One recipient takes 30% of the blast, through the deposit law.
    let mut alone = charge();
    let outcomes = alone
        .release(&[Share {
            key: 1,
            region: &plate,
            face,
            coupling: 0.3,
        }])
        .unwrap();
    assert_eq!(
        outcomes,
        vec![deposit(&plate, face, PAYLOAD * 0.3).unwrap()]
    );
    assert_eq!(alone.ledger().payload.budget, f64::from(PAYLOAD));
    assert_eq!(alone.ledger().payload.delivered, f64::from(PAYLOAD * 0.3));
    // Ten recipients sharing the same 30% get a tenth of it each.
    let mut crowd = charge();
    let plates = [plate; 10];
    let shares: Vec<Share> = plates
        .iter()
        .enumerate()
        .map(|(key, region)| Share {
            key: key as u64,
            region,
            face,
            coupling: 0.03,
        })
        .collect();
    let outcomes = crowd.release(&shares).unwrap();
    assert_eq!(outcomes.len(), 10);
    for outcome in &outcomes {
        assert_eq!(*outcome, deposit(&plate, face, PAYLOAD * 0.03).unwrap());
    }
    assert!(near(
        crowd.ledger().payload.delivered,
        alone.ledger().payload.delivered
    ));
    let absorbed = outcomes
        .iter()
        .map(|outcome| f64::from(outcome.consumed[0]))
        .sum::<f64>();
    assert!(near(crowd.ledger().payload.absorbed, absorbed));
    // A source releases once.
    let before = crowd.clone();
    assert_eq!(
        crowd.release(&shares),
        Err(SourceError::Spent(Part::Payload))
    );
    assert_eq!(crowd, before);
}

#[test]
fn release_couplings_cannot_multiply_the_payload() {
    let plate = tank().region();
    let face = plate_face(1.0);
    let share = |key, coupling| Share {
        key,
        region: &plate,
        face,
        coupling,
    };
    let mut source = charge();
    let before = source.clone();
    assert_eq!(
        source.release(&[share(1, 0.6), share(2, 0.6)]),
        Err(SourceError::Coupling)
    );
    for coupling in [-0.1, 1.5, f32::NAN] {
        assert_eq!(
            source.release(&[share(1, coupling)]),
            Err(SourceError::Coupling)
        );
    }
    // One recipient named twice, such as a body's two colliders.
    assert_eq!(
        source.release(&[share(1, 0.2), share(1, 0.2)]),
        Err(SourceError::Duplicate(1))
    );
    assert_eq!(source, before);
    // Couplings a hair past one for rounding still deliver no more than the
    // payload.
    source
        .release(&[share(1, 0.5), share(2, 0.500_000_9)])
        .unwrap();
    assert_eq!(source.ledger().payload.delivered, f64::from(PAYLOAD));
    // Couplings under one whose shares would round past the payload in `f32`.
    let mut source = charge();
    source
        .release(&[share(1, 0.090_713_02), share(2, 0.909_287)])
        .unwrap();
    assert!(source.ledger().payload.delivered <= f64::from(PAYLOAD));
}

#[test]
fn beam_pulses_never_carry_more_than_their_energy() {
    let blade = blade().region();
    let blade_face = Face::new(0.2, self::blade().section(0.2));
    let plate = tank().region();
    // Two blades in the beam's path each take a tenth of what reaches them,
    // and the plate where the beam ends takes the rest.
    let path = [
        Share {
            key: 1,
            region: &blade,
            face: blade_face,
            coupling: 0.1,
        },
        Share {
            key: 2,
            region: &blade,
            face: blade_face,
            coupling: 0.09,
        },
        Share {
            key: 3,
            region: &plate,
            face: plate_face(0.2),
            coupling: 0.81,
        },
    ];
    let mut source = beam(100.0, 3);
    assert_eq!(source.ledger().beam.budget, 300.0);
    let outcomes = source.pulse(&path).unwrap();
    let energies: Vec<f32> = outcomes
        .iter()
        .map(|outcome| outcome.ledger.available)
        .collect();
    assert_eq!(energies, [10.0, 9.0, 81.0]);
    for (share, outcome) in path.iter().zip(&outcomes) {
        assert_eq!(
            *outcome,
            deposit(share.region, share.face, outcome.ledger.available).unwrap()
        );
    }
    assert_eq!(source.ledger().beam.delivered, 100.0);
    // Without the plate, what the blades leave goes on.
    source.pulse(&path[..2]).unwrap();
    assert_eq!(source.ledger().beam.delivered, 119.0);
    let before = source.clone();
    assert_eq!(
        source.pulse(&[path[0], path[0]]),
        Err(SourceError::Duplicate(1))
    );
    let over = Share {
        coupling: 0.9,
        ..path[2]
    };
    assert_eq!(
        source.pulse(&[path[0], path[1], over]),
        Err(SourceError::Coupling)
    );
    assert_eq!(source, before);
    source.pulse(&[]).unwrap();
    assert_eq!(source.pulses_left(), 0);
    let before = source.clone();
    assert_eq!(source.pulse(&path), Err(SourceError::Spent(Part::Beam)));
    assert_eq!(source, before);
}

#[test]
fn exhausted_recipients_refuse_work_and_the_ledger_keeps_it() {
    let face = plate_face(1.0);
    let worn = Region::with_remaining(ARMOR, tank().volume, 1.0).unwrap();
    let failed = Region::with_remaining(ARMOR, tank().volume, 0.0).unwrap();
    let mut source = charge();
    let outcomes = source
        .release(&[
            Share {
                key: 1,
                region: &worn,
                face,
                coupling: 0.5,
            },
            Share {
                key: 2,
                region: &failed,
                face,
                coupling: 0.5,
            },
        ])
        .unwrap();
    // The worn plate takes what it has left and fails.
    assert_eq!(outcomes[0].consumed[0], 1.0);
    assert!(outcomes[0].failed[0]);
    // The failed plate takes nothing more.
    assert_eq!(outcomes[1].consumed[0], 0.0);
    assert!(!outcomes[1].failed[0]);
    // What they could not take is the host's to resolve, and it is accounted.
    let ledger = source.ledger().payload;
    assert_eq!(ledger.delivered, f64::from(PAYLOAD));
    assert_eq!(ledger.absorbed, 1.0);
    let excess = outcomes
        .iter()
        .map(|outcome| f64::from(outcome.ledger.excess))
        .sum::<f64>();
    assert!(excess > 0.0);
    assert!(near(ledger.excess, excess));
    for outcome in &outcomes {
        assert!(outcome.ledger.closes(LEDGER_TOLERANCE));
    }
}

#[test]
fn a_retired_source_delivers_nothing_and_keeps_its_attribution() {
    let blade = blade();
    let face = face(&shell(), &blade);
    let (contact, load) = hard_hit(SPEED);
    let mut source = everything(9, 4);
    source.pass(1, &blade.region(), face, SPEED).unwrap();
    let ledger = source.ledger();
    // The firing actor's death does not touch the source; the host retires it
    // when the shot or charge ends.
    source.retire();
    assert!(source.spent());
    assert_eq!(source.id(), 9);
    assert_eq!(source.owner(), Some(OWNER));
    let before = source.clone();
    assert_eq!(
        source.pass(2, &blade.region(), face, SPEED),
        Err(SourceError::Retired)
    );
    assert_eq!(
        source.hit(3, &tank().region(), contact, load),
        Err(SourceError::Retired)
    );
    assert_eq!(source.release(&[]), Err(SourceError::Retired));
    assert_eq!(source.pulse(&[]), Err(SourceError::Retired));
    assert_eq!(source, before);
    assert_eq!(source.ledger(), ledger);
}

#[test]
fn a_source_is_spent_once_every_part_is() {
    let (contact, load) = hard_hit(SPEED);
    let mut source = everything(4, 2);
    assert!(!source.spent());
    source.hit(1, &tank().region(), contact, load).unwrap();
    assert!(!source.spent());
    source.release(&[]).unwrap();
    assert!(!source.spent());
    source.pulse(&[]).unwrap();
    assert!(!source.spent());
    assert_eq!(source.pulses_left(), 1);
    source.pulse(&[]).unwrap();
    assert!(source.spent());

    // A source without a part has none of it to spend.
    let mut empty = Source::new(spec(5)).unwrap();
    assert!(empty.spent());
    let blade = blade();
    assert_eq!(
        empty.pass(1, &blade.region(), face(&shell(), &blade), SPEED),
        Err(SourceError::Spent(Part::Kinetic))
    );
    assert_eq!(empty.release(&[]), Err(SourceError::Spent(Part::Payload)));
    assert_eq!(empty.pulse(&[]), Err(SourceError::Spent(Part::Beam)));
}

#[test]
fn invalid_specs_and_loads_are_refused_and_change_nothing() {
    let invalid = SourceError::Material(MaterialError::InvalidLoad);
    let geometry_error = SourceError::Material(MaterialError::InvalidGeometry);
    let fire = |projectile| {
        Source::new(SourceSpec {
            projectile: Some(projectile),
            ..spec(6)
        })
    };
    for projectile in [
        Projectile {
            mass: 0.0,
            ..projectile()
        },
        Projectile {
            speed: -1.0,
            ..projectile()
        },
        Projectile {
            speed: f32::NAN,
            ..projectile()
        },
        Projectile {
            mass: f32::INFINITY,
            ..projectile()
        },
    ] {
        assert_eq!(fire(projectile), Err(invalid));
    }
    // The shell's region outweighs it.
    assert_eq!(
        fire(Projectile {
            mass: 0.4,
            ..projectile()
        }),
        Err(SourceError::Material(MaterialError::InvalidRegion))
    );
    // A region sized from the projectile's mass may round a hair over it.
    let mass = projectile().region.mass().next_down();
    assert!(
        fire(Projectile {
            mass,
            ..projectile()
        })
        .is_ok()
    );
    for payload in [-1.0, f32::NAN, f32::INFINITY] {
        let spec = SourceSpec {
            payload: Some(payload),
            ..spec(6)
        };
        assert_eq!(Source::new(spec), Err(invalid));
    }
    for beam in [
        Pulses {
            energy: -1.0,
            count: 1,
        },
        Pulses {
            energy: f32::NAN,
            count: 1,
        },
        Pulses {
            energy: f32::INFINITY,
            count: 1,
        },
        Pulses {
            energy: 1.0,
            count: 0,
        },
    ] {
        let spec = SourceSpec {
            beam: Some(beam),
            ..spec(6)
        };
        assert_eq!(Source::new(spec), Err(invalid));
    }

    let blade = blade();
    let blade_face = face(&shell(), &blade);
    let (contact, load) = hard_hit(SPEED);
    let plate = tank().region();
    let shares = [
        Share {
            key: 1,
            region: &plate,
            face: plate_face(1.0),
            coupling: 0.5,
        },
        Share {
            key: 2,
            region: &plate,
            face: Face::new(-1.0, tank().section(1.0)),
            coupling: 0.5,
        },
    ];
    let mut source = everything(6, 1);
    let before = source.clone();
    assert_eq!(
        source.pass(1, &blade.region(), blade_face, f32::NAN),
        Err(invalid)
    );
    assert_eq!(
        source.pass(1, &blade.region(), shares[1].face, SPEED),
        Err(geometry_error)
    );
    let backward = Impact {
        closing_speed: -1.0,
        ..load
    };
    assert_eq!(
        source.hit(2, &tank().region(), contact, backward),
        Err(invalid)
    );
    // An invalid last share refuses the whole release or pulse.
    assert_eq!(source.release(&shares), Err(geometry_error));
    assert_eq!(source.pulse(&shares), Err(geometry_error));
    assert_eq!(source, before);
    // Every part is still there to deliver.
    source.pass(1, &blade.region(), blade_face, SPEED).unwrap();
    source.hit(2, &tank().region(), contact, load).unwrap();
    source.release(&shares[..1]).unwrap();
    source.pulse(&shares[..1]).unwrap();
    assert!(source.spent());
}

#[test]
fn a_deposit_is_an_impact_on_a_rigid_backing() {
    let plate = tank().region();
    let face = plate_face(1.0);
    // The safe limit is the material's resilience over the loaded volume. The
    // design note's figures: a face 1 wu wide, 1 wu thick and 0.2 wu deep is
    // safe to 20, and a share of 1500 charges it 1465 of its 200,000.
    let limit = ARMOR.resilience() * face.area() * face.section.depth;
    assert_eq!(face.section, Section::new(1.0, 0.2));
    assert!((limit - 20.0).abs() < 1.0e-4, "{limit}");
    assert_eq!(plate.capacity(), 200_000.0);
    let share = deposit(&plate, face, 1500.0).unwrap().consumed[0];
    assert!((share - 1465.0).abs() < 0.5, "{share}");
    let below = deposit(&plate, face, 0.999 * limit).unwrap();
    assert!(!below.yielded[0]);
    assert_eq!(below.consumed[0], 0.0);
    assert!(close(
        below.ledger.recoverable[0] + below.ledger.dissipated[0],
        0.999 * limit
    ));
    let above = deposit(&plate, face, 1.001 * limit).unwrap();
    assert!(above.yielded[0]);
    assert!(above.consumed[0] > 0.0);

    // The same energy delivered by an impact against a rigid, unbreakable
    // anvil loads the plate the same way.
    let anvil = Region::new(
        Material {
            name: "anvil",
            density: 1.0,
            stiffness: 1.0e12,
            strength: 1.0e12,
            toughness: 1.0e15,
            damping: 0.0,
            mode: FailureMode::Ductile,
        },
        1.0,
    )
    .unwrap();
    let contact = ContactGeometry::new(
        face.width,
        Section::new(face.section.thickness, 1.0),
        face.section,
    );
    for energy in [0.5 * limit, 10.0 * limit, 1000.0 * limit, 1.0e5] {
        let one = deposit(&plate, face, energy).unwrap();
        let load = Impact {
            closing_speed: (2.0 * energy).sqrt(),
            effective_mass: 1.0,
        };
        let two = impact(&anvil, &plate, contact, load).unwrap();
        assert_eq!(one.yielded[0], two.yielded[1], "{energy}");
        assert!(!two.yielded[0]);
        assert!(
            close(one.ledger.peak_force, two.ledger.peak_force),
            "{energy}"
        );
        assert!(close(one.consumed[0], two.consumed[1]), "{energy}");
        assert!(
            close(one.ledger.recoverable[0], two.ledger.recoverable[1]),
            "{energy}"
        );
        assert!(
            close(one.ledger.dissipated[0], two.ledger.dissipated[1]),
            "{energy}"
        );
        assert!(one.ledger.closes(LEDGER_TOLERANCE));
    }
    for energy in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            deposit(&plate, face, energy),
            Err(MaterialError::InvalidLoad)
        );
    }
    assert_eq!(
        deposit(&plate, Face::new(0.0, face.section), 1.0),
        Err(MaterialError::InvalidGeometry)
    );
}
