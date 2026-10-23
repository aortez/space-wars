use crate::fixtures::{
    self, Body, GRAVITY, TICK, bend, geometry, head_on, press, safe_limit_speed, strike,
};
use crate::*;
use engine_core::Vec2;

fn approx(actual: f32, expected: f32, relative: f32) -> bool {
    (actual - expected).abs() <= relative * actual.abs().max(expected.abs()).max(1.0e-6)
}

fn fresh(a: &Body, b: &Body) -> (Region, Region) {
    (a.region(), b.region())
}

#[test]
fn reference_materials_validate_and_have_distinct_identities() {
    let mut identities = Vec::new();
    for material in ALL_REFERENCE {
        material.validate().unwrap();
        assert_eq!(material.identity_hex().len(), 16);
        identities.push(material.identity());
        let ratio = material.toughness / material.resilience();
        match material.mode {
            FailureMode::Brittle => assert!(ratio < 4.0, "{} is brittle", material.name),
            FailureMode::Ductile => assert!(ratio > 4.0, "{} is ductile", material.name),
        }
        assert!(approx(
            material.yield_strain() * material.stiffness,
            material.strength,
            1.0e-6
        ));
    }
    identities.sort_unstable();
    identities.dedup();
    assert_eq!(identities.len(), ALL_REFERENCE.len());
    // The name is a label, not part of the identity.
    assert_eq!(
        Material {
            name: "renamed",
            ..WOOD
        }
        .identity(),
        WOOD.identity()
    );
    assert_ne!(
        Material {
            strength: 3.1e3,
            ..WOOD
        }
        .identity(),
        WOOD.identity()
    );
}

#[test]
fn reference_bodies_hold_no_more_material_than_they_weigh() {
    for body in [
        fixtures::tank(),
        fixtures::ship(),
        fixtures::spaceling(),
        fixtures::branch(),
        fixtures::boulder(),
        fixtures::stone(),
        fixtures::shell(),
    ] {
        let (mass, material) = (body.mass.unwrap(), body.region().mass());
        assert!(
            material <= mass * (1.0 + 1.0e-6),
            "{} holds {material} mu of material in {mass} mu",
            body.name
        );
    }
}

#[test]
fn invalid_inputs_are_rejected() {
    let invalid = Err(MaterialError::InvalidMaterial);
    assert_eq!(
        Material {
            strength: 1.0e7,
            ..ARMOR
        }
        .validate(),
        invalid
    );
    assert_eq!(
        Material {
            toughness: f32::NAN,
            ..WOOD
        }
        .validate(),
        invalid
    );
    assert_eq!(
        Material {
            damping: 1.01,
            ..WOOD
        }
        .validate(),
        invalid
    );
    assert_eq!(
        Material {
            strength: -0.1,
            ..WOOD
        }
        .validate(),
        invalid
    );
    assert_eq!(Region::new(WOOD, 0.0), Err(MaterialError::InvalidRegion));
    assert_eq!(
        Region::new(WOOD, f32::INFINITY),
        Err(MaterialError::InvalidRegion)
    );
    assert_eq!(
        Impact {
            closing_speed: -1.0,
            effective_mass: 1.0
        }
        .validate(),
        Err(MaterialError::InvalidLoad)
    );
    assert_eq!(
        effective_mass(ContactBody::IMMOVABLE, ContactBody::IMMOVABLE),
        Err(MaterialError::InvalidLoad)
    );
    assert_eq!(
        effective_mass(ContactBody::translating(-1.0), ContactBody::IMMOVABLE),
        Err(MaterialError::InvalidLoad)
    );
    assert_eq!(
        Section::new(0.0, 1.0).validate(),
        Err(MaterialError::InvalidGeometry)
    );
    assert_eq!(
        Beam::new(1.0, 1.0, f32::NAN).validate(),
        Err(MaterialError::InvalidGeometry)
    );
    assert_eq!(
        patch_width_from_radii(0.0, None),
        Err(MaterialError::InvalidGeometry)
    );
    assert_eq!(
        ContactGeometry::merge(&[]),
        Err(MaterialError::InvalidGeometry)
    );
    let largest = ContactGeometry::new(1.0e6, Section::new(1.0, 1.0), Section::new(1.0, 1.0));
    assert_eq!(
        ContactGeometry::merge(&[largest; 2]),
        Err(MaterialError::InvalidGeometry)
    );
    let region = Region::new(WOOD, 1.0).unwrap();
    let load = SustainedLoad {
        force: -5.0,
        width: 1.0,
        section: Section::new(1.0, 1.0),
        duration: TICK,
        measured_plastic_displacement: None,
    };
    assert_eq!(sustained(&region, load), Err(MaterialError::InvalidLoad));
    let joint = Connection::new(WOOD, Beam::new(1.0, 1.0, 1.0)).unwrap();
    let bad = ConnectionLoad {
        moment: 1.0,
        axial: 0.0,
        duration: -1.0,
        measured_plastic_rotation: None,
        measured_plastic_extension: None,
    };
    assert_eq!(connection(&joint, bad), Err(MaterialError::InvalidLoad));
    assert!(!MaterialError::InvalidLoad.to_string().is_empty());
}

#[test]
fn safe_bumps_never_consume() {
    let pairs = [
        (fixtures::branch(), fixtures::tank()),
        (fixtures::stone(), fixtures::ship()),
        (fixtures::spaceling(), fixtures::branch()),
        (fixtures::shell(), fixtures::tank()),
        (fixtures::ship(), fixtures::planet()),
    ];
    for (a, b) in &pairs {
        let (mut region_a, mut region_b) = fresh(a, b);
        let geometry = geometry(a, b);
        let load = head_on(a, b, 0.5 * safe_limit_speed(a, b));
        for _ in 0..1000 {
            let outcome = strike(&mut region_a, &mut region_b, geometry, load);
            assert_eq!(outcome.consumed, [0.0, 0.0], "{} vs {}", a.name, b.name);
            assert!(!outcome.yielded[0] && !outcome.yielded[1]);
            assert!(outcome.ledger.closes(LEDGER_TOLERANCE));
            // Everything available is stored or damped; nothing is permanent.
            let elastic: f32 = outcome
                .ledger
                .recoverable
                .iter()
                .chain(&outcome.ledger.dissipated)
                .sum();
            assert!(approx(elastic, outcome.ledger.available, 1.0e-5));
            assert_eq!(outcome.ledger.excess, 0.0);
        }
        assert_eq!(region_a.integrity(), 1.0);
        assert_eq!(region_b.integrity(), 1.0);
    }
}

#[test]
fn the_safe_limit_speed_separates_recoverable_from_damaging_hits() {
    let (a, b) = (fixtures::shell(), fixtures::tank());
    let limit = safe_limit_speed(&a, &b);
    let geometry = geometry(&a, &b);
    let (region_a, region_b) = fresh(&a, &b);
    let mass = head_on(&a, &b, 0.0).effective_mass;
    let below = impact(
        &region_a,
        &region_b,
        geometry,
        Impact {
            closing_speed: limit * 0.999,
            effective_mass: mass,
        },
    )
    .unwrap();
    let above = impact(
        &region_a,
        &region_b,
        geometry,
        Impact {
            closing_speed: limit * 1.001,
            effective_mass: mass,
        },
    )
    .unwrap();
    assert!(below.safe());
    assert!(!above.safe());
    assert!(
        above.yielded[1] && !above.yielded[0],
        "the plate is the weaker side"
    );
    // Zero energy is a valid, empty event.
    let still = impact(
        &region_a,
        &region_b,
        geometry,
        Impact {
            closing_speed: 0.0,
            effective_mass: mass,
        },
    )
    .unwrap();
    assert_eq!(still, Outcome::default());
}

#[test]
fn progressive_hits_fail_after_the_predicted_count() {
    // Every fresh projectile charges the same work until capacity runs out.
    let a = fixtures::shell();
    let b = fixtures::tank();
    let geometry = geometry(&a, &b);
    let load = head_on(&a, &b, 300.0);
    let (mut region_a, mut region_b) = fresh(&a, &b);
    let first = strike(&mut region_a, &mut region_b, geometry, load);
    let per_hit = first.consumed[1];
    assert!(per_hit > 0.0);
    assert!(first.consumed[0] > 0.0);
    assert!(!first.failed[1]);
    let predicted = (region_b.capacity() / per_hit).ceil() as usize;
    let mut hits = 1;
    let mut previous = region_b.integrity();
    while !region_b.failed() {
        region_a = a.region();
        let outcome = strike(&mut region_a, &mut region_b, geometry, load);
        hits += 1;
        assert!(region_b.integrity() < previous);
        previous = region_b.integrity();
        assert!(outcome.ledger.closes(LEDGER_TOLERANCE));
        // Only the hit that exhausts the plate reports its failure.
        assert_eq!(outcome.failed[1], region_b.failed());
        assert!(hits <= predicted + 1, "runaway");
    }
    assert_eq!(hits, predicted);
    assert!(
        region_a.failed(),
        "each fresh shell spends its own finite capacity"
    );
}

#[test]
fn accumulated_damage_does_not_change_strength_or_safe_contact_threshold() {
    let (a, b) = (fixtures::shell(), fixtures::tank());
    let contact = geometry(&a, &b);
    let safe = head_on(&a, &b, 0.5 * safe_limit_speed(&a, &b));
    let damaging = head_on(&a, &b, 10.0);
    let baseline = impact(&a.region(), &b.region(), contact, damaging).unwrap();
    for fraction in [1.0, 0.5, 0.01] {
        let mut target =
            Region::with_remaining(ARMOR, b.volume, b.region().capacity() * fraction).unwrap();
        assert_eq!(target.effective_strength(), ARMOR.strength);
        for _ in 0..1000 {
            assert_eq!(
                strike(&mut a.region(), &mut target, contact, safe).consumed,
                [0.0; 2]
            );
        }
        let hit = impact(&a.region(), &target, contact, damaging).unwrap();
        assert_eq!(hit.ledger.peak_force, baseline.ledger.peak_force);
        assert_eq!(hit.consumed, baseline.consumed);
    }
}

#[test]
fn doubling_volume_doubles_hits_to_failure() {
    let a = fixtures::shell();
    let single = fixtures::tank();
    let double = single.with_volume(2.0 * single.volume);
    let (hits_single, ..) = fixtures::hits_to_failure(&a, &single, 300.0, 1000);
    let (hits_double, ..) = fixtures::hits_to_failure(&a, &double, 300.0, 1000);
    // ceil(2x) is 2 ceil(x) or 2 ceil(x) - 1.
    assert!(hits_double == 2 * hits_single || hits_double == 2 * hits_single - 1);
}

#[test]
fn doubling_thickness_can_make_a_hit_safe() {
    let a = fixtures::shell();
    let thin = fixtures::tank();
    let thick = fixtures::tank().with_thickness(2.0);
    let speed = 1.2 * safe_limit_speed(&a, &thin);
    let (region_a, region_b) = fresh(&a, &thin);
    let dented = impact(
        &region_a,
        &region_b,
        geometry(&a, &thin),
        head_on(&a, &thin, speed),
    )
    .unwrap();
    assert!(!dented.safe());
    let (region_a, region_b) = fresh(&a, &thick);
    let bounced = impact(
        &region_a,
        &region_b,
        geometry(&a, &thick),
        head_on(&a, &thick, speed),
    )
    .unwrap();
    assert!(bounced.safe());
    // Twice the area halves the peak stress for the same force.
    assert!(approx(
        geometry(&a, &thick).area(),
        2.0 * geometry(&a, &thin).area(),
        1.0e-6
    ));
}

#[test]
fn wood_breaks_on_armor_without_armor_loss() {
    let (a, b) = (fixtures::branch(), fixtures::tank());
    for speed in [15.0, 40.0] {
        let (mut region_a, mut region_b) = fresh(&a, &b);
        let outcome = strike(
            &mut region_a,
            &mut region_b,
            geometry(&a, &b),
            head_on(&a, &b, speed),
        );
        assert!(outcome.yielded[0] && !outcome.yielded[1]);
        assert_eq!(outcome.consumed[1], 0.0);
        assert_eq!(outcome.ledger.irreversible[1], 0.0);
        assert!(outcome.failed[0] && region_a.failed());
        assert!(
            outcome.ledger.excess > 0.0,
            "failure energy requires a host disposition"
        );
        assert!(outcome.ledger.closes(LEDGER_TOLERANCE));
        // Brittle wood does not spring back; the plate does.
        assert_eq!(outcome.ledger.recoverable[0], 0.0);
        assert!(outcome.ledger.recoverable[1] > 0.0);
    }
}

#[test]
fn soft_tissue_is_more_vulnerable_than_wood() {
    let stone = fixtures::stone();
    let loss = |target: &Body| {
        let (mut region_a, mut region_b) = fresh(&stone, target);
        let outcome = strike(
            &mut region_a,
            &mut region_b,
            geometry(&stone, target),
            head_on(&stone, target, 10.0),
        );
        assert!(
            outcome.yielded[1] && !outcome.yielded[0],
            "rock is the stronger side"
        );
        1.0 - region_b.integrity()
    };
    let tissue = loss(&fixtures::spaceling());
    let wood = loss(&fixtures::branch());
    assert!(tissue > wood && wood > 0.0, "tissue {tissue} wood {wood}");
}

#[test]
fn severe_armor_failure_is_finite() {
    let (a, b) = (fixtures::shell(), fixtures::tank());
    let (mut region_a, mut region_b) = fresh(&a, &b);
    let outcome = strike(
        &mut region_a,
        &mut region_b,
        geometry(&a, &b),
        head_on(&a, &b, 1800.0),
    );
    assert!(outcome.failed[1] && region_b.failed());
    assert!(outcome.yielded[0]);
    assert_eq!(outcome.consumed[0], a.region().capacity());
    assert!(outcome.ledger.excess > 0.0);
    assert!(outcome.ledger.finite());
    assert!(outcome.ledger.closes(LEDGER_TOLERANCE));
    assert_eq!(outcome.consumed[1], b.region().capacity());
    // A failed region keeps bouncing but cannot absorb anything more, and its
    // failure is not reported again.
    let again = impact(
        &region_a,
        &region_b,
        geometry(&a, &b),
        head_on(&a, &b, 300.0),
    )
    .unwrap();
    assert_eq!(again.consumed[1], 0.0);
    assert!(again.yielded[1] && !again.failed[1] && !again.safe());
    assert!(again.ledger.closes(LEDGER_TOLERANCE));
    // A branch snaps on the broken plate, which is still the stronger side.
    let branch = fixtures::branch();
    let snapped = impact(
        &branch.region(),
        &region_b,
        geometry(&branch, &b),
        head_on(&branch, &b, 40.0),
    )
    .unwrap();
    assert_eq!(snapped.failed, [true, false]);
}

#[test]
fn exhausted_regions_stay_finite_without_repeated_failure() {
    let (stone, branch) = (fixtures::stone(), fixtures::branch());
    let contact = geometry(&stone, &branch);
    let load = head_on(&stone, &branch, 30.0);
    let (mut striker, mut target) = fresh(&stone, &branch);
    assert!(strike(&mut striker, &mut target, contact, load).failed[1]);
    striker.absorb(striker.remaining()).unwrap();
    for _ in 0..3 {
        let again = strike(&mut striker, &mut target, contact, load);
        assert!(again.yielded[1] && again.failed == [false; 2] && again.consumed == [0.0; 2]);
        assert!(again.ledger.finite() && again.ledger.closes(LEDGER_TOLERANCE));
    }
    let pressed = sustained(
        &target,
        SustainedLoad {
            force: 4000.0,
            width: 1.0,
            section: branch.section(1.0),
            duration: TICK,
            measured_plastic_displacement: None,
        },
    )
    .unwrap();
    let mut joint = Connection::new(WOOD, Beam::new(1.0, 1.0, 2.0)).unwrap();
    joint.absorb(joint.region().capacity()).unwrap();
    let bent = connection(
        &joint,
        ConnectionLoad {
            moment: 600.0,
            axial: 50.0,
            duration: TICK,
            measured_plastic_rotation: None,
            measured_plastic_extension: None,
        },
    )
    .unwrap();
    for outcome in [pressed, bent] {
        assert!(outcome.yielded[0] && !outcome.failed[0] && outcome.consumed[0] == 0.0);
        assert!(outcome.ledger.finite() && outcome.ledger.closes(LEDGER_TOLERANCE));
    }
}

#[test]
fn wide_intermediates_stay_finite_and_overflowing_outputs_return_an_error() {
    let material = Material {
        stiffness: 1.0e12,
        strength: 1.0e12,
        ..ARMOR
    };
    let region = Region::new(material, 1.0e6).unwrap();
    let section = Section::new(1.0e6, 1.0e-6);
    let hit = impact(
        &region,
        &region,
        ContactGeometry::new(1.0e6, section, section),
        Impact {
            closing_speed: 1.0e6,
            effective_mass: 1.0e12,
        },
    )
    .unwrap();
    assert!(hit.ledger.finite() && hit.ledger.closes(LEDGER_TOLERANCE));
    assert_eq!(
        sustained(
            &Region::new(ARMOR, 1.0).unwrap(),
            SustainedLoad {
                force: 1.0e12,
                width: 1.0,
                section: Section::new(1.0, 1.0),
                duration: 1.0,
                measured_plastic_displacement: Some(f32::MAX),
            }
        ),
        Err(MaterialError::Overflow)
    );
}

#[test]
fn glancing_hits_do_strictly_less_damage() {
    let (a, b) = (fixtures::ship(), fixtures::planet());
    let speed = 20.0;
    let mass = head_on(&a, &b, 0.0).effective_mass;
    let mut previous = f32::INFINITY;
    for angle in [0.0_f32, 30.0, 60.0, 80.0] {
        let (mut region_a, mut region_b) = fresh(&a, &b);
        let outcome = strike(
            &mut region_a,
            &mut region_b,
            geometry(&a, &b),
            Impact {
                closing_speed: speed * libm::cosf(angle.to_radians()),
                effective_mass: mass,
            },
        );
        let damage = outcome.consumed[0] + outcome.consumed[1];
        assert!(
            damage > 0.0 && damage < previous,
            "angle {angle}: {damage} vs {previous}"
        );
        assert!(outcome.yielded[0], "the hull yields on rock");
        previous = damage;
    }
}

#[test]
fn off_center_hits_do_strictly_less_damage() {
    let (a, b) = (fixtures::boulder(), fixtures::tank());
    let speed = 3.0;
    let mut previous = f32::INFINITY;
    for lever_arm in [0.0, 1.0, 2.0, 3.0] {
        let striker = ContactBody::rotating(a.mass.unwrap(), a.inertia.unwrap(), lever_arm);
        let (mut region_a, mut region_b) = fresh(&a, &b);
        let outcome = strike(
            &mut region_a,
            &mut region_b,
            geometry(&a, &b),
            Impact {
                closing_speed: speed,
                effective_mass: effective_mass(striker, b.contact()).unwrap(),
            },
        );
        let damage = outcome.consumed[0];
        assert!(
            damage > 0.0 && damage < previous,
            "lever arm {lever_arm}: {damage} vs {previous}"
        );
        assert!(
            outcome.yielded[0] && !outcome.failed[0] && outcome.consumed[1] == 0.0,
            "the boulder chips on the plate without breaking"
        );
        previous = damage;
    }
}

#[test]
fn lever_arm_reduces_effective_mass_per_the_analytic_formula() {
    let tank = ContactBody::translating(60.0);
    let central = effective_mass(ContactBody::rotating(50.0, 225.0, 0.0), tank).unwrap();
    assert!(approx(central, 1.0 / (1.0 / 50.0 + 1.0 / 60.0), 1.0e-6));
    let mut previous = central;
    for lever_arm in [1.0, 2.0, 3.0] {
        let mass = effective_mass(ContactBody::rotating(50.0, 225.0, lever_arm), tank).unwrap();
        let expected = 1.0 / (1.0 / 50.0 + lever_arm * lever_arm / 225.0 + 1.0 / 60.0);
        assert!(approx(mass, expected, 1.0e-6));
        assert!(mass < previous);
        previous = mass;
    }
    // Against an immovable support only the striker counts.
    assert!(approx(
        effective_mass(ContactBody::translating(5.0), ContactBody::IMMOVABLE).unwrap(),
        5.0,
        1.0e-6
    ));
    // The lever helper is the 2D cross product.
    assert_eq!(lever(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0)), -2.0);
    assert_eq!(lever(Vec2::new(2.0, 0.0), Vec2::new(0.0, 1.0)), 2.0);
}

#[test]
fn closing_speed_ignores_common_velocity_and_separation() {
    let normal = Vec2::new(1.0, 0.0);
    let (velocity_a, velocity_b) = (Vec2::new(7.0, 1.0), Vec2::new(3.0, -2.0));
    let base = closing_speed(normal, velocity_a, velocity_b);
    assert_eq!(base, 4.0);
    let shift = Vec2::new(100.0, -50.0);
    assert_eq!(
        closing_speed(normal, velocity_a + shift, velocity_b + shift),
        base
    );
    assert_eq!(closing_speed(normal, velocity_b, velocity_a), 0.0);
    // Point velocity adds the spin term.
    let velocity = point_velocity(Vec2::new(1.0, 0.0), 2.0, Vec2::new(0.0, 1.0));
    assert_eq!((velocity.x, velocity.y), (-1.0, 0.0));
}

#[test]
fn deformation_depth_follows_the_patch_width_up_to_the_material() {
    assert_eq!(deformation_depth(0.5, None), Ok(0.5 * DEPTH_WIDTH_FRACTION));
    assert_eq!(deformation_depth(0.5, Some(0.2)), Ok(0.2));
    assert_eq!(
        deformation_depth(0.5, Some(4.0)),
        Ok(0.5 * DEPTH_WIDTH_FRACTION)
    );
    assert_eq!(
        deformation_depth(0.0, None),
        Err(MaterialError::InvalidGeometry)
    );
    assert_eq!(
        deformation_depth(0.5, Some(f32::NAN)),
        Err(MaterialError::InvalidGeometry)
    );
    // A ship's thin hull caps its depth; the planet below is a half-space.
    let contact = geometry(&fixtures::ship(), &fixtures::planet());
    assert_eq!(contact.a.depth, 0.1);
    assert_eq!(contact.b.depth, contact.width * DEPTH_WIDTH_FRACTION);
}

#[test]
fn available_energy_and_elastic_force_match_analytic_forms() {
    let (a, b) = (fixtures::shell(), fixtures::tank());
    let geometry = geometry(&a, &b);
    let (region_a, region_b) = fresh(&a, &b);
    let speed = 0.5 * safe_limit_speed(&a, &b);
    let load = head_on(&a, &b, speed);
    assert!(approx(
        load.energy(),
        0.5 * load.effective_mass * speed * speed,
        1.0e-6
    ));
    let outcome = impact(&region_a, &region_b, geometry, load).unwrap();
    assert!(outcome.safe());
    let area = geometry.area();
    let stiffness_a = geometry.a.contact_stiffness(SHELL.stiffness, area);
    let stiffness_b = geometry.b.contact_stiffness(ARMOR.stiffness, area);
    let series = 1.0 / (1.0 / stiffness_a + 1.0 / stiffness_b);
    assert!(approx(
        outcome.ledger.peak_force,
        (2.0 * load.energy() * series).sqrt(),
        1.0e-5
    ));
    assert!(approx(
        outcome.ledger.peak_stress[1],
        outcome.ledger.peak_force / area,
        1.0e-6
    ));
    assert!(approx(
        outcome.ledger.deformation[1],
        outcome.ledger.peak_force / stiffness_b,
        1.0e-6
    ));
    assert!(approx(
        outcome.ledger.dissipated[1],
        ARMOR.damping * 0.5 * outcome.ledger.peak_force * outcome.ledger.peak_force / stiffness_b,
        1.0e-5
    ));
    let limit = safe_limit_speed(&a, &b);
    assert!(approx(
        limit,
        ARMOR.strength * area / (load.effective_mass * series).sqrt(),
        1.0e-5
    ));
}

#[test]
fn restitution_returns_exactly_the_recoverable_energy() {
    let (a, b) = (fixtures::shell(), fixtures::tank());
    let geometry = geometry(&a, &b);
    let (region_a, region_b) = fresh(&a, &b);
    let mut restitution = Vec::new();
    for speed in [0.5 * safe_limit_speed(&a, &b), 300.0] {
        let load = head_on(&a, &b, speed);
        let mut ledger = impact(&region_a, &region_b, geometry, load).unwrap().ledger;
        ledger.resolve_excess(0.0).unwrap();
        let coefficient = ledger.restitution().unwrap();
        // The rebound the host sets returns the recoverable energy, no more.
        let rebound = 0.5 * load.effective_mass * (coefficient * speed).powi(2);
        assert!(approx(
            rebound,
            ledger.recoverable.iter().sum::<f32>(),
            1.0e-4
        ));
        restitution.push(coefficient);
    }
    // An elastic knock bounces back; a dent hardly does.
    assert!(
        restitution[0] > 0.9 && restitution[1] < 0.1,
        "{restitution:?}"
    );
    assert_eq!(WorkLedger::default().restitution(), Ok(0.0));
}

#[test]
fn every_reference_row_closes_its_ledger() {
    let report = fixtures::report();
    assert_eq!(report.law_version, LAW_VERSION);
    for case in report.cases.iter().chain(&report.sweeps) {
        assert!(!case.rows.is_empty(), "{}", case.name);
        for row in &case.rows {
            assert!(row.closes, "{}: {}", case.name, row.label);
            let accounted = row
                .recoverable
                .iter()
                .chain(&row.dissipated)
                .chain(&row.consumed)
                .sum::<f32>()
                + row.excess
                + row.failure_motion
                + row.failure_dissipated;
            assert!(
                approx(row.available, accounted, LEDGER_TOLERANCE),
                "{}: {}",
                case.name,
                row.label
            );
        }
    }
}

#[test]
fn merged_sub_contacts_match_one_contact_and_naive_evaluation_multiplies_damage() {
    let (a, b) = (fixtures::shell(), fixtures::tank());
    let whole = geometry(&a, &b);
    let halves = [ContactGeometry {
        width: whole.width / 2.0,
        ..whole
    }; 2];
    let merged = ContactGeometry::merge(&halves).unwrap();
    assert_eq!(merged, whole);
    let load = head_on(&a, &b, 300.0);
    let (region_a, region_b) = fresh(&a, &b);
    let single = impact(&region_a, &region_b, whole, load).unwrap();
    assert_eq!(impact(&region_a, &region_b, merged, load).unwrap(), single);
    let (mut naive_a, mut naive_b) = fresh(&a, &b);
    let mut naive = 0.0;
    for half in halves {
        naive += strike(&mut naive_a, &mut naive_b, half, load).consumed[1];
    }
    assert!(naive > 1.5 * single.consumed[1]);
    // Mismatched sections cannot be merged.
    let other = ContactGeometry {
        a: Section::new(2.0, 0.5),
        ..whole
    };
    assert_eq!(
        ContactGeometry::merge(&[whole, other]),
        Err(MaterialError::InvalidGeometry)
    );
}

#[test]
fn sustained_overloads_fail_in_bounded_time_and_safe_loads_never_charge() {
    let plank = fixtures::branch();
    let region = plank.region();
    let rest = SustainedLoad {
        force: fixtures::tank().mass.unwrap() * GRAVITY,
        width: 2.0,
        section: plank.section(2.0),
        duration: TICK,
        measured_plastic_displacement: None,
    };
    let mut resting = region;
    for _ in 0..1000 {
        let outcome = press(&mut resting, rest);
        assert_eq!(outcome.consumed[0], 0.0);
        assert!(outcome.safe());
        assert!(outcome.ledger.closes(LEDGER_TOLERANCE));
        // Resting is not a loading cycle, so nothing is damped each tick.
        assert_eq!(outcome.ledger.dissipated[0], 0.0);
    }
    assert_eq!(resting.integrity(), 1.0);

    // A mild overload: five percent past the safe stress.
    let overload = SustainedLoad {
        force: 6300.0,
        ..rest
    };
    let one = sustained(&region, overload).unwrap();
    let two = sustained(
        &region,
        SustainedLoad {
            duration: 2.0 * TICK,
            ..overload
        },
    )
    .unwrap();
    assert!(one.yielded[0] && one.consumed[0] > 0.0);
    // Constant strength makes work proportional to duration.
    assert!(approx(two.consumed[0], 2.0 * one.consumed[0], 1.0e-6));
    // The strain energy reported each call is the same persistent amount.
    assert!(approx(
        one.ledger.recoverable[0] + one.ledger.dissipated[0],
        two.ledger.recoverable[0] + two.ledger.dissipated[0],
        1.0e-6
    ));

    // A real overload fails in bounded time, and the failure is reported once.
    let mut crushed = region;
    let mut ticks: usize = 0;
    while !crushed.failed() {
        let outcome = press(&mut crushed, overload);
        ticks += 1;
        assert_eq!(outcome.failed[0], crushed.failed());
        assert!(ticks <= 1000, "runaway");
    }
    let after = press(&mut crushed, overload);
    assert!(after.yielded[0] && !after.failed[0] && after.consumed[0] == 0.0);
    assert!(after.ledger.finite() && after.ledger.closes(LEDGER_TOLERANCE));

    let predicted = (region.capacity() / one.consumed[0]).ceil() as usize;
    assert!(ticks.abs_diff(predicted) <= 1 && ticks > 1);

    // A much larger overload fails sooner.
    let strong = SustainedLoad {
        force: 9000.0,
        ..rest
    };
    let mut crushed_fast = region;
    let mut fast_ticks = 0;
    while !crushed_fast.failed() {
        press(&mut crushed_fast, strong);
        fast_ticks += 1;
        assert!(fast_ticks <= ticks, "runaway");
    }
    assert!(fast_ticks < ticks);

    // A measured displacement replaces the fallback progression.
    let yield_force = WOOD.strength * rest.width * plank.thickness;
    let measured = sustained(
        &region,
        SustainedLoad {
            measured_plastic_displacement: Some(0.02),
            ..strong
        },
    )
    .unwrap();
    assert!(approx(measured.consumed[0], yield_force * 0.02, 1.0e-5));
}

/// Constant-strength analytic rate, independent of the production fallback.
fn exact_fallback(
    material: Material,
    capacity: f32,
    stress: f32,
    volume: f32,
    time: f32,
) -> (f32, f32) {
    let rate = volume * (stress - material.strength) / CRUSH_TIME_CONSTANT;
    ((capacity - rate * time).max(0.0), capacity / rate)
}

/// Step a load at `rate` until the material fails. `tick` applies one tick
/// and returns the remaining capacity and whether the material has failed.
/// Returns the remaining capacity at `COMMON_TIME` and the failure time.
fn step_until_failure(rate: f32, mut tick: impl FnMut() -> (f32, bool)) -> (f32, f32) {
    let common = (rate * fixtures::COMMON_TIME).round() as usize;
    let mut at_common = None;
    let mut ticks: usize = 0;
    loop {
        let (remaining, failed) = tick();
        ticks += 1;
        if ticks == common {
            at_common = Some(remaining);
        }
        if failed {
            break;
        }
        assert!(ticks < 10_000, "runaway");
    }
    (
        at_common.expect("intact at the common time"),
        ticks as f32 / rate,
    )
}

#[test]
fn the_fallback_progression_does_not_depend_on_the_step() {
    // The fixed-step case's plank press flows over the loaded volume.
    let plank = fixtures::branch();
    let area = 2.0 * plank.thickness;
    let (expected, failure) = exact_fallback(
        WOOD,
        plank.region().capacity(),
        6300.0 / area,
        area * plank.section(2.0).depth,
        fixtures::COMMON_TIME,
    );
    for rate in fixtures::STEP_RATES {
        let load = SustainedLoad {
            force: 6300.0,
            width: 2.0,
            section: plank.section(2.0),
            duration: 1.0 / rate,
            measured_plastic_displacement: None,
        };
        let mut region = plank.region();
        let (remaining, time) = step_until_failure(rate, || {
            press(&mut region, load);
            (region.remaining(), region.failed())
        });
        assert!(
            approx(remaining, expected, 1.0e-4),
            "{rate} Hz: {remaining} vs {expected}"
        );
        assert!(
            time >= failure - 1.0e-6 && time <= failure + 1.0 / rate + 1.0e-6,
            "{rate} Hz: {time} s vs {failure} s"
        );
    }

    // Its connection moment flows over a third of the beam.
    let beam = Beam::new(1.0, 1.0, 2.0);
    let joint = Connection::new(WOOD, beam).unwrap();
    let (expected, failure) = exact_fallback(
        WOOD,
        joint.region().capacity(),
        540.0 / beam.section_modulus(),
        beam.volume() / 3.0,
        fixtures::COMMON_TIME,
    );
    for rate in fixtures::STEP_RATES {
        let load = ConnectionLoad {
            moment: 540.0,
            axial: 0.0,
            duration: 1.0 / rate,
            measured_plastic_rotation: None,
            measured_plastic_extension: None,
        };
        let mut stepped = joint;
        let (remaining, time) = step_until_failure(rate, || {
            bend(&mut stepped, load);
            (stepped.region().remaining(), stepped.failed())
        });
        assert!(
            approx(remaining, expected, 1.0e-4),
            "{rate} Hz: {remaining} vs {expected}"
        );
        assert!(
            time >= failure - 1.0e-6 && time <= failure + 1.0 / rate + 1.0e-6,
            "{rate} Hz: {time} s vs {failure} s"
        );
    }

    // One long call charges the work of the ticks it spans, also across the
    // tick that exhausts the plank.
    let tick_load = SustainedLoad {
        force: 6300.0,
        width: 2.0,
        section: plank.section(2.0),
        duration: TICK,
        measured_plastic_displacement: None,
    };
    let work = |outcome: Outcome| outcome.consumed[0] + outcome.ledger.excess;
    let mut split = plank.region();
    let split_work: f32 = (0..12).map(|_| work(press(&mut split, tick_load))).sum();
    let whole = sustained(
        &plank.region(),
        SustainedLoad {
            duration: 12.0 * TICK,
            ..tick_load
        },
    )
    .unwrap();
    assert!(split.failed());
    assert!(approx(split_work, work(whole), 1.0e-5));
}

#[test]
fn only_the_overloaded_connection_fails() {
    let beam = Beam::new(1.0, 1.0, 2.0);
    let mut joints = [Connection::new(WOOD, beam).unwrap(); 3];
    let arms = [6.0_f32, 4.0, 2.0];
    for _ in 0..400 {
        for (joint, arm) in joints.iter_mut().zip(arms) {
            let outcome = bend(
                joint,
                ConnectionLoad {
                    moment: 100.0 * arm,
                    axial: 0.0,
                    duration: TICK,
                    measured_plastic_rotation: None,
                    measured_plastic_extension: None,
                },
            );
            assert!(outcome.ledger.closes(LEDGER_TOLERANCE));
        }
    }
    assert!(joints[0].failed());
    assert_eq!(joints[1].integrity(), 1.0);
    assert_eq!(joints[2].integrity(), 1.0);
    assert!(approx(beam.section_modulus(), 1.0 / 6.0, 1.0e-6));
    assert!(approx(beam.second_moment(), 1.0 / 12.0, 1.0e-6));

    // Pure tension on a root attachment uses the same law.
    let root = Connection::new(WOOD, beam).unwrap();
    let pull = connection(
        &root,
        ConnectionLoad {
            moment: 0.0,
            axial: 2.0 * WOOD.strength * beam.area(),
            duration: TICK,
            measured_plastic_rotation: None,
            measured_plastic_extension: Some(0.01),
        },
    )
    .unwrap();
    assert!(pull.yielded[0]);
    assert!(approx(
        pull.consumed[0],
        WOOD.strength * beam.area() * 0.01,
        1.0e-5
    ));
    let gentle = connection(
        &root,
        ConnectionLoad {
            axial: 0.5 * WOOD.strength * beam.area(),
            ..ConnectionLoad {
                moment: 0.0,
                axial: 0.0,
                duration: TICK,
                measured_plastic_rotation: None,
                measured_plastic_extension: None,
            }
        },
    )
    .unwrap();
    assert!(gentle.safe() && gentle.ledger.recoverable[0] > 0.0);
}

#[test]
fn repair_split_and_absorb_stay_bounded() {
    let mut region = Region::new(ARMOR, 10.0).unwrap();
    assert_eq!(region.capacity(), ARMOR.toughness * 10.0);
    assert!(approx(region.mass(), ARMOR.density * 10.0, 1.0e-6));
    assert_eq!(region.absorb(region.capacity() * 0.25).unwrap(), 0.0);
    assert!(approx(region.integrity(), 0.75, 1.0e-6));
    assert!(approx(region.normalized_health(), 75.0, 1.0e-5));
    assert!(approx(region.effective_strength(), ARMOR.strength, 1.0e-6));
    let excess = region.absorb(region.capacity()).unwrap();
    assert!(approx(excess, region.capacity() * 0.25, 1.0e-5));
    assert!(region.failed());
    assert_eq!(region.integrity(), 0.0);
    assert!(approx(region.effective_strength(), ARMOR.strength, 1.0e-6));
    region.repair(0.5).unwrap();
    assert!(approx(region.integrity(), 0.5, 1.0e-6));
    region.repair(1.0).unwrap();
    assert_eq!(region.integrity(), 1.0);
    assert_eq!(region.repair(1.5), Err(MaterialError::InvalidRegion));
    assert_eq!(region.absorb(-1.0), Err(MaterialError::InvalidRegion));
    region.absorb(region.capacity() * 0.4).unwrap();
    let (part, rest) = region.split(0.25).unwrap();
    assert!(approx(
        part.volume() + rest.volume(),
        region.volume(),
        1.0e-6
    ));
    assert!(approx(part.integrity(), region.integrity(), 1.0e-6));
    assert!(approx(rest.integrity(), region.integrity(), 1.0e-6));
    assert!(approx(
        part.remaining() + rest.remaining(),
        region.remaining(),
        1.0e-5
    ));
    assert_eq!(region.split(1.0), Err(MaterialError::InvalidRegion));
    // A saved region restores exactly, and only within its capacity.
    assert_eq!(
        Region::with_remaining(ARMOR, 10.0, region.remaining()),
        Ok(region)
    );
    for remaining in [-1.0, 2.0 * region.capacity(), f32::NAN] {
        assert_eq!(
            Region::with_remaining(ARMOR, 10.0, remaining),
            Err(MaterialError::InvalidRegion)
        );
    }
}

#[test]
fn committed_report_matches_the_regenerated_report() {
    let regenerated = serde_json::to_string_pretty(&fixtures::report()).unwrap() + "\n";
    let committed = include_str!("../../../docs/data/material-model-v2.json");
    assert!(
        committed == regenerated,
        "docs/data/material-model-v2.json is stale; run `cargo run -p engine-material --example reference_cases`"
    );
    let markdown = fixtures::render_markdown(&fixtures::report());
    let committed_markdown = include_str!("../../../docs/data/material-model-v2.md");
    assert!(
        committed_markdown == markdown,
        "docs/data/material-model-v2.md is stale; run `cargo run -p engine-material --example reference_cases`"
    );
}

#[test]
fn markdown_render_lists_every_case_and_material() {
    let report = fixtures::report();
    let markdown = fixtures::render_markdown(&report);
    for case in report.cases.iter().chain(&report.sweeps) {
        assert!(markdown.contains(case.name), "{}", case.name);
    }
    for material in ALL_REFERENCE {
        assert!(markdown.contains(material.name));
    }
    assert!(markdown.contains("| yes |"));
    assert!(!markdown.contains("| no |"));
}

#[test]
fn energetic_wood_can_damage_armor_without_a_pair_exception() {
    let (a, b) = (fixtures::branch(), fixtures::tank());
    let mut previous = 0.0;
    for speed in [15.0, 40.0, 80.0, 120.0, 240.0] {
        let hit = impact(
            &a.region(),
            &b.region(),
            geometry(&a, &b),
            head_on(&a, &b, speed),
        )
        .unwrap();
        assert!(hit.failed[0]);
        if speed <= 40.0 {
            assert_eq!(hit.consumed[1], 0.0);
        } else {
            assert!(hit.consumed[1] > previous && hit.yielded[1]);
            assert!(hit.ledger.peak_stress[1] > ARMOR.strength);
        }
        previous = hit.consumed[1];
        assert!(hit.ledger.closes(LEDGER_TOLERANCE));
    }
}

#[test]
fn force_matches_independent_numerical_integration_of_loading_curves() {
    let (a, b) = (
        Region::new(WOOD, 1000.0).unwrap(),
        Region::new(ARMOR, 1000.0).unwrap(),
    );
    let contact = ContactGeometry::new(0.4, Section::new(1.0, 0.3), Section::new(1.0, 0.1));
    // Integrate F dx directly in displacement space, independently of the
    // production force-squared inversion. Cover neither, one and both yielding.
    for force in [400.0_f64, 4000.0, 12000.0] {
        let mut energy = 0.0;
        for (material, section) in [(WOOD, contact.a), (ARMOR, contact.b)] {
            let stiffness =
                f64::from(section.contact_stiffness(material.stiffness, contact.area()));
            let yield_force = f64::from(material.strength * contact.area());
            let slope = stiffness * f64::from(POST_YIELD_STIFFNESS_FRACTION);
            let yield_displacement = yield_force / stiffness;
            let end = if force <= yield_force {
                force / stiffness
            } else {
                yield_displacement + (force - yield_force) / slope
            };
            let dx = end / 10000.0;
            for i in 0..10000 {
                let x = (f64::from(i) + 0.5) * dx;
                let f = if x <= yield_displacement {
                    stiffness * x
                } else {
                    yield_force + slope * (x - yield_displacement)
                };
                energy += f * dx;
            }
        }
        let hit = impact(
            &a,
            &b,
            contact,
            Impact {
                closing_speed: libm::sqrt(2.0 * energy) as f32,
                effective_mass: 1.0,
            },
        )
        .unwrap();
        assert!(approx(hit.ledger.peak_force, force as f32, 1.0e-4));
        assert!(hit.ledger.closes(LEDGER_TOLERANCE));
    }
}

#[test]
fn strength_crossing_is_continuous_and_side_order_is_symmetric() {
    let contact = ContactGeometry::new(1.0, Section::new(1.0, 1.0), Section::new(1.0, 1.0));
    let load = Impact {
        closing_speed: 20.0,
        effective_mass: 10.0,
    };
    let target = Region::new(ARMOR, 1000.0).unwrap();
    let mut work = Vec::new();
    for ratio in [0.999, 1.0, 1.001] {
        let striker = Region::new(
            Material {
                strength: ARMOR.strength * ratio,
                ..ARMOR
            },
            1000.0,
        )
        .unwrap();
        let hit = impact(&striker, &target, contact, load).unwrap();
        let reverse = impact(&target, &striker, contact, load).unwrap();
        assert_eq!(hit.consumed, [reverse.consumed[1], reverse.consumed[0]]);
        assert!(hit.consumed.iter().all(|w| *w > 0.0));
        work.push(hit.consumed);
    }
    assert_eq!(work[1][0], work[1][1]);
    for ((low, high), middle) in work[0].iter().zip(work[2]).zip(work[1]) {
        assert!((low - high).abs() < 0.03 * middle);
    }
}

#[test]
fn all_material_pairs_close_and_bound_work_across_loads_and_capacity() {
    for a in ALL_REFERENCE {
        for b in ALL_REFERENCE {
            for width in [0.01, 1.0, 100.0] {
                let contact =
                    ContactGeometry::new(width, Section::new(0.5, width), Section::new(1.0, 0.2));
                for fraction in [0.0, 0.01, 1.0] {
                    let a = Region::with_remaining(a, 1.0, fraction * a.toughness).unwrap();
                    let b = Region::with_remaining(b, 2.0, fraction * b.toughness * 2.0).unwrap();
                    for speed in [0.0, 0.01, 1.0, 100.0, 10000.0] {
                        let hit = impact(
                            &a,
                            &b,
                            contact,
                            Impact {
                                closing_speed: speed,
                                effective_mass: 3.0,
                            },
                        )
                        .unwrap();
                        assert!(
                            hit.ledger.finite() && hit.ledger.closes(LEDGER_TOLERANCE),
                            "{a:?} {b:?} {hit:?}"
                        );
                        assert!(
                            hit.consumed[0] <= a.remaining() && hit.consumed[1] <= b.remaining()
                        );
                        assert_eq!(
                            hit.failed[0],
                            a.remaining() > 0.0 && hit.consumed[0] == a.remaining()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn excess_requires_explicit_bounded_failure_allocation() {
    let (a, b) = (fixtures::branch(), fixtures::tank());
    let original = impact(
        &a.region(),
        &b.region(),
        geometry(&a, &b),
        head_on(&a, &b, 120.0),
    )
    .unwrap()
    .ledger;
    assert!(original.excess > 0.0);
    assert_eq!(original.restitution(), Err(MaterialError::UnresolvedExcess));
    for motion in [-1.0, f32::NAN, original.excess * 2.0] {
        let mut ledger = original;
        assert_eq!(
            ledger.resolve_excess(motion),
            Err(MaterialError::InvalidLoad)
        );
        assert_eq!(ledger, original);
    }
    for fraction in [0.0, 0.5, 1.0] {
        let mut ledger = original;
        ledger.resolve_excess(original.excess * fraction).unwrap();
        assert_eq!(ledger.excess, 0.0);
        assert_eq!(ledger.failure_motion, original.excess * fraction);
        assert!(approx(
            ledger.failure_motion + ledger.failure_dissipated,
            original.excess,
            1.0e-6
        ));
        assert!(ledger.closes(LEDGER_TOLERANCE));
        assert!(ledger.restitution().is_ok());
        let settled = ledger;
        assert_eq!(ledger.resolve_excess(1.0), Err(MaterialError::InvalidLoad));
        assert_eq!(ledger, settled);
        ledger.resolve_excess(0.0).unwrap();
        assert_eq!(ledger, settled);
    }
}

#[test]
fn gentle_contacts_can_dissipate_all_energy_without_damage() {
    let material = Material {
        damping: 1.0,
        ..HULL
    };
    let region = Region::new(material, 1.0).unwrap();
    let contact = ContactGeometry::new(1.0, Section::new(1.0, 0.1), Section::new(1.0, 0.1));
    let hit = impact(
        &region,
        &region,
        contact,
        Impact {
            closing_speed: 0.1,
            effective_mass: 1.0,
        },
    )
    .unwrap();
    assert!(hit.safe());
    assert_eq!(hit.consumed, [0.0; 2]);
    assert_eq!(hit.ledger.restitution(), Ok(0.0));
    assert!(approx(
        hit.ledger.dissipated.iter().sum(),
        hit.ledger.available,
        1.0e-6
    ));
    let (a, b) = (fixtures::ship(), fixtures::planet());
    let landing = impact(
        &a.region(),
        &b.region(),
        geometry(&a, &b),
        head_on(&a, &b, 0.5 * safe_limit_speed(&a, &b)),
    )
    .unwrap();
    assert!(landing.safe() && landing.ledger.restitution().unwrap() < 0.2);
    let candidate = fixtures::landing_ship();
    for fraction in [1.0, 0.5, 0.01] {
        let target = Region::with_remaining(
            candidate.material,
            candidate.volume,
            candidate.region().capacity() * fraction,
        )
        .unwrap();
        for speed in [1.0, 1.6, 2.2] {
            let landing = impact(
                &target,
                &b.region(),
                geometry(&candidate, &b),
                head_on(&candidate, &b, speed),
            )
            .unwrap();
            assert_eq!(landing.consumed, [0.0; 2]);
            assert!(landing.ledger.restitution().unwrap() < 0.1);
        }
    }
}

#[test]
fn zero_duration_adds_no_work_even_with_measured_displacement() {
    let region = Region::new(WOOD, 1.0).unwrap();
    assert_eq!(
        sustained(
            &region,
            SustainedLoad {
                force: 1.0e6,
                width: 1.0,
                section: Section::new(1.0, 1.0),
                duration: 0.0,
                measured_plastic_displacement: Some(1.0),
            }
        )
        .unwrap(),
        Outcome::default()
    );
    let joint = Connection::new(WOOD, Beam::new(1.0, 1.0, 1.0)).unwrap();
    assert_eq!(
        connection(
            &joint,
            ConnectionLoad {
                moment: 1.0e6,
                axial: 1.0e6,
                duration: 0.0,
                measured_plastic_rotation: Some(1.0),
                measured_plastic_extension: Some(1.0),
            }
        )
        .unwrap(),
        Outcome::default()
    );
}
