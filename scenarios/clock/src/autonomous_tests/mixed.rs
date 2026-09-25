//! Fast-headless, seeded event sequences. Diagnostics are measurements, not
//! promises that an automatic actor can solve every flooded/blocked course.
use super::*;
use engine_common::{ClockDuckBehavior, ClockDuckJumpProfile, ClockDuckOutcome, ClockDuckState};
use serde::Serialize;

const MAX_TICKS: u64 = 60 * 60;
const LAYOUTS: [(&str, f32); 3] = [
    ("picade", 1024.0 / 768.0),
    ("hyperpixel", 800.0 / 480.0),
    ("portrait", 480.0 / 800.0),
];
const PROFILES: [ClockDuckJumpProfile; 2] =
    [ClockDuckJumpProfile::Careful, ClockDuckJumpProfile::Flowing];

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Sequence {
    Dry,
    Rain,
    RainCleared,
    Mixed,
}

impl Sequence {
    fn name(self) -> &'static str {
        match self {
            Self::Dry => "dry",
            Self::Rain => "rain",
            Self::RainCleared => "rain-cleared",
            Self::Mixed => "mixed",
        }
    }

    fn script(self) -> &'static [(u64, ClockEventKind)] {
        match self {
            Self::Dry => &[],
            Self::Rain => &[(100, ClockEventKind::Rain)],
            Self::RainCleared => &[
                (100, ClockEventKind::Rain),
                (700, ClockEventKind::ColorCycle),
            ],
            Self::Mixed => &[
                (100, ClockEventKind::Rain),
                (700, ClockEventKind::Falling),
                (1060, ClockEventKind::Meltdown),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
struct Case {
    layout: &'static str,
    #[serde(skip)]
    aspect: f32,
    seed: u64,
    profile: ClockDuckJumpProfile,
    amount: ClockRainAmount,
    sequence: Sequence,
}

impl Case {
    fn id(self) -> String {
        format!(
            "{}:{}:{:?}:{}:{}",
            self.layout,
            self.seed,
            self.profile,
            self.amount.label(),
            self.sequence.name()
        )
        .to_ascii_lowercase()
    }
}

#[derive(Debug, Default, Serialize)]
struct Metrics {
    outcome: Option<ClockDuckOutcome>,
    outcome_tick: Option<u64>,
    event_at_outcome: Option<ClockEventKind>,
    wet_at_outcome: bool,
    behavior_at_outcome: Option<ClockDuckBehavior>,
    interruptions: u32,
    recoveries: u32,
    collision: engine_common::ClockDuckRecoveryState,
    paddling_ticks: u64,
    recovering_ticks: u64,
    last_recovery_tick: Option<u64>,
    recovery_to_outcome_ticks: Option<u64>,
    landings: u32,
    post_recovery_landings: u32,
    post_collision_landings: u32,
    longest_dry_no_progress_ticks: u64,
    peak_bodies: usize,
    peak_colliders: usize,
    peak_parcels: usize,
    cleanup_tick: u64,
}

#[derive(Debug, Serialize)]
struct Row {
    case_id: String,
    case: Case,
    metrics: Metrics,
    /// Last live pose/controller state, before reset drops the physical actor.
    terminal_duck: ClockDuckState,
    /// Post-step contact/motion history, capped at two simulated seconds.
    #[serde(skip)]
    trace: std::collections::VecDeque<events::duck::trace::Sample>,
    trace_file: Option<String>,
    course: Vec<[f32; 3]>,
}

#[derive(Debug, Default, Serialize)]
struct Totals {
    visits: usize,
    exited: usize,
    fell: usize,
    timed_out: usize,
    wet_timeouts: usize,
    recovering_timeouts: usize,
    interruptions: u32,
    recoveries: u32,
    post_recovery_landings: u32,
    collision_recoveries: u32,
    escape_jumps: u32,
    post_collision_landings: u32,
    peak_bodies: usize,
    peak_colliders: usize,
    peak_parcels: usize,
}

impl Totals {
    fn of(rows: &[Row]) -> Self {
        let mut totals = Self::default();
        for row in rows {
            let m = &row.metrics;
            totals.visits += 1;
            totals.exited += usize::from(m.outcome == Some(ClockDuckOutcome::Exited));
            totals.fell += usize::from(m.outcome == Some(ClockDuckOutcome::Fell));
            totals.timed_out += usize::from(m.outcome == Some(ClockDuckOutcome::TimedOut));
            totals.wet_timeouts +=
                usize::from(m.outcome == Some(ClockDuckOutcome::TimedOut) && m.wet_at_outcome);
            totals.recovering_timeouts += usize::from(
                m.outcome == Some(ClockDuckOutcome::TimedOut)
                    && m.behavior_at_outcome == Some(ClockDuckBehavior::Recovering),
            );
            totals.interruptions += m.interruptions;
            totals.recoveries += m.recoveries;
            totals.post_recovery_landings += m.post_recovery_landings;
            totals.collision_recoveries += m.collision.recoveries;
            totals.escape_jumps += m.collision.escape_jumps;
            totals.post_collision_landings += m.post_collision_landings;
            totals.peak_bodies = totals.peak_bodies.max(m.peak_bodies);
            totals.peak_colliders = totals.peak_colliders.max(m.peak_colliders);
            totals.peak_parcels = totals.peak_parcels.max(m.peak_parcels);
        }
        totals
    }
}

fn start(case: Case) -> ClockState {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: case.aspect,
            event_profile: ClockEventProfile::Off,
            rain_amount: case.amount,
            duck_jump_profile: Some(case.profile),
            ..Default::default()
        },
        case.seed,
    );
    ClockScenario::step(
        &mut state,
        &[
            ClockAction::set_reading(ClockReading::new(12, 34, 56).unwrap()),
            ClockAction::preview_event(ClockEventKind::Duck),
        ],
        Duration::ZERO,
    );
    state
}

fn check_state(state: &ClockState, case: Case, tick: u64) {
    // One character plus the bounded course and event bodies, not accumulating
    // worlds. Water ledgers are checked even after the character has departed.
    let bound = MAX_MELTDOWN_CELLS + 9;
    assert!(state.body_count() <= bound, "{case:?} tick={tick}");
    assert!(state.collider_count() <= bound, "{case:?} tick={tick}");
    assert!(state.player_duck_session().is_none());
    if let Some(duck) = state.duck_state() {
        if let Some(position) = duck.position_milli {
            assert!(
                position.iter().all(|v| v.abs() < 2_000_000),
                "{case:?} {duck:?}"
            );
        }
        assert!(duck.visit.unwrap().submerged_milli <= 1000);
    }
    if let Some(rain) = state.rain_state() {
        assert!(rain.duck_course && rain.duck_joined);
        assert_eq!(rain.duck_spawns, 0);
        assert!(rain.parcels <= 512);
        let accounted = rain.pooled_microunits
            + rain.in_flight_microunits
            + rain.drained_microunits
            + rain.reclaimed_microunits;
        assert!(
            accounted.abs_diff(rain.injected_microunits) <= 4,
            "{case:?} {rain:?}"
        );
    }
    if let Some(material) = state.meltdown_state() {
        assert!(material.spill_parcels <= MAX_SPILL_PARCELS);
        let accounted = material.solid_microunits
            + material.pooled_microunits
            + material.spilling_microunits
            + material.drained_microunits
            + material.reclaimed_microunits;
        assert!(
            accounted.abs_diff(material.initial_microunits) <= 4,
            "{case:?} {material:?}"
        );
    }
}

fn run(case: Case) -> Row {
    let mut state = start(case);
    let duck = state.duck_visit.as_ref().unwrap();
    let course = duck
        .course
        .as_ref()
        .unwrap()
        .surfaces
        .iter()
        .map(|s| [s.start, s.end, duck.layout.floor_y + s.height])
        .collect();
    let mut replay = start(case);
    let settings = state.settings();
    let mut metrics = Metrics::default();
    let mut previous: Option<ClockDuckState> = None;
    let mut terminal_duck = None;
    let mut progress_x = None;
    let mut stationary_ticks = 0;
    let mut trace = std::collections::VecDeque::with_capacity(120);
    let last_request = case.sequence.script().last().map_or(0, |(tick, _)| *tick);
    for tick in 0..MAX_TICKS {
        for &(_, event) in case.sequence.script().iter().filter(|(at, _)| *at == tick) {
            let before = state.duck_state();
            for scene in [&mut state, &mut replay] {
                ClockScenario::step(scene, &[ClockAction::preview_event(event)], Duration::ZERO);
                assert_eq!(scene.event_kind(), Some(event));
            }
            assert_eq!(
                state.duck_state(),
                before,
                "preview must not reset the visit"
            );
        }
        ticks(&mut state, 1);
        ticks(&mut replay, 1);
        assert_eq!(state.simulation_tick(), tick + 1);
        assert_eq!(state.settings(), settings);
        assert_eq!(
            state.duck_state(),
            replay.duck_state(),
            "{case:?} tick={tick}"
        );
        assert_eq!(
            state.rain_state(),
            replay.rain_state(),
            "{case:?} tick={tick}"
        );
        assert_eq!(
            state.meltdown_state(),
            replay.meltdown_state(),
            "{case:?} tick={tick}"
        );
        assert_eq!(state.event_id(), replay.event_id());
        assert_eq!(state.event_phase(), replay.event_phase());
        assert_eq!(state.phase_tick(), replay.phase_tick());
        assert_eq!(state.lifecycle(), replay.lifecycle());
        assert_eq!(state.floor_mode(), replay.floor_mode());
        assert_eq!(state.body_count(), replay.body_count());
        assert_eq!(state.collider_count(), replay.collider_count());
        assert_eq!(state.segments(), replay.segments(), "{case:?} tick={tick}");
        check_state(&state, case, tick);
        if metrics.outcome.is_none()
            && let Some(sample) = state.duck_visit.as_ref().and_then(|d| d.trace_sample())
        {
            if trace.len() == 120 {
                trace.pop_front();
            }
            trace.push_back(sample);
        }
        metrics.peak_bodies = metrics.peak_bodies.max(state.body_count());
        metrics.peak_colliders = metrics.peak_colliders.max(state.collider_count());
        metrics.peak_parcels = metrics.peak_parcels.max(
            state
                .rain_state()
                .map_or(0, |r| r.parcels)
                .max(state.meltdown_state().map_or(0, |m| m.spill_parcels)),
        );
        if let Some(duck) = state.duck_state() {
            let nav = duck.navigation.unwrap();
            metrics.collision = nav.recovery;
            let landings = nav.planning.unwrap().confirmed_landings;
            if nav.water.recoveries > 0 {
                metrics.post_recovery_landings += landings.saturating_sub(metrics.landings);
            }
            if nav.recovery.recoveries > 0 {
                metrics.post_collision_landings += landings.saturating_sub(metrics.landings);
            }
            if previous.is_some_and(|previous| duck.jumps > previous.jumps) {
                assert!(
                    previous.unwrap().grounded,
                    "{case:?} tick={tick}: airborne jump"
                );
            }
            metrics.landings = landings;
            metrics.interruptions = nav.water.interruptions;
            if nav.water.recoveries > metrics.recoveries {
                metrics.last_recovery_tick = Some(tick + 1);
            }
            metrics.recoveries = nav.water.recoveries;
            metrics.paddling_ticks = nav.water.paddling_ticks;
            metrics.recovering_ticks = nav.water.recovering_ticks;
            if metrics.outcome.is_none() && duck.outcome.is_some() {
                metrics.outcome = duck.outcome;
                metrics.outcome_tick = Some(tick + 1);
                metrics.event_at_outcome = state.event_kind();
                metrics.wet_at_outcome =
                    previous.is_some_and(|d| d.visit.unwrap().submerged_milli >= 150);
                metrics.behavior_at_outcome =
                    previous.and_then(|d| d.navigation).map(|n| n.behavior);
                metrics.recovery_to_outcome_ticks =
                    metrics.last_recovery_tick.map(|t| tick + 1 - t);
                terminal_duck = previous;
            }
            if let Some([x, _]) = duck.position_milli
                && !matches!(
                    nav.behavior,
                    ClockDuckBehavior::Paddling | ClockDuckBehavior::Recovering
                )
            {
                // Translation by a body diameter counts as progress. In-place
                // warm-up jumps still count as stationary; this is diagnostic,
                // not a flaky wall-time or blanket "stuck" assertion.
                if progress_x
                    .is_none_or(|start: i32| start.abs_diff(x) >= nav.body_radius_milli * 2)
                {
                    progress_x = Some(x);
                    stationary_ticks = 0;
                } else {
                    stationary_ticks += 1;
                }
                metrics.longest_dry_no_progress_ticks =
                    metrics.longest_dry_no_progress_ticks.max(stationary_ticks);
            } else {
                progress_x = None;
                stationary_ticks = 0;
            }
            previous = Some(duck);
        }
        if tick > last_request
            && !state.has_duck_visit()
            && state.lifecycle() == EventLifecycle::Idle
        {
            assert!(state.event_kind().is_none());
            assert_eq!((state.body_count(), state.collider_count()), (0, 0));
            assert_eq!(state.floor_mode(), engine_common::ClockFloorMode::Closed);
            assert!(metrics.outcome.is_some(), "{case:?}: no departure outcome");
            metrics.cleanup_tick = tick + 1;
            return Row {
                case_id: case.id(),
                case,
                metrics,
                terminal_duck: terminal_duck.expect("live actor before reset"),
                trace,
                trace_file: None,
                course,
            };
        }
    }
    panic!("{case:?}: did not clean up in {MAX_TICKS} ticks; {metrics:?}");
}

fn suite(name: &str, cases: impl IntoIterator<Item = Case>) -> Vec<Row> {
    let cases: Vec<_> = cases.into_iter().collect();
    let expected_visits = cases.len();
    let mut rows = Vec::new();
    for case in cases {
        eprintln!("starting {}", case.id());
        let mut row = run(case);
        eprintln!(
            "{}",
            serde_json::json!({"case": row.case, "metrics": row.metrics})
        );
        if let Some(directory) = std::env::var_os("SPACEWARS_CLOCK_REGRESSION_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let filename = format!("{name}-{}.trace.json", row.case_id.replace(':', "-"));
            let trace = serde_json::json!({
                "schema_version": 1,
                "case_id": row.case_id,
                "coordinate_system": "entrance-relative-x/world-y",
                "course": row.course,
                "radius": row.terminal_duck.navigation.unwrap().body_radius_milli as f32 / 1000.0,
                "outcome": row.metrics.outcome,
                "steps": row.trace,
            });
            std::fs::write(
                directory.join(&filename),
                serde_json::to_vec_pretty(&trace).unwrap(),
            )
            .unwrap();
            row.trace_file = Some(filename);
            rows.push(row);
            let report = serde_json::json!({
                "schema_version": 2,
                "suite": name,
                "fixed_hz": FIXED_HZ,
                "max_ticks": MAX_TICKS,
                "expected_visits": expected_visits,
                "completed_visits": rows.len(),
                "summary": Totals::of(&rows),
                "rows": rows,
            });
            std::fs::write(
                directory.join(format!("{name}.json")),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
        } else {
            rows.push(row);
        }
    }
    eprintln!(
        "{name}: {}",
        serde_json::to_string(&Totals::of(&rows)).unwrap()
    );
    rows
}

#[test]
fn mixed_event_collision_recovery() {
    // Both fell in the pre-recovery baseline. The first was stranded on loose
    // digit debris above a gap; the second lost its trajectory in a collision.
    let rows = suite(
        "collision-recovery",
        [
            Case {
                layout: "picade",
                aspect: 4.0 / 3.0,
                seed: 42,
                profile: ClockDuckJumpProfile::Careful,
                amount: ClockRainAmount::Light,
                sequence: Sequence::Mixed,
            },
            Case {
                layout: "hyperpixel",
                aspect: 800.0 / 480.0,
                seed: 0,
                profile: ClockDuckJumpProfile::Flowing,
                amount: ClockRainAmount::Light,
                sequence: Sequence::Mixed,
            },
        ],
    );
    for row in rows {
        assert_eq!(
            row.metrics.outcome,
            Some(ClockDuckOutcome::Exited),
            "{:?}",
            row.metrics
        );
        assert!(row.metrics.collision.recoveries > 0);
        assert!(row.metrics.post_collision_landings > 0);
    }
}

#[test]
fn mixed_event_clear_weather_recovers() {
    let rows = suite(
        "recovery-smoke",
        [
            Case {
                layout: "picade",
                aspect: 4.0 / 3.0,
                seed: 42,
                profile: ClockDuckJumpProfile::Careful,
                amount: ClockRainAmount::Heavy,
                sequence: Sequence::RainCleared,
            },
            Case {
                layout: "portrait",
                aspect: 0.6,
                seed: 42,
                profile: ClockDuckJumpProfile::Flowing,
                amount: ClockRainAmount::Heavy,
                sequence: Sequence::RainCleared,
            },
        ],
    );
    for row in rows {
        assert_eq!(
            row.metrics.outcome,
            Some(ClockDuckOutcome::Exited),
            "{row:?}"
        );
        assert!(
            row.metrics.recoveries > 0 && row.metrics.post_recovery_landings > 0,
            "{row:?}"
        );
    }
}

#[test]
fn mixed_event_smoke() {
    let rows = suite(
        "mixed-smoke",
        LAYOUTS.into_iter().flat_map(|(layout, aspect)| {
            PROFILES.map(|profile| Case {
                layout,
                aspect,
                seed: 42,
                profile,
                amount: ClockRainAmount::Heavy,
                sequence: Sequence::Mixed,
            })
        }),
    );
    assert_eq!(rows.len(), 6);
    for row in &rows {
        assert!(row.metrics.interruptions > 0, "{row:?}");
        assert!(row.metrics.recoveries > 0, "{row:?}");
        assert!(row.metrics.post_recovery_landings > 0, "{row:?}");
        // A deterministic behavioral guard, not an execution-time assertion.
        assert!(
            row.metrics.longest_dry_no_progress_ticks < 5 * 60,
            "{row:?}"
        );
        assert_ne!(
            row.metrics.outcome,
            Some(ClockDuckOutcome::TimedOut),
            "{row:?}"
        );
    }
    assert!(Totals::of(&rows).exited >= 4);
}

#[test]
#[ignore = "optional 240-visit seeded mixed-event sweep; run with --ignored --nocapture"]
fn mixed_event_sweep() {
    let cases = LAYOUTS.into_iter().flat_map(|(layout, aspect)| {
        PROFILES.into_iter().flat_map(move |profile| {
            [0, 1, 7, 42].into_iter().flat_map(move |seed| {
                let base = Case {
                    layout,
                    aspect,
                    profile,
                    seed,
                    amount: ClockRainAmount::Light,
                    sequence: Sequence::Dry,
                };
                std::iter::once(base).chain(
                    [
                        ClockRainAmount::Light,
                        ClockRainAmount::Medium,
                        ClockRainAmount::Heavy,
                    ]
                    .into_iter()
                    .flat_map(move |amount| {
                        [Sequence::Rain, Sequence::RainCleared, Sequence::Mixed].map(|sequence| {
                            Case {
                                amount,
                                sequence,
                                ..base
                            }
                        })
                    }),
                )
            })
        })
    });
    let mut cases: Vec<_> = cases.collect();
    assert_eq!(cases.len(), 240);
    if let Ok(id) = std::env::var("SPACEWARS_CLOCK_REGRESSION_CASE") {
        cases.retain(|case| case.id() == id);
        assert_eq!(cases.len(), 1, "unknown or ambiguous case ID: {id}");
    }
    let rows = suite("mixed-sweep", cases);
    for row in rows
        .iter()
        .filter(|row| matches!(row.case.sequence, Sequence::Dry | Sequence::RainCleared))
    {
        assert_eq!(
            row.metrics.outcome,
            Some(ClockDuckOutcome::Exited),
            "dry/clear-weather control: {row:?}"
        );
        assert_eq!(
            row.metrics.collision.ticks, 0,
            "recovery must leave the dry personalities alone"
        );
    }
}
