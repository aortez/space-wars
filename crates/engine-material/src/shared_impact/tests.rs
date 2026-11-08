use super::*;

fn mobile(v: Vec2) -> BodyMotion {
    BodyMotion {
        center: Vec2::ZERO,
        linear_velocity: v,
        angular_velocity: 0.0,
        inverse_mass: Vec2::splat(0.5),
        inverse_inertia: 0.0,
    }
}
fn fixed(v: Vec2) -> BodyMotion {
    BodyMotion {
        inverse_mass: Vec2::ZERO,
        ..mobile(v)
    }
}
fn contact(id: u64, normal: Vec2) -> Contact {
    Contact {
        id,
        bodies: [0, 1],
        point: Vec2::ZERO,
        normal,
        stiffness: 1.0,
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1.0e-7 * b.abs().max(1.0), "{a} != {b}");
}
fn total(b: &Budget) -> f64 {
    b.components.iter().map(|c| c.available).sum()
}

#[test]
fn redundant_patches_share_motion_instead_of_duplicating_isolated_energy() {
    let bodies = [mobile(Vec2::X * 4.0), fixed(Vec2::ZERO)];
    let contacts = [
        contact(1, Vec2::X),
        Contact {
            stiffness: 3.0,
            ..contact(2, Vec2::X)
        },
    ];
    let budget = allocate(&bodies, &contacts).unwrap();
    assert_eq!(budget.components.len(), 1);
    near(total(&budget), 16.0);
    near(budget.contacts[0].energy, 4.0);
    near(budget.contacts[1].energy, 12.0);
    near(budget.contacts[0].compression_impulse, 2.0);
    near(budget.contacts[1].compression_impulse, 6.0);
    assert!(budget.iterations <= MAX_ITERATIONS);
    assert_eq!(
        budget,
        allocate(&bodies, &[contacts[1], contacts[0]]).unwrap()
    );
    // Both independent evaluations would each consume the full 16-unit mode.
    near(
        allocate(&bodies, &contacts[..1]).unwrap().contacts[0].energy,
        16.0,
    );
}

#[test]
fn equivalent_patch_partition_preserves_budget_and_material_work() {
    let bodies = [mobile(Vec2::X * 40.0), fixed(Vec2::ZERO)];
    let region = crate::Region::new(crate::ARMOR, 20.0).unwrap();
    let mut expected = None;
    for parts in [1, 2, 4, 8] {
        let contacts: Vec<_> = (0..parts)
            .map(|i| Contact {
                stiffness: 1.0 / parts as f32,
                ..contact(i, Vec2::X)
            })
            .collect();
        let budget = allocate(&bodies, &contacts).unwrap();
        near(total(&budget), 1600.0);
        let mut work = 0.0;
        for part in budget.contacts {
            let outcome = crate::impact(
                &region,
                &region,
                crate::ContactGeometry::new(
                    1.0 / parts as f32,
                    crate::Section::new(1.0, 1.0),
                    crate::Section::new(1.0, 1.0),
                ),
                part.material_input().unwrap().unwrap(),
            )
            .unwrap();
            work += outcome.consumed.iter().map(|v| f64::from(*v)).sum::<f64>();
        }
        if let Some(expected) = expected {
            near(work, expected);
        }
        assert!(work > 0.0);
        expected = Some(work);
    }
}

#[test]
fn independent_normal_components_do_not_exchange_recoverable_energy() {
    let bodies = [mobile(Vec2::new(4.0, 3.0)), fixed(Vec2::ZERO)];
    let result = allocate(&bodies, &[contact(1, Vec2::X), contact(2, Vec2::Y)]).unwrap();
    assert_eq!(result.components.len(), 2);
    near(result.components[0].available, 16.0);
    near(result.components[1].available, 9.0);
}

#[test]
fn oblique_normals_and_angular_modes_respect_kinetic_energy() {
    let bodies = [mobile(Vec2::new(4.0, 3.0)), fixed(Vec2::ZERO)];
    let normals = [contact(1, Vec2::X), contact(2, Vec2::new(0.5, 0.8660254))];
    let budget = allocate(&bodies, &normals).unwrap();
    near(total(&budget), 25.0);
    assert_eq!(budget.components.len(), 1);
    let mut rotating = bodies;
    rotating[0].angular_velocity = 0.5;
    rotating[0].inverse_inertia = 1.0;
    let budget = allocate(
        &rotating,
        &[
            Contact {
                point: Vec2::new(1.0, 1.0),
                ..contact(1, Vec2::X)
            },
            Contact {
                point: Vec2::new(1.0, -1.0),
                ..contact(2, Vec2::Y)
            },
        ],
    )
    .unwrap();
    near(total(&budget), 24.5);
    near(budget.components[0].kinetic_removed, 24.5);
}

#[test]
fn dynamic_support_chain_uses_relative_energy_and_preserves_center_of_mass_motion() {
    let bodies = [
        mobile(Vec2::X * 4.0),
        mobile(Vec2::ZERO),
        mobile(-Vec2::X * 2.0),
    ];
    let budget = allocate(
        &bodies,
        &[
            contact(1, Vec2::X),
            Contact {
                bodies: [1, 2],
                ..contact(2, Vec2::X)
            },
        ],
    )
    .unwrap();
    near(total(&budget), 20.0 - 4.0 / 3.0);
    near(budget.components[0].prescribed_work, 0.0);
    let shifted = bodies.map(|mut b| {
        b.linear_velocity += Vec2::new(17.0, -5.0);
        b
    });
    assert_eq!(
        budget,
        allocate(
            &shifted,
            &[
                contact(1, Vec2::X),
                Contact {
                    bodies: [1, 2],
                    ..contact(2, Vec2::X)
                }
            ]
        )
        .unwrap()
    );
}

#[test]
fn moving_support_accounts_driver_work_separately_from_world_frame_energy() {
    for shift in [-17.0, 0.0, 17.0] {
        let bodies = [mobile(Vec2::X * (4.0 + shift)), fixed(Vec2::X * shift)];
        let budget = allocate(&bodies, &[contact(1, Vec2::X)]).unwrap();
        let component = &budget.components[0];
        near(component.available, 16.0);
        near(component.prescribed_work, -8.0 * f64::from(shift));
        near(component.kinetic_removed, 16.0 + 8.0 * f64::from(shift));
        near(
            component.kinetic_removed + component.prescribed_work,
            component.available,
        );
    }
}

#[test]
fn inactive_contacts_cannot_take_tensile_impulses_or_negative_work() {
    let bodies = [mobile(Vec2::new(4.0, -1.0)), fixed(Vec2::ZERO)];
    let budget = allocate(
        &bodies,
        &[contact(1, Vec2::X), contact(2, Vec2::new(0.5, 0.8660254))],
    )
    .unwrap();
    near(total(&budget), 16.0);
    near(budget.contacts[1].compression_impulse, 0.0);
    assert_eq!(budget.contacts[1].energy, 0.0);
    assert_eq!(budget.contacts[1].material_input(), Ok(None));
}

#[test]
fn invalid_and_incompatible_inputs_return_no_partial_budget() {
    let bodies = [mobile(Vec2::X), fixed(Vec2::ZERO)];
    assert_eq!(
        allocate(&bodies, &[contact(1, Vec2::X), contact(1, Vec2::X)]),
        Err(BudgetError::DuplicateContact)
    );
    assert_eq!(
        allocate(&bodies, &[contact(1, -Vec2::X)]),
        Err(BudgetError::SeparatingContact)
    );
    assert_eq!(
        allocate(&bodies, &[contact(1, Vec2::X); MAX_CONTACTS + 1]),
        Err(BudgetError::Limit)
    );
    assert_eq!(
        allocate(&bodies, &[contact(1, Vec2::new(0.5, 0.0))]),
        Err(BudgetError::InvalidInput)
    );
    assert_eq!(
        allocate(&[fixed(Vec2::X), fixed(Vec2::ZERO)], &[contact(1, Vec2::X)]),
        Err(BudgetError::ImmovableContact)
    );
    let driven = [mobile(Vec2::ZERO), fixed(-Vec2::X), fixed(Vec2::X)];
    assert_eq!(
        allocate(
            &driven,
            &[
                contact(1, Vec2::X),
                Contact {
                    bodies: [0, 2],
                    ..contact(2, -Vec2::X)
                }
            ]
        ),
        Err(BudgetError::NotConverged)
    );
}

#[test]
fn empty_and_stationary_batches_have_no_available_energy() {
    let empty = allocate(&[], &[]).unwrap();
    assert!(empty.contacts.is_empty());
    assert!(empty.components.is_empty());
    assert_eq!(empty.iterations, 0);
    let bodies = [mobile(Vec2::Y * 5.0), fixed(Vec2::ZERO)];
    let stationary = allocate(&bodies, &[contact(1, Vec2::X)]).unwrap();
    assert_eq!(stationary.iterations, 0);
    assert_eq!(total(&stationary), 0.0);
    assert_eq!(stationary.contacts[0].material_input(), Ok(None));
}

#[test]
fn material_conversion_rejects_energy_that_f32_would_silently_drop() {
    let contact = ContactBudget {
        id: 1,
        component: 0,
        closing_speed: 1.0e-40,
        loading_speed: 1.0e-40,
        compression_impulse: 1.0e-40,
        energy: 0.5e-80,
    };
    assert_eq!(contact.material_input(), Err(BudgetError::Range));
    let ordinary = ContactBudget {
        closing_speed: 3.0,
        loading_speed: 3.0,
        energy: 7.0,
        ..contact
    };
    let converted = ordinary.material_input().unwrap().unwrap();
    assert!((f64::from(converted.energy()) - ordinary.energy).abs() < 1.0e-6);
}

#[test]
fn resting_chain_contact_transmits_impulse_but_initial_work_split_is_zero() {
    let bodies = [
        mobile(Vec2::X * 4.0),
        mobile(Vec2::ZERO),
        mobile(Vec2::ZERO),
    ];
    let budget = allocate(
        &bodies,
        &[
            contact(1, Vec2::X),
            Contact {
                bodies: [1, 2],
                ..contact(2, Vec2::X)
            },
        ],
    )
    .unwrap();
    near(total(&budget), 16.0 - 16.0 / 3.0);
    let resting = &budget.contacts[1];
    near(resting.compression_impulse, 8.0 / 3.0);
    assert_eq!(resting.energy, 0.0);
    // This captures a limit of the allocation convention, not evidence that
    // the downstream material cannot deform. Production needs transmitted
    // loading/work before using zero allocated energy to skip material damage.
    assert_eq!(resting.material_input(), Ok(None));
}

#[test]
fn pulse_work_reaches_resting_contacts_and_preserves_each_compression_impulse() {
    let bodies = [
        mobile(Vec2::X * 40.0),
        mobile(Vec2::ZERO),
        mobile(Vec2::ZERO),
    ];
    let contacts = [
        contact(1, Vec2::X),
        Contact {
            bodies: [1, 2],
            ..contact(2, Vec2::X)
        },
    ];
    let budget = allocate_pulse(&bodies, &contacts).unwrap();
    near(total(&budget), 1600.0 - 1600.0 / 3.0);
    near(budget.contacts[0].energy / budget.contacts[1].energy, 4.0);
    assert_eq!(budget.contacts[1].closing_speed, 0.0);
    assert!(budget.contacts[1].loading_speed > 0.0);
    let region = crate::Region::new(crate::WOOD, 20.0).unwrap();
    for c in &budget.contacts {
        let input = c.material_input().unwrap().unwrap();
        assert!((f64::from(input.energy()) - c.energy).abs() < 1.0e-6 * c.energy);
        assert!(
            (f64::from(input.effective_mass * input.closing_speed) - c.compression_impulse).abs()
                < 1.0e-6 * c.compression_impulse
        );
        let outcome = crate::impact(
            &region,
            &region,
            crate::ContactGeometry::new(
                1.0,
                crate::Section::new(1.0, 1.0),
                crate::Section::new(1.0, 1.0),
            ),
            input,
        )
        .unwrap();
        assert!(outcome.consumed.iter().sum::<f32>() > 0.0);
    }
    assert_eq!(
        budget,
        allocate_pulse(&bodies, &[contacts[1], contacts[0]]).unwrap()
    );
    let stiffer_downstream = allocate_pulse(
        &bodies,
        &[
            contacts[0],
            Contact {
                stiffness: 4.0,
                ..contacts[1]
            },
        ],
    )
    .unwrap();
    near(
        stiffer_downstream.contacts[0].energy / stiffer_downstream.contacts[1].energy,
        16.0,
    );
}

#[test]
fn pulse_partition_and_translation_preserve_total_work() {
    for shift in [0.0, 17.0] {
        let bodies = [mobile(Vec2::X * (40.0 + shift)), fixed(Vec2::X * shift)];
        for parts in [1, 2, 4, 8] {
            let contacts: Vec<_> = (0..parts)
                .map(|id| Contact {
                    stiffness: 1.0 / parts as f32,
                    ..contact(id, Vec2::X)
                })
                .collect();
            let result = allocate_pulse(&bodies, &contacts).unwrap();
            near(total(&result), 1600.0);
            for c in result.contacts {
                near(c.energy, 1600.0 / parts as f64);
            }
        }
    }
}
