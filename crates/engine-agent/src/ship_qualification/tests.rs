use super::*;

#[test]
fn schedule_and_gate_match_the_plan_saved_before_evaluation() {
    let frozen: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/data/ship-arrival-qualification-v1-plan.json"
    ))
    .unwrap();
    let current = manifest().unwrap();
    assert_eq!(frozen["id"], current.id);
    assert_eq!(frozen["candidate_id"], current.candidate_id);
    assert_eq!(frozen["reference_id"], current.reference_id);
    assert_eq!(frozen["evaluator_parameters"], current.evaluator_parameters);
    assert_eq!(frozen["gate"], current.gate);
    assert_eq!(
        frozen["required_fresh_arrivals"],
        current.required_fresh_arrivals
    );
    assert_eq!(
        frozen["required_regression_arrivals"],
        current.required_regression_arrivals
    );
    let expected = frozen["fresh_cases"].as_array().unwrap();
    assert_eq!(expected.len(), current.fresh_cases.len());
    for (saved, scheduled) in expected.iter().zip(current.fresh_cases) {
        // Read physical quantities at their declared f32 precision.
        let case: ArrivalCase = serde_json::from_value(saved["case"].clone()).unwrap();
        assert_eq!(case, scheduled.case);
        assert_eq!(
            saved["group"],
            serde_json::to_value(scheduled.group).unwrap()
        );
    }
    let expected: Vec<ArrivalCase> =
        serde_json::from_value(frozen["regression_cases"].clone()).unwrap();
    assert_eq!(expected, current.regression_cases);
    // Executable identity is intentionally build-specific and is recorded in
    // each run, rather than compared with the historical build in routine CI.
}

#[test]
fn fresh_schedule_is_balanced_and_disjoint_from_all_development_cases() {
    let cases = cases();
    assert_eq!(cases.len(), FRESH_CASES);
    assert_eq!(regressions().len(), REGRESSION_CASES);
    for group in [
        Group::Geometry,
        Group::InitialMotion,
        Group::TargetMotion,
        Group::Combined,
    ] {
        assert_eq!(cases.iter().filter(|c| c.group == group).count(), 24);
    }
    let mut all = cases.iter().map(|c| c.case.clone()).collect::<Vec<_>>();
    all.extend(regressions());
    for (index, case) in all.iter().enumerate() {
        for other in &all[index + 1..] {
            assert_ne!(case.name, other.name);
            let mut renamed = other.clone();
            renamed.name = case.name.clone();
            assert_ne!(case, &renamed, "overlapping physical case: {}", case.name);
        }
    }
    assert!(
        cases
            .iter()
            .all(|c| c.case.seed == 42 && c.case.max_ticks == 1200)
    );
    assert_eq!(candidate().unwrap().id(), CANDIDATE);
}

#[test]
fn every_fresh_case_has_a_valid_start_and_marker_path() {
    // Preflight geometry using idle controls, without inspecting or tuning the
    // frozen candidate or reference against the new cases.
    for scheduled in cases() {
        let name = scheduled.case.name.clone();
        ship_arrival::run_episode(scheduled.case, Policy::Idle, false)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn paired_counts_keep_reference_failures_and_pass_gate_requires_every_case() {
    use ArrivalOutcome::{Arrived, InvalidOutput, OutsideRegion, Timeout};
    let mut counts = Counts::default();
    for (candidate, reference) in [
        (Arrived, Arrived),
        (Arrived, Timeout),
        (Timeout, Arrived),
        (OutsideRegion, InvalidOutput),
    ] {
        counts.record(candidate, reference);
    }
    assert_eq!(
        counts,
        Counts {
            cases: 4,
            both_arrived: 1,
            candidate_only: 1,
            reference_only: 1,
            neither: 1
        }
    );
    assert!(!passes(&Counts::default(), &Counts::default()));
    let mut fresh = Counts {
        cases: FRESH_CASES,
        candidate_only: FRESH_CASES,
        ..Counts::default()
    };
    let mut regression = Counts {
        cases: REGRESSION_CASES,
        both_arrived: REGRESSION_CASES,
        ..Counts::default()
    };
    // Qualification is absolute arrival, not a race against the reference.
    assert!(passes(&fresh, &regression));
    fresh.candidate_only -= 1;
    fresh.neither += 1;
    assert!(!passes(&fresh, &regression));
    fresh.candidate_only += 1;
    fresh.neither -= 1;
    regression.both_arrived -= 1;
    regression.reference_only += 1;
    assert!(!passes(&fresh, &regression));
}
