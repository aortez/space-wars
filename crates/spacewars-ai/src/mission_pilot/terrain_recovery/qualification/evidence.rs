use super::*;

pub(super) fn save(path: impl AsRef<Path>, value: &impl Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

pub(super) fn line(file: &mut BufWriter<File>, value: &impl Serialize) {
    serde_json::to_writer(&mut *file, value).unwrap();
    writeln!(file).unwrap();
}

// Same-build comparison checksum, not a cryptographic source identity.
pub(super) fn fnv64(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}")
}

pub(super) fn snapshot(run: &Run) -> Value {
    json!({
        "brains":run.brains.each_ref().map(|b| b.telemetry()),
        "pilots":([0,1].map(|s| run.state.combat_observation(s,run.brains[s].site_request()))),
        "audit":run.state.terrain_diagnostics(),"match":run.state.match_observation(),
        "scenario":serde_json::from_slice::<Value>(&SurfaceSortieScenario::observe(&run.state).payload).unwrap()
    })
}

pub(super) fn audit(run: &Run, initial_cells: u64) {
    let a = run.state.terrain_diagnostics();
    assert!(a.issues.is_empty(), "terrain audit: {:?}", a.issues);
    assert_eq!(
        a.occupied_cells + a.loose_cells as u64 + a.removed_cells,
        initial_cells
    );
    assert!(a.max_speed < 500.0, "physical speed alarm: {}", a.max_speed);
}

pub(super) fn save_state(out: &Path, name: &str, run: &Run) {
    save(out.join(format!("{name}.json")), &snapshot(run));
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

#[derive(Default, Serialize)]
pub(super) struct SeatMetrics {
    recovery_decisions: u64,
    native_refinement_decisions: u64,
    recovery_blocked_ticks: u64,
    longest_recovery_block: u64,
    block_reasons: BTreeMap<String, u64>,
    mission_goals: BTreeMap<String, u64>,
    mission_blocked_ticks: u64,
    completed: Vec<u64>,
    first_block: Option<u64>,
    first_mission_block: Option<u64>,
}

pub(super) struct Trace {
    actions: BufWriter<File>,
    pub samples: BufWriter<File>,
    pub costs: BufWriter<File>,
    events: BufWriter<File>,
    features: BufWriter<File>,
    milestones: [Value; 2],
    reviews: [Value; 2],
    blocked_streak: [u64; 2],
    pub metrics: [SeatMetrics; 2],
}

impl Trace {
    pub fn new(out: &Path) -> Self {
        let file = |name| BufWriter::new(File::create(out.join(name)).unwrap());
        Self {
            actions: file("actions.jsonl"),
            samples: file("samples.jsonl"),
            costs: file("costs.jsonl"),
            events: file("events.jsonl"),
            features: file("features.jsonl"),
            milestones: [Value::Null, Value::Null],
            reviews: [Value::Null, Value::Null],
            blocked_streak: [0; 2],
            metrics: Default::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn decision(
        &mut self,
        out: &Path,
        case: &Case,
        tick: u64,
        d: &Decision,
        run: &Run,
        had_task: [bool; 2],
        completed: [u32; 2],
    ) {
        let active = run.brains.each_ref().map(|b| b.recovery.is_some());
        let native = [0, 1].map(|s| run.state.rebuild_refinement_enabled(s).unwrap());
        line(
            &mut self.actions,
            &json!({
                "tick":tick,"actions":d.actions,"sites":d.sites,
                "decision_fnv64":fnv64(&serde_json::to_vec(d).unwrap()),
                "scenario_before_step_fnv64":fnv64(&SurfaceSortieScenario::observe(&run.state).payload),
                "recovery_active":active,"native_refinement":native
            }),
        );
        for seat in 0..2 {
            let b = &d.brains[seat];
            let r = b.recovery.as_ref();
            let o = &d.observations[seat].local.combat;
            let p = &o.recovery.flight.pilot;
            let m = &mut self.metrics[seat];
            m.recovery_decisions += u64::from(had_task[seat] || active[seat]);
            m.native_refinement_decisions += u64::from(native[seat]);
            *m.mission_goals
                .entry(
                    serde_json::to_value(b.goal)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                )
                .or_default() += 1;
            if b.goal == MissionGoal::Blocked {
                m.mission_blocked_ticks += 1;
                if m.first_mission_block.is_none() {
                    m.first_mission_block = Some(tick);
                    save_state(out, &format!("first-mission-block-p{}", seat + 1), run);
                }
            }
            if active[seat] && r.is_some_and(|r| r.status == TaskStatus::Blocked) {
                self.blocked_streak[seat] += 1;
                m.recovery_blocked_ticks += 1;
                m.longest_recovery_block = m.longest_recovery_block.max(self.blocked_streak[seat]);
                *m.block_reasons
                    .entry(r.unwrap().reason.unwrap_or("unspecified").to_owned())
                    .or_default() += 1;
                if m.first_block.is_none() {
                    m.first_block = Some(tick);
                    save_state(out, &format!("first-recovery-block-p{}", seat + 1), run);
                }
            } else {
                self.blocked_streak[seat] = 0;
            }
            if b.completed_recoveries > completed[seat] {
                assert_eq!(b.completed_recoveries, completed[seat] + 1);
                assert!(p.ship_available && p.ship_form == ShipForm::Ship);
                assert_eq!(p.location, PilotLocation::Aboard(p.vehicle));
                assert!(!active[seat] && !native[seat] && r.is_none());
                m.completed.push(tick);
                save_state(out, &format!("completion-{tick}-p{}", seat + 1), run);
                save(
                    out.join(format!("completion-{tick}-p{}-decision.json", seat + 1)),
                    d,
                );
            }
            let milestone = json!({"goal":b.goal,"target":b.target,"reason":b.reason,
                "completed_sorties":b.completed_sorties,"completed_recoveries":b.completed_recoveries,
                "losses":p.recovery.as_ref().map(|r|r.ships_lost),"planet":p.planet.index,
                "capture":b.capture.as_ref().map(|c|json!({"goal":c.goal,"failure":c.failure})),
                "recovery":r.map(|r|json!({"status":r.status,"goal":r.goal,"reason":r.reason,
                    "started_tick":r.started_tick,"landed_tick":r.landed_tick,"exited_tick":r.exited_tick,
                    "rebuilt_tick":r.rebuilt_tick,"completed_tick":r.completed_tick,
                    "scuttle_attempts":r.scuttle_attempts,"relocations":r.relocations}))});
            if milestone != self.milestones[seat] {
                line(
                    &mut self.events,
                    &json!({"tick":tick,"seat":seat,"brain":b,"observation":d.observations[seat]}),
                );
                self.milestones[seat] = milestone;
            }
            if let Some(survey) = &o.recovery.rebuild {
                if let Some(work) = &survey.refinement {
                    assert!(work.coarse_candidates <= 8 && work.refined_candidates <= 8);
                    assert!(work.offset_checks <= 240 && work.staging_route_checks <= 16);
                }
                line(
                    &mut self.features,
                    &json!({"tick":tick,"seat":seat,"kind":"survey","survey":survey}),
                );
            }
            if let Some(forecast) = o
                .recovery
                .jetpack
                .as_ref()
                .and_then(|j| j.terrain_flight.as_ref())
            {
                line(
                    &mut self.features,
                    &json!({"tick":tick,"seat":seat,"kind":"terrain_forecast","forecast":forecast}),
                );
            }
            let review = r.and_then(|r| r.pod_reconsideration.as_ref());
            if let Some(review) = review {
                assert_eq!(case.candidate_seat, Some(seat));
                assert!(review.surveys <= 15);
                if let Some(start) = review.started_tick {
                    assert!(start > review.blocked_tick && start <= review.blocked_tick + 900);
                    assert_eq!(review.deadline_tick, Some(start + 5400));
                }
            }
            let value = serde_json::to_value(review).unwrap();
            if value != self.reviews[seat] {
                line(
                    &mut self.features,
                    &json!({"tick":tick,"seat":seat,"kind":"pod_review","review":value,"brain":b}),
                );
                self.reviews[seat] = value;
            }
        }
    }

    pub fn flush(&mut self) {
        for file in [
            &mut self.actions,
            &mut self.samples,
            &mut self.costs,
            &mut self.events,
            &mut self.features,
        ] {
            file.flush().unwrap();
        }
    }
}
