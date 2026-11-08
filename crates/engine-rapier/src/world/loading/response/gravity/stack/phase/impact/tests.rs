use super::*;
use crate::world::loading::response::gravity::tests::stack_world;

fn recoil_world() -> PhysicsWorld {
    let mut world = stack_world([1, 2, 3], false, [4, 2, 4], 2);
    let body = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
    world.set_pose(body, Vec2::ZERO, 0.0, true);
    // This late-return velocity produces a real, nonzero rounded B recoil.
    world.set_velocity(body, Vec2::X * f32::from_bits(1_032_097_870), 0.0, true);
    let materials = &mut world
        .loading
        .as_mut()
        .unwrap()
        .response
        .as_mut()
        .unwrap()
        .materials;
    let tokens: Vec<_> = materials.keys().copied().collect();
    // Match the independent ARMOR/ARMOR/WOOD fixture.
    materials.insert(tokens[1], materials[&tokens[0]].clone());
    world
}

fn state(world: &mut PhysicsWorld) -> &mut State {
    let Status::Stack(state) = &mut world
        .loading
        .as_mut()
        .unwrap()
        .gravity
        .as_mut()
        .unwrap()
        .status
    else {
        panic!("expected stack history");
    };
    state
}

fn refused(mut world: PhysicsWorld, expected: Rejection) {
    let bytes = world.snapshot_bytes().unwrap();
    let batches = world.loading_batches().to_vec();
    let responses = world.material_response_batches().to_vec();
    let events = world.contact_events().to_vec();
    let diagnostics = (
        world.loading_diagnostics(),
        world.material_response_diagnostics(),
    );
    assert_eq!(
        instant(&mut world, GravitySettling::BoundedNumerical),
        Err(expected)
    );
    assert_eq!(world.snapshot_bytes().unwrap(), bytes);
    assert_eq!(world.loading_batches(), batches);
    assert_eq!(world.material_response_batches(), responses);
    assert_eq!(world.contact_events(), events);
    assert_eq!(
        (
            world.loading_diagnostics(),
            world.material_response_diagnostics()
        ),
        diagnostics
    );
}

#[test]
fn gravity_stack_phase_real_recoil_charges_actual_work_and_retains_history() {
    let fresh = recoil_world();
    let mut prepared = Staged::prepare(&fresh, GravitySettling::BoundedNumerical, 2).unwrap();
    let history = prepared
        .world
        .loading
        .as_ref()
        .unwrap()
        .gravity
        .as_ref()
        .unwrap()
        .history
        .clone();
    let prior = Corrections {
        impulse: 1.0e-7,
        work: 1.0e-12,
    };
    state(&mut prepared.world).corrections = prior.clone();
    let mut world = prepared.world;
    let mut replay = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
    let response = instant(&mut world, GravitySettling::BoundedNumerical).unwrap();
    assert_eq!(
        response,
        instant(&mut replay, GravitySettling::BoundedNumerical).unwrap()
    );
    assert_eq!(
        world.snapshot_bytes().unwrap(),
        replay.snapshot_bytes().unwrap()
    );
    let c = response.correction.unwrap();
    let recoil = f64::from(-1.1920929e-8_f32);
    assert_eq!(c.impulse, 20.0 * recoil);
    assert_eq!(c.dissipated, 10.0 * recoil * recoil);
    assert_eq!(
        state(&mut world).corrections,
        Corrections {
            impulse: prior.impulse + c.impulse,
            work: prior.work + c.dissipated,
        }
    );
    assert_eq!(
        world
            .loading
            .as_ref()
            .unwrap()
            .gravity
            .as_ref()
            .unwrap()
            .history,
        history
    );
    let resumed = Staged::prepare(&world, GravitySettling::BoundedNumerical, 0).unwrap();
    assert_eq!(resumed.active().unwrap().support.id, c.episode);
    assert_eq!(resumed.endpoints()[1].velocity, [0.0; 2]);
    // Correction does not modify any material charge.
    let mut zero_prior = fresh;
    let baseline = instant(&mut zero_prior, GravitySettling::BoundedNumerical).unwrap();
    assert_eq!(response.impact.response, baseline.impact.response);
    assert_eq!(
        bincode::serialize(
            &world
                .loading
                .as_ref()
                .unwrap()
                .response
                .as_ref()
                .unwrap()
                .materials
        )
        .unwrap(),
        bincode::serialize(
            &zero_prior
                .loading
                .as_ref()
                .unwrap()
                .response
                .as_ref()
                .unwrap()
                .materials
        )
        .unwrap()
    );
}

#[test]
fn gravity_stack_phase_recoil_exhaustion_and_lost_increments_roll_back() {
    let fresh = recoil_world();
    let base = Staged::prepare(&fresh, GravitySettling::BoundedNumerical, 2)
        .unwrap()
        .world;
    for case in 0..4 {
        let mut world = base.clone();
        let history = &mut world
            .loading
            .as_mut()
            .unwrap()
            .gravity
            .as_mut()
            .unwrap()
            .history;
        let corrections = match case {
            0 => Corrections {
                impulse: -1.0e-5 * history.scales[1],
                work: 1.0e-12,
            },
            1 => Corrections {
                impulse: 0.0,
                work: 1.0e-5 * history.scales[0],
            },
            _ => {
                // A valid much larger original reference makes either tiny new
                // increment unrepresentable without exceeding the fixed bounds.
                history.reference[1] = f64::from(1.0e20_f32);
                history.scales[0] = 10.0 * history.reference[1] * history.reference[1];
                history.scales[1] = libm::sqrt(40.0 * history.scales[0]);
                Corrections {
                    impulse: if case == 2 { 1.0e10 } else { 0.0 },
                    work: if case == 2 { 1.0 } else { 1.0e10 },
                }
            }
        };
        state(&mut world).corrections = corrections;
        // This is accepted structural state; the *new* physical response cannot commit.
        let world = PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().unwrap()).unwrap();
        refused(world, Rejection::Numerical);
    }
    let mut accepted = fresh.clone();
    assert!(
        instant(&mut accepted, GravitySettling::BoundedNumerical)
            .unwrap()
            .correction
            .is_some()
    );
    let mut limited = fresh;
    limited
        .loading
        .as_mut()
        .unwrap()
        .config
        .max_records_per_step = accepted.loading.as_ref().unwrap().records - 1;
    let staged = Staged::prepare(&limited, GravitySettling::BoundedNumerical, 2).unwrap();
    assert_eq!(staged.metrics.len(), 1);
    // Both captures fit; only the final correction record exhausts the allowance.
    refused(limited, Rejection::RecordLimit);
}

#[test]
fn gravity_stack_phase_recoil_guard_refuses_larger_or_nonfinite_motion() {
    for velocity in [0.001, -0.001, f32::INFINITY, f32::NAN] {
        let mut staged =
            Staged::prepare(&recoil_world(), GravitySettling::BoundedNumerical, 2).unwrap();
        let handle = staged.scene.plates.bodies[1];
        staged.world.raw.bodies[handle].set_linvel(Vector::X * velocity, true);
        let before = state(&mut staged.world).corrections.clone();
        let records = staged.world.loading.as_ref().unwrap().records;
        assert_eq!(
            staged.correct_recoil(0.064728364),
            Err(Rejection::Numerical)
        );
        assert_eq!(state(&mut staged.world).corrections, before);
        assert_eq!(staged.world.loading.as_ref().unwrap().records, records);
    }
}
