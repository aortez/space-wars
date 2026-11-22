//! Ship lasers as material weapon beams against a match's ship, pod and pilot
//! on foot (#330).
use super::material_shots::{Target, arrange, fixture, health};
use super::*;

/// The rounds' fixture, with seat 0's ship facing seat 1.
fn aimed(target: Target) -> SurfaceSortieState {
    aim(fixture(target))
}

/// `state` with seat 0's ship facing seat 1.
fn aim(mut state: SurfaceSortieState) -> SurfaceSortieState {
    let shooter = &mut state.world.ships[0];
    shooter.direction = Vec2::X;
    shooter.rotation_radians = rotation_for_direction(Vec2::X);
    state
}

fn shine(state: &mut SurfaceSortieState) {
    let action = combat::SurfaceWeaponAction {
        laser: true,
        cannon: false,
    }
    .encode(PlayerId::PLAYER_1);
    SurfaceSortieScenario::step(state, &[action], Duration::from_nanos(16_666_667));
}

/// What the laser's hits name `target` as.
fn laser_target(target: Target) -> LaserTarget {
    match target {
        Target::Ship | Target::Pod => LaserTarget::Ship(1),
        Target::OnFoot => LaserTarget::Spaceling(1),
    }
}

/// The health the laser's hits this step charged `target`.
fn charged(state: &SurfaceSortieState, target: Target) -> f32 {
    let hits = state.world.laser_hits.iter();
    let hits = hits.filter(|hit| hit.target == laser_target(target));
    hits.map(|hit| hit.damage).sum()
}

#[test]
fn the_laser_fails_a_ship_a_pod_and_a_pilot_on_foot_in_tiers() {
    // The seconds the laser takes from 40 units off: the old laser's for the
    // ship, and less for the pod and the pilot, whose thinner bodies take less
    // of the beam but have less to lose.
    let expected = [13.87, 5.75, 2.87];
    let seconds = [Target::Ship, Target::Pod, Target::OnFoot].map(|target| {
        let mut state = aimed(target);
        let start = state.world.tick;
        for _ in 0..60 * 60 {
            let before = health(&state, target);
            shine(&mut state);
            // The target loses what the beam charged it, and no old damage
            // besides.
            let lost = before - health(&state, target);
            let charged = charged(&state, target).min(before);
            assert!(
                (lost - charged).abs() < 1.0e-3,
                "{target:?}: {lost} {charged}"
            );
            if health(&state, target) == 0.0 {
                break;
            }
        }
        assert_eq!(health(&state, target), 0.0, "{target:?}");
        if target != Target::OnFoot {
            assert_eq!(
                vitals(&state, 1).health,
                PILOT_HEALTH,
                "the hull shields its pilot"
            );
        }
        (state.world.tick - start) as f32 / 60.0
    });
    for (seconds, expected) in seconds.iter().zip(expected) {
        assert!((seconds - expected).abs() < 0.05, "{seconds:?}");
    }
    // Each form lasts more than 1.8 times the next, as under the old rules.
    assert!(seconds[0] > seconds[1] * 1.8, "{seconds:?}");
    assert!(seconds[1] > seconds[2] * 1.8, "{seconds:?}");
}

#[test]
fn ejection_protection_covers_a_pod_and_a_pilot_from_the_laser() {
    for target in [Target::Pod, Target::OnFoot] {
        let mut state = aimed(target);
        let tick = state.world.tick;
        state.pilots[1]
            .vitals
            .as_mut()
            .unwrap()
            .protect_ejection(tick);
        for _ in 0..30 {
            shine(&mut state);
            // The beam still ends on the target, charging it nothing.
            let [hit] = state.world.laser_hits[..] else {
                panic!("{target:?}: {:?}", state.world.laser_hits);
            };
            assert_eq!(hit.target, laser_target(target), "{target:?}");
            assert_eq!(hit.damage, 0.0, "{target:?}");
        }
        assert_eq!(health(&state, target), 100.0, "{target:?}");
    }
}

#[test]
fn outside_a_match_the_laser_ends_on_a_pod_and_a_pilot_without_harm() {
    for target in [Target::Pod, Target::OnFoot] {
        // Without a match, pods and pilots are outside the model.
        let flight = SurfaceSortieScenario::init_material_combat_flight(42, &[]);
        let mut state = aim(arrange(flight, target));
        assert!(state.pilots[1].vitals.is_none());
        let life = state.world.ships[1].life;
        for _ in 0..30 {
            shine(&mut state);
            let [hit] = state.world.laser_hits[..] else {
                panic!("{target:?}: {:?}", state.world.laser_hits);
            };
            assert_eq!(hit.target, laser_target(target), "{target:?}");
            assert_eq!(hit.damage, 0.0, "{target:?}");
        }
        assert_eq!(state.world.ships[1].life, life, "{target:?}");
    }
}
