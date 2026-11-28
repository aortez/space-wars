use super::*;

#[test]
fn sorted_terms_keep_each_primary_goal_ahead_of_all_lower_ones() {
    let progress = |outcome, wall_tags, cleared, tick| {
        Score::from_terminal(DuckProgress {
            tick,
            wall_tags,
            cleared,
            landings: 0,
            outcome: Some(outcome),
        })
    };
    let scores = [
        progress(ClockDuckOutcome::Fell, [0, 0], 0, 1),
        progress(ClockDuckOutcome::Fell, [0, 0], 0, 2000),
        progress(ClockDuckOutcome::TimedOut, [0, 0], 0, 2000),
        progress(ClockDuckOutcome::Fell, [0, 0], 5, 100),
        progress(ClockDuckOutcome::Fell, [1, 0], 0, 100),
        progress(ClockDuckOutcome::Exited, [0, 0], 0, 1),
    ];
    assert!(scores.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(
        scores.into_iter().sum::<Score>().terms(),
        [1, 1, 5, 2, 4202]
    );
}

#[test]
fn seeded_case_partitions_and_mirrored_worlds_are_reproducible() {
    let cases = training_cases(1, 1, &PATTERNS, 64).unwrap();
    assert_eq!(cases, training_cases(1, 1, &PATTERNS, 64).unwrap());
    assert_ne!(cases, training_cases(1, 2, &PATTERNS, 64).unwrap());
    assert!(
        PATTERNS
            .iter()
            .all(|p| cases.iter().any(|c| c.pattern == *p))
    );
    assert!(
        cases
            .chunks_exact(2)
            .all(|p| p[0].entrance_right != p[1].entrance_right)
    );
    assert!(
        cases
            .iter()
            .any(|c| c.clock_second != cases[0].clock_second)
    );
    let validation = held_out_cases(1, Partition::Validation, 12);
    let qualification = held_out_cases(1, Partition::Qualification, 12);
    assert_eq!(
        qualification,
        held_out_cases(1, Partition::Qualification, 12)
    );
    for [left, right] in &validation {
        assert!(!left.entrance_right && right.entrance_right);
        assert_eq!(
            *left,
            DuckCase {
                entrance_right: false,
                ..*right
            }
        );
        assert!(!cases.iter().any(|c| c.seed == left.seed));
        assert!(!qualification.iter().any(|q| q[0].seed == left.seed));
    }
}

#[test]
fn random_controls_replay_and_the_scripted_baseline_repeats_from_both_doors() {
    let pairs = held_out_cases(1, Partition::Validation, 1);
    let scripted = || {
        suite(&pairs, None, |case| {
            run_reference(case, Reference::Scripted, false)
        })
        .unwrap()
    };
    let result = scripted();
    assert_eq!(result, scripted());
    assert_eq!(result.episodes, 2);
    assert_eq!(result.score.terms(), [1, 2, 12, 1, 1378]);
    let pair = &result.pairs[0];
    assert_eq!(result.score, pair.left.score.min(pair.right.score));
    for case in pairs[0] {
        let visit = run_reference(case, Reference::Random, true).unwrap();
        visit.validate_replay().unwrap();
        let mut replay = DuckEpisode::new(case, visit.driver()).unwrap();
        while replay.progress().outcome.is_none() {
            replay.step(visit.control_at(replay.progress().tick));
        }
        assert_eq!(replay.progress(), visit.progress);
    }
}

#[test]
fn sampled_jump_inputs_reach_narrow_landings_and_exit_from_both_doors() {
    for entrance_right in [false, true] {
        let case = DuckCase {
            pattern: ClockDuckCoursePattern::Platforms,
            entrance_right,
            ..DuckCase::seeded(0)
        };
        let run = |cadence: u64, pulse: u64| {
            let mut visit = DuckEpisode::new(case, DuckDriver::External).unwrap();
            let mut held = DuckControls::default();
            loop {
                let tick = visit.progress().tick;
                if tick.is_multiple_of(cadence) && tick > 1250 {
                    held = DuckControls {
                        move_milli: if entrance_right { -1000 } else { 1000 },
                        jump: tick % 15 < pulse,
                        run: true,
                        dive: false,
                    };
                }
                let progress = visit.step(held);
                if progress.outcome.is_some() {
                    return progress;
                }
            }
        };
        // A six-tick button hold on this narrow obstacle can reach the farther
        // platform when sampled every third tick. Rounding its edges to the
        // next sixth tick misses the landing.
        assert_eq!(run(1, 6), run(3, 6));
        assert_eq!(run(3, 6).outcome, Some(ClockDuckOutcome::Exited));
        assert_eq!(run(6, 6).outcome, Some(ClockDuckOutcome::Fell));
        let every_tick = run(1, 1);
        let every_decision = run(u64::from(DECISION_TICKS), 1);
        for result in [every_tick, every_decision] {
            assert_eq!(
                result.outcome,
                Some(ClockDuckOutcome::Exited),
                "{entrance_right}: {every_tick:?}, {every_decision:?}"
            );
            assert!(result.landings > 0 && result.cleared > 0);
        }
    }
}

#[test]
fn malformed_outputs_release_controls_and_count_the_invalid_decisions() {
    let case = DuckCase::seeded(3);
    let episode = DuckEpisode::new(case, DuckDriver::External).unwrap();
    let mut decisions = 0;
    let visit = run(episode, case, None, true, |_| {
        decisions += 1;
        Ok(CreatureControls::decode(
            &[1., 1., f32::NAN, 1.],
            &BodyProfile::duck(),
        ))
    })
    .unwrap();
    assert!(decisions > 0);
    assert_eq!(visit.invalid_decisions, decisions);
    assert!(
        visit
            .controls
            .iter()
            .all(|held| *held == DuckControls::default())
    );
    visit.validate_replay().unwrap();
    let mut damaged = visit;
    damaged.controls.pop();
    assert!(damaged.validate_replay().is_err());
}
