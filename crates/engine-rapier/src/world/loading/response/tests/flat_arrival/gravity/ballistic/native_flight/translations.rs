use super::*;

#[test]
fn quarter_turn_translated_pair_repeated_returns_match_independent_work_and_ownership() {
    for turn in 0..4 {
        check_pair_repeated_returns_match_independent_work_and_ownership(turn);
    }
}

fn check_pair_repeated_returns_match_independent_work_and_ownership(turn: usize) {
    let mut cases = 0;
    for mass in [5.0, 13.0, 20.0, 80.0] {
        for offset in [
            Vec2::new(-0.125, -16.0),
            Vec2::new(0.125, 16.0),
            Vec2::new(0.0, 3.25),
        ] {
            for hz in [30.0, 240.0] {
                let (mut independent, tokens) =
                    translated_fixture(&[(4, mass), (2, 20.0)], 2, true, 0.0, 4.0, 10.0, offset);
                let mut world = independent.clone();
                turn_world(&mut world, turn);
                let initial = cardinal_mechanical(&world, turn, 10.0);
                let mut tracker = departure::Tracker::default();
                let mut actual = Observed::default();
                let mut expected = Observed::default();
                let mut elapsed = 0.0;
                for _ in 0..1000 {
                    let dt = 1.0 / hz;
                    expected.independent(
                        step_tracked(&mut independent, dt, MAX_ADVANCES, Some(&mut tracker))
                            .unwrap(),
                        elapsed,
                    );
                    actual.native(native_step(&mut world, dt), elapsed);
                    elapsed += f64::from(dt);
                    if actual.impacts.len() == 3 {
                        break;
                    }
                }
                assert_eq!(actual.impacts.len(), 3);
                compare(&actual, &expected, initial);
                let spent: f64 = actual.impacts.iter().map(|i| loss(&i.response)).sum();
                near(
                    cardinal_mechanical(&world, turn, 10.0) + spent,
                    initial,
                    1.0e-5 * initial,
                );
                for (a, b) in balances(&world, &tokens)
                    .iter()
                    .zip(balances(&independent, &tokens))
                {
                    near(*a, b, 1.0e-5 * initial);
                }
                for pair in actual.events.chunks_exact(2) {
                    assert_eq!(pair[0].event.transition, EpisodeTransition::Impact);
                    assert_eq!(pair[1].event.transition, EpisodeTransition::End);
                    assert_eq!(pair[0].event.id, pair[1].event.id);
                }
                assert_eq!(
                    &world.snapshot_bytes().unwrap()[..4],
                    &(if turn == 0 { 10_u32 } else { 12 }).to_le_bytes()
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 24);
}
