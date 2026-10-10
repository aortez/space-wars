//! Developer-only handoff for terrain/recovery integration. This module cannot
//! enter an application build. The sole intervention replaces one recovery task;
//! it does not reset RulePilotV4, the opponent, or any scenario state.
use super::*;
use engine_common::{DirtMode, MaterialAsteroidSettings, MaterialAsteroidSeverity, Scenario};
use scenario_spacewars::surface_sortie::{
    SurfaceSortieScenario, SurfaceSortieState, pilot::MaterialFlightStart,
};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
    time::Duration,
};

const STEP: Duration = Duration::from_nanos(16_666_667);
const SEED: u64 = 42;
const CONTINUATION_TICKS: u64 = 300 * 60;

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    mode: DirtMode,
    seat: usize,
    blocked_tick: u64,
    reason: &'static str,
    fork_tick: u64,
}

struct Run {
    state: SurfaceSortieState,
    brains: [RulePilotV4; 2],
}

#[derive(Serialize, PartialEq)]
struct Decision {
    sites: [Option<LandingSiteId>; 2],
    observations: [CombatObservationV2; 2],
    brains: [CombatPilotTelemetry; 2],
    actions: Vec<Action>,
}

impl Run {
    fn new(mode: DirtMode) -> Self {
        let mut state = SurfaceSortieScenario::init_material_combat_trial(
            SEED,
            &[0, 1].map(|seat| {
                (
                    PlayerId::from_index(seat).unwrap(),
                    MaterialFlightStart {
                        bearing: if seat == 0 { -0.5 } else { 0.5 },
                        altitude: 90.0,
                        radial_speed: 0.0,
                        lateral_speed: 0.0,
                        heading_offset: 0.0,
                    },
                )
            }),
            [100.0; 2],
        );
        state.enable_dirt_mode(mode);
        state.set_asteroid_pressure(MaterialAsteroidSettings {
            interval_seconds: 1,
            severity: MaterialAsteroidSeverity::Heavy,
        });
        let brains = [0, 1].map(|seat| {
            RulePilotV4::with_combat_breaks(
                BrainReset {
                    actor: PlayerId::from_index(seat).unwrap(),
                    episode_seed: SEED,
                },
                CombatBreakSettings {
                    interval_seconds: 15,
                    duration_seconds: 4,
                },
            )
        });
        Self { state, brains }
    }

    fn duplicate(&self) -> Self {
        Self {
            state: self.state.clone(),
            brains: self.brains.clone(),
        }
    }

    fn decide(&mut self) -> Decision {
        let sites = self.brains.each_ref().map(|b| b.site_request());
        let observations =
            std::array::from_fn(|seat| self.state.combat_observation(seat, sites[seat]));
        let actions = (0..2)
            .flat_map(|seat| {
                self.brains[seat]
                    .intent(&observations[seat])
                    .encode(PlayerId::from_index(seat).unwrap())
            })
            .collect();
        Decision {
            sites,
            observations,
            brains: self.brains.each_ref().map(|b| b.telemetry().clone()),
            actions,
        }
    }

    fn snapshot(&self) -> Value {
        json!({
            "brains": self.brains.each_ref().map(|b| b.telemetry()),
            "pilots": ([0, 1].map(|seat| self.state.combat_observation(seat, self.brains[seat].site_request()))),
            "audit": self.state.terrain_diagnostics(),
            "asteroids": self.state.asteroid_pressure(),
            "scenario": serde_json::from_slice::<Value>(&SurfaceSortieScenario::observe(&self.state).payload).unwrap(),
        })
    }

    fn audit(&self, initial_cells: u64) {
        let a = self.state.terrain_diagnostics();
        assert!(a.issues.is_empty(), "terrain audit: {:?}", a.issues);
        assert_eq!(
            a.occupied_cells + a.loose_cells as u64 + a.removed_cells,
            initial_cells
        );
        assert!(a.max_speed < 500.0, "physical speed alarm: {}", a.max_speed);
    }
}

fn save(path: impl AsRef<Path>, value: &impl Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn line(file: &mut BufWriter<File>, value: &impl Serialize) {
    serde_json::to_writer(&mut *file, value).unwrap();
    writeln!(file).unwrap();
}

// A named same-build comparison checksum, not a cryptographic source identity.
fn fnv64(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

fn save_state(out: &Path, name: &str, run: &Run) {
    save(out.join(format!("{name}.json")), &run.snapshot());
    // Debug includes private controller memory, unlike the public telemetry.
    fs::write(
        out.join(format!("{name}-controllers.txt")),
        format!("{:#?}\n", run.brains),
    )
    .unwrap();
    for seat in 0..2 {
        save(
            out.join(format!("{name}-frame-p{}.json", seat + 1)),
            &SurfaceSortieScenario::player_frame(&run.state, seat),
        );
    }
}

struct Trace {
    actions: BufWriter<File>,
    samples: BufWriter<File>,
    events: BufWriter<File>,
    last_milestones: [Value; 2],
    initial_recoveries: [u32; 2],
    initial_losses: [u64; 2],
    first_completion: [Option<u64>; 2],
    first_new_loss: [Option<u64>; 2],
    first_block: [Option<(u64, Option<&'static str>)>; 2],
}

impl Trace {
    fn new(out: &Path, arm: &str, brains: &[RulePilotV4; 2]) -> Self {
        Self {
            actions: BufWriter::new(
                File::create(out.join(format!("{arm}-actions.jsonl"))).unwrap(),
            ),
            samples: BufWriter::new(
                File::create(out.join(format!("{arm}-samples.jsonl"))).unwrap(),
            ),
            events: BufWriter::new(File::create(out.join(format!("{arm}-events.jsonl"))).unwrap()),
            last_milestones: [Value::Null, Value::Null],
            initial_recoveries: brains.each_ref().map(|b| b.telemetry.completed_recoveries),
            initial_losses: brains.each_ref().map(|b| b.seen_losses),
            first_completion: [None; 2],
            first_new_loss: [None; 2],
            first_block: [None; 2],
        }
    }

    fn decision(&mut self, tick: u64, d: &Decision, run: &Run) {
        line(
            &mut self.actions,
            &json!({
                "tick": tick, "actions": d.actions, "sites": d.sites,
                "decision_fnv64": fnv64(&serde_json::to_vec(d).unwrap()),
                "scenario_before_step_fnv64": fnv64(&SurfaceSortieScenario::observe(&run.state).payload),
            }),
        );
        for seat in 0..2 {
            let b = &d.brains[seat];
            if b.completed_recoveries > self.initial_recoveries[seat] {
                self.first_completion[seat].get_or_insert(tick);
            }
            if run.brains[seat].seen_losses > self.initial_losses[seat] {
                self.first_new_loss[seat].get_or_insert(tick);
            }
            let recovery = b.recovery.as_ref();
            if let Some(t) = recovery.filter(|r| r.status == TaskStatus::Blocked) {
                self.first_block[seat].get_or_insert((tick, t.reason));
            }
            let milestone = json!({
                "completed_recoveries": b.completed_recoveries,
                "losses": run.brains[seat].seen_losses,
                "recovery": recovery.map(|r| json!({
                    "status": r.status, "goal": r.goal, "reason": r.reason,
                    "started_tick": r.started_tick, "landed_tick": r.landed_tick,
                    "exited_tick": r.exited_tick, "rebuilt_tick": r.rebuilt_tick,
                    "completed_tick": r.completed_tick, "scuttle_attempts": r.scuttle_attempts,
                })),
            });
            if milestone != self.last_milestones[seat] {
                line(
                    &mut self.events,
                    &json!({"tick":tick,"seat":seat,"brain":b,"observation":d.observations[seat]}),
                );
                self.last_milestones[seat] = milestone;
            }
        }
    }

    fn summary(&self) -> Value {
        json!({
            "initial_recoveries": self.initial_recoveries,
            "initial_losses": self.initial_losses,
            "first_completion": self.first_completion,
            "first_new_loss": self.first_new_loss,
            "first_block": self.first_block,
        })
    }

    fn flush(&mut self) {
        self.actions.flush().unwrap();
        self.samples.flush().unwrap();
        self.events.flush().unwrap();
    }
}

fn run_case(root: &Path, case: Case) {
    let out = root.join(case.name);
    fs::create_dir(&out).expect("use a fresh output directory to retain earlier evidence");
    let mut control = Run::new(case.mode);
    let initial_cells = control.state.terrain_diagnostics().occupied_cells;
    let mut trace = Trace::new(&out, "control", &control.brains);
    let mut found_block = None;
    for tick in 0..case.fork_tick {
        if tick == case.blocked_tick {
            save_state(&out, "before-block-decision", &control);
        }
        let d = control.decide();
        trace.decision(tick, &d, &control);
        if found_block.is_none()
            && let Some(t) = d.brains[case.seat].recovery.as_ref()
            && t.status == TaskStatus::Blocked
        {
            assert_eq!(tick, case.blocked_tick);
            assert_eq!(t.reason, Some(case.reason));
            found_block = Some(tick);
            save(out.join("blocking-decision.json"), &d);
            save_state(&out, "after-block-decision", &control);
        }
        SurfaceSortieScenario::step(&mut control.state, &d.actions, STEP);
        if (tick + 1) % 60 == 0 {
            control.audit(initial_cells);
            line(
                &mut trace.samples,
                &json!({"tick":tick+1,"state":control.snapshot()}),
            );
        }
    }
    assert_eq!(found_block, Some(case.blocked_tick));
    let recovery = control.brains[case.seat].task.as_ref().unwrap().telemetry();
    assert_eq!(recovery.status, TaskStatus::Blocked);
    assert_eq!(recovery.reason, Some(case.reason));
    save_state(&out, "fork", &control);
    let mut unchanged_clone = control.duplicate();
    let mut reset = control.duplicate();
    assert_eq!(control.snapshot(), unchanged_clone.snapshot());
    assert_eq!(control.snapshot(), reset.snapshot());
    assert_eq!(
        format!("{:?}", control.brains),
        format!("{:?}", reset.brains)
    );

    // This is the entire intervention. All surrounding combat state is retained,
    // including RNG, break clocks, seen ship losses and the previous-tick cache.
    // Internal recovery history is deliberately renewed for this diagnostic;
    // this is not a proposal for production retry eligibility or budgets.
    reset.brains[case.seat].task = Some(RecoverShipTask::new(reset.brains[case.seat].context));
    let mut restored = reset.brains.clone();
    restored[case.seat].task = control.brains[case.seat].task.clone();
    assert_eq!(format!("{:?}", control.brains), format!("{restored:?}"));
    assert_eq!(
        SurfaceSortieScenario::observe(&control.state).payload,
        SurfaceSortieScenario::observe(&reset.state).payload
    );
    save_state(&out, "reset-before-first-decision", &reset);
    let mut reset_trace = Trace::new(&out, "reset", &reset.brains);
    let baseline_recoveries = control
        .brains
        .each_ref()
        .map(|b| b.telemetry.completed_recoveries);
    let end_tick = case.fork_tick + CONTINUATION_TICKS;
    for tick in case.fork_tick..end_tick {
        let d = control.decide();
        let cloned = unchanged_clone.decide();
        assert!(d == cloned, "clone decision diverged at tick {tick}");
        assert!(
            SurfaceSortieScenario::observe(&control.state).payload
                == SurfaceSortieScenario::observe(&unchanged_clone.state).payload,
            "clone scenario observation diverged at tick {tick}"
        );
        let changed = reset.decide();
        trace.decision(tick, &d, &control);
        reset_trace.decision(tick, &changed, &reset);
        if tick == case.fork_tick {
            save(out.join("first-control-decision.json"), &d);
            save(out.join("first-reset-decision.json"), &changed);
        }
        if reset_trace.first_block[case.seat].is_some_and(|b| b.0 == tick) {
            save_state(&out, "reset-first-block", &reset);
        }
        if reset_trace.first_completion[case.seat] == Some(tick) {
            save_state(&out, "reset-first-completion", &reset);
        }
        SurfaceSortieScenario::step(&mut control.state, &d.actions, STEP);
        SurfaceSortieScenario::step(&mut unchanged_clone.state, &cloned.actions, STEP);
        SurfaceSortieScenario::step(&mut reset.state, &changed.actions, STEP);
        if (tick + 1) % 60 == 0 || tick + 1 == end_tick {
            control.audit(initial_cells);
            unchanged_clone.audit(initial_cells);
            reset.audit(initial_cells);
            assert_eq!(
                control.snapshot(),
                unchanged_clone.snapshot(),
                "clone state at {}",
                tick + 1
            );
            assert_eq!(
                format!("{:?}", control.brains),
                format!("{:?}", unchanged_clone.brains)
            );
            line(
                &mut trace.samples,
                &json!({"tick":tick+1,"state":control.snapshot()}),
            );
            line(
                &mut reset_trace.samples,
                &json!({"tick":tick+1,"state":reset.snapshot()}),
            );
        }
    }
    save_state(&out, "control-final", &control);
    save_state(&out, "reset-final", &reset);
    let summary = json!({
        "case":case.name, "seed":SEED, "subject_seat":case.seat,
        "first_block_tick":case.blocked_tick, "blocking_reason":case.reason,
        "fork_tick":case.fork_tick, "end_tick":end_tick,
        "continuation_ticks":CONTINUATION_TICKS, "clone_control_ticks_checked":CONTINUATION_TICKS,
        "policy": RULE_PILOT_V4_POLICY_ID, "break_interval_seconds":15, "break_duration_seconds":4,
        "asteroid_interval_seconds":1, "asteroid_severity":"heavy", "match_finish_rules":false,
        "intervention":"replace subject RecoverShipTask only, before the next decision",
        "baseline_recoveries_at_fork":baseline_recoveries,
        "control":trace.summary(), "reset":reset_trace.summary(),
        "control_final":control.snapshot(), "reset_final":reset.snapshot(),
    });
    save(out.join("summary.json"), &summary);
    trace.flush();
    reset_trace.flush();
    eprintln!(
        "{}: reset completion {:?}, next block {:?}",
        case.name, reset_trace.first_completion[case.seat], reset_trace.first_block[case.seat]
    );
}

#[test]
#[ignore = "developer diagnostic: writes frozen controls and runs sustained heavy combat"]
fn frozen_terrain_recovery_controls() {
    let root = std::env::var_os("SPACEWARS_RECOVERY_OUT")
        .expect("set SPACEWARS_RECOVERY_OUT to a fresh output directory");
    let root = Path::new(&root);
    fs::create_dir_all(root).unwrap();
    let selected = std::env::var("SPACEWARS_RECOVERY_CASE").ok();
    let cases = [
        Case {
            name: "angular-pod-immediate",
            mode: DirtMode::WeakAngular,
            seat: 1,
            blocked_tick: 8064,
            reason: "recovery exceeded two-minute task budget",
            fork_tick: 8065,
        },
        Case {
            name: "round-claim-immediate",
            mode: DirtMode::WeakRound,
            seat: 0,
            blocked_tick: 3071,
            reason: "no supported claim progress",
            fork_tick: 3072,
        },
        Case {
            name: "angular-pod-later",
            mode: DirtMode::WeakAngular,
            seat: 1,
            blocked_tick: 8064,
            reason: "recovery exceeded two-minute task budget",
            fork_tick: 14400,
        },
        Case {
            name: "round-claim-later",
            mode: DirtMode::WeakRound,
            seat: 0,
            blocked_tick: 3071,
            reason: "no supported claim progress",
            fork_tick: 14400,
        },
    ];
    assert!(
        selected
            .as_ref()
            .is_none_or(|name| cases.iter().any(|c| c.name == name))
    );
    for case in cases {
        if selected.as_ref().is_none_or(|name| *name == case.name) {
            run_case(root, case);
        }
    }
}
