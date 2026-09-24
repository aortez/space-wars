use super::*;
use crate::events::ActiveEvent;

fn event(state: &ClockState) -> &MeltdownEvent {
    let Some(ActiveEvent::Meltdown(event)) = &state.active_event else {
        panic!("Meltdown")
    };
    event
}

fn join(state: &mut ClockState) {
    ClockScenario::step(state, &[ClockAction::toggle_player_duck(1)], Duration::ZERO);
}

#[test]
fn promotion_preserves_every_solid_pose_velocity_release_time_and_water_sample() {
    use engine_rapier::world::{BodyId, BodyRole, PhysicsId};
    for aspect in [4.0 / 3.0, 5.0 / 3.0, 0.6] {
        for elapsed in [0, 24, 70, 120, 230, 419, 420, 480, 509] {
            let mut state = ready(aspect, 42);
            for _ in 0..elapsed {
                tick(&mut state);
            }
            assert_eq!(state.body_count(), 0, "standalone stays lightweight");
            let before = event(&state);
            let cells = before.cells.clone();
            let pools = before.water.pools().to_vec();
            let parcels = before.water.parcels().to_vec();
            let stats = before.water.stats();
            let floor = before.floor.as_ref().unwrap().clone();
            let opacity = before.floor_opacity();
            join(&mut state);
            let after = event(&state);
            assert_eq!(after.cells, cells);
            assert_eq!(after.tick, elapsed);
            assert_eq!(after.water.pools(), pools);
            assert_eq!(after.water.parcels(), parcels);
            assert_eq!(after.water.stats(), stats);
            let duck = state.duck_visit.as_ref().unwrap();
            assert_eq!(duck.arena_opacity(), opacity);
            assert_eq!(duck.responsive_floor().unwrap().opening, floor.opening);
            assert_eq!(duck.responsive_floor().unwrap().load, floor.load);
            let world = duck.arena_world();
            let mut released = 0;
            for (index, cell) in cells.iter().enumerate() {
                let id = BodyId::new(PhysicsId::new(4001 + index as u64), BodyRole::PRIMARY);
                if elapsed > cell.release_tick {
                    released += 1;
                    let body = world
                        .motion(id)
                        .expect("already released block is interactive immediately");
                    assert_eq!(body.position, cell.position);
                    assert!((body.angle - cell.angle).sin().abs() < 1e-6);
                    assert_eq!(body.linear_velocity, cell.velocity);
                    assert_eq!(body.angular_velocity, cell.spin);
                } else {
                    assert!(
                        world.motion(id).is_none(),
                        "waiting cell has no collider yet"
                    );
                }
            }
            assert_eq!(
                world.body_count(),
                released + 3,
                "panels + rear wall; no actor before entry"
            );
        }
    }
}

#[test]
fn promoted_meltdown_replays_and_conserves_through_deadline_and_cleanup() {
    for aspect in [4.0 / 3.0, 0.6] {
        for at in [0, 70, 150, 419, 480, 509] {
            let mut a = ready(aspect, 42);
            let mut b = ready(aspect, 42);
            for elapsed in 0..510 {
                if elapsed == at {
                    join(&mut a);
                    join(&mut b);
                }
                let d = a.meltdown_state().unwrap();
                let total = d.solid_microunits
                    + d.pooled_microunits
                    + d.spilling_microunits
                    + d.drained_microunits
                    + d.reclaimed_microunits;
                assert!(total.abs_diff(d.initial_microunits) <= 3, "{d:?}");
                assert_eq!(a.meltdown_state(), b.meltdown_state());
                assert_eq!(a.player_duck_state(), b.player_duck_state());
                assert!(a.body_count() <= 123 && a.collider_count() <= 123);
                assert!(d.spill_parcels <= MAX_SPILL_PARCELS);
                if elapsed % 60 == 0 {
                    assert_eq!(
                        ClockScenario::render_frame(&a),
                        ClockScenario::render_frame(&b)
                    );
                }
                tick(&mut a);
                tick(&mut b);
            }
            assert!(a.active_event.is_none());
            assert!(a.body_count() <= 4);
            a.set_aspect_ratio(2.0);
            assert_eq!((a.body_count(), a.collider_count()), (0, 0));
            assert!(a.duck_visit.is_none());
        }
    }
}
