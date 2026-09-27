//! First native choice without a destination nomination or control override.
use scenario_spacewars::surface_sortie::{mission::MissionObservationV1, pilot::LandingSiteId};
use serde_json::{Value, json};
use spacewars_ai::{mission_evaluation::MissionEvaluator, mission_policy::MissionBot};
use std::time::Instant;

pub fn timing_enabled() -> bool {
    match super::arg("--probe-neutral-timing", "false").as_str() {
        "true" => true,
        "false" => false,
        _ => panic!("--probe-neutral-timing must be true or false"),
    }
}

pub fn compare(
    bot: &MissionBot,
    o: &MissionObservationV1,
    reference: LandingSiteId,
    timing: bool,
) -> (Value, Option<Value>) {
    let p = &o.local.combat.recovery.flight.pilot;
    let started = Instant::now();
    let (result, estimate) = if timing {
        match bot.landing_choice_with_neutral_timing(o, reference) {
            Ok((c, t)) => (Ok(c), Some(json!({"report":t,"unknown":null}))),
            Err(reason) => (Err(reason), Some(json!({"report":null,"unknown":reason}))),
        }
    } else {
        (bot.landing_choice_comparison(o, reference), None)
    };
    let assessment_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (report, unknown) = match result {
        Ok(report) => (Some(report), None),
        Err(reason) => (None, Some(reason)),
    };
    (
        json!({"reference":reference,"observation_tick":p.tick,"actor":p.owner,
        "report":report,"unknown":unknown,"assessment_ms":assessment_ms,"physics_queries":0}),
        estimate,
    )
}

pub struct NativeCaptureProbe {
    seat: usize,
    choice_ticks: u64,
    timing: bool,
    choice: Option<Value>,
    no_choice: Option<Value>,
    capture: super::capture_probe::CaptureProbe,
}

impl NativeCaptureProbe {
    pub fn from_args() -> Option<Self> {
        let seat = super::arg("--probe-native-capture-seat", "none");
        if seat == "none" {
            return None;
        }
        let seat: usize = seat.parse().expect("native capture seat must be 0 or 1");
        let choice_seconds: u64 = super::arg("--native-choice-seconds", "60").parse().unwrap();
        let capture_seconds: u64 = super::arg("--native-capture-seconds", "120")
            .parse()
            .unwrap();
        let seconds: u64 = super::arg("--seconds", "180").parse().unwrap();
        assert!(seat < 2 && choice_seconds > 0 && choice_seconds + capture_seconds < seconds);
        validate_args(super::arg);
        assert!(
            super::arg("--trace-end-tick", "0").parse::<u64>().unwrap()
                > (choice_seconds + capture_seconds) * 60
        );
        Some(Self {
            seat,
            choice_ticks: choice_seconds * 60,
            timing: timing_enabled(),
            choice: None,
            no_choice: None,
            capture: super::capture_probe::CaptureProbe::new(capture_seconds),
        })
    }

    pub fn record(
        &mut self,
        seat: usize,
        bot: &MissionBot,
        o: &MissionObservationV1,
        evaluator: &MissionEvaluator,
    ) {
        if seat != self.seat || self.done() {
            return;
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let m = bot.telemetry();
        if !self.capture.started() {
            if p.tick >= self.choice_ticks {
                self.no_choice =
                    Some(json!({"tick":p.tick,"reason":"no_native_choice","observed":true}));
                return;
            }
            let Some(a) = m.capture.as_ref().and_then(|c| c.acquisition.as_ref()) else {
                return;
            };
            if a.tick != p.tick || a.reason != "selected_site" {
                return;
            }
            // Reference is the native choice itself, never a supplied candidate.
            let reference = a.selected_site.expect("selected_site has a site");
            let (choice, timing) = compare(bot, o, reference, self.timing);
            self.capture.start(o, m, evaluator, &choice);
            if let Some(timing) = timing {
                self.capture.attach_neutral_timing(timing);
            }
            self.choice = Some(choice);
        }
        if !self.capture.done() {
            self.capture.observe(o, m);
        }
    }

    pub fn done(&self) -> bool {
        self.no_choice.is_some() || self.capture.done()
    }

    pub fn finish(&mut self, tick: u64, match_finished: bool) -> Value {
        if !self.capture.started() && self.no_choice.is_none() {
            self.no_choice = Some(
                json!({"tick":tick,"reason":if match_finished {"match_finished"} else {"runner_ended"},"observed":false}),
            );
        }
        json!({"schema":1,"seat":self.seat,"choice_window_ticks":self.choice_ticks,
            "landing_choice":self.choice,"no_choice":self.no_choice,
            "capture_followthrough":self.capture.finish(tick, match_finished),
            "scope":"First native selected_site in [0, choice_window_ticks), regardless of timing admission. No nomination, search for a supported source, or intent override. Same one-attempt capture observer with timing off/on. Only endpoint command is unexecuted. Source unexposed does not promise an unexposed trip. Diagnostic work outside planner fuel."})
    }
}

fn validate_args(read: impl Fn(&str, &str) -> String) {
    for (arg, default, required) in [
        ("--mode", "quiet", "duel"),
        ("--match", "false", "true"),
        ("--evaluate-missions", "false", "true"),
        ("--trace", "false", "true"),
        ("--trace-start-tick", "0", "0"),
        ("--probe-transfer-destination", "none", "none"),
        ("--continue-successor", "none", "none"),
        ("--probe-ground-start", "false", "false"),
        ("--probe-ground-tick", "none", "none"),
        ("--strike-after-departure", "false", "false"),
        ("--require-finish", "false", "false"),
    ] {
        assert_eq!(
            read(arg, default),
            required,
            "native capture incompatible with {arg}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use scenario_spacewars::{PlayerId, surface_sortie::SurfaceSortieScenario};
    use spacewars_ai::{BrainReset, mission_policy::MissionPolicy};
    use std::time::Duration;

    #[test]
    fn required_flags_use_actual_defaults_and_reject_interventions() {
        for omitted in ["--mode", "--match", "--evaluate-missions", "--trace"] {
            assert!(
                std::panic::catch_unwind(|| validate_args(|arg, default| {
                    if arg == omitted {
                        default.to_owned()
                    } else {
                        match arg {
                            "--mode" => "duel",
                            "--match" | "--evaluate-missions" | "--trace" => "true",
                            _ => default,
                        }
                        .to_owned()
                    }
                }))
                .is_err(),
                "{omitted}"
            );
        }
        assert!(
            std::panic::catch_unwind(|| validate_args(|arg, default| match arg {
                "--mode" => "duel",
                "--match" | "--evaluate-missions" | "--trace" => "true",
                "--probe-transfer-destination" => "0",
                _ => default,
            }
            .to_owned()))
            .is_err()
        );
    }

    #[test]
    fn no_choice_window_and_match_censor_do_not_manufacture_a_source() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let mut o = state.mission_observation(0, None);
        let bot = MissionBot::new(
            MissionPolicy::ValuePlanner,
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: 42,
            },
            Default::default(),
        );
        let evaluator = MissionEvaluator::new(1);
        for finish in [false, true] {
            let mut probe = NativeCaptureProbe {
                seat: 0,
                choice_ticks: 60,
                timing: true,
                choice: None,
                no_choice: None,
                capture: super::super::capture_probe::CaptureProbe::new(120),
            };
            o.local.combat.recovery.flight.pilot.tick = if finish { 59 } else { 60 };
            probe.record(0, &bot, &o, &evaluator);
            assert_eq!(probe.done(), !finish);
            let report = probe.finish(60, finish);
            assert_eq!(
                report["no_choice"]["reason"],
                if finish {
                    "match_finished"
                } else {
                    "no_native_choice"
                }
            );
            assert!(report["capture_followthrough"]["source"].is_null());
            assert!(report["landing_choice"].is_null());
            assert_eq!(report["capture_followthrough"]["observed_rows"], 0);
        }
    }

    #[test]
    fn first_exposed_choice_is_retained_and_window_is_half_open() {
        use engine_core::Vec2;
        use scenario_spacewars::{ShipForm, surface_sortie::combat::CombatTarget};
        for tick in [3599, 3600] {
            let mut state = SurfaceSortieScenario::init_material_combat(42);
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
            let mut o = state.mission_observation(0, None);
            o.opponent = None;
            o.sun = None;
            o.local.sun = None;
            let p = &mut o.local.combat.recovery.flight.pilot;
            p.controls_armed = true;
            p.tick = tick - 1;
            o.planets = vec![p.planet.clone()];
            let mut motion = p.ship;
            motion.position += Vec2::X * 100.0;
            o.local.combat.target = Some(CombatTarget {
                owner: PlayerId::PLAYER_2,
                motion,
                health: 100.0,
                health_fraction: 1.0,
                ship_form: Some(ShipForm::Ship),
                visible: true,
                ground_occluded: false,
            });
            for cover in &mut o.local.cover {
                cover.grounded = true;
                cover.approach = true;
                cover.departure = true;
            }
            let mut bot = MissionBot::new(
                MissionPolicy::ValuePlanner,
                BrainReset {
                    actor: PlayerId::PLAYER_1,
                    episode_seed: 42,
                },
                Default::default(),
            );
            bot.intent(&o);
            o.local.combat.recovery.flight.pilot.tick = tick;
            bot.intent(&o);
            assert_eq!(
                bot.telemetry()
                    .capture
                    .as_ref()
                    .unwrap()
                    .acquisition
                    .as_ref()
                    .unwrap()
                    .reason,
                "selected_site"
            );
            let mut probe = NativeCaptureProbe {
                seat: 0,
                choice_ticks: 3600,
                timing: true,
                choice: None,
                no_choice: None,
                capture: super::super::capture_probe::CaptureProbe::new(120),
            };
            probe.record(0, &bot, &o, &MissionEvaluator::new(1));
            assert_eq!(probe.capture.started(), tick == 3599);
            let report = probe.finish(tick + 1, false);
            if tick == 3599 {
                let t = &report["capture_followthrough"]["source"]["neutral_timing"]["report"];
                assert_eq!(t["unknown"], "source exposed to opponent");
                assert!(t["phases"].is_null());
                assert_eq!(report["capture_followthrough"]["observed_rows"], 1);
            } else {
                assert_eq!(report["no_choice"]["reason"], "no_native_choice");
            }
        }
    }
}
