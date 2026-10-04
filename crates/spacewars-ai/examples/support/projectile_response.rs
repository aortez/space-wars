//! One explicitly selected, bounded laboratory response; never a default policy.
use scenario_spacewars::{
    DebrisId, ShipForm,
    surface_sortie::{
        LandingPhase, PilotLocation, SurfaceSortieState, VehicleId, mission::MissionObservationV1,
        projectile_diagnostics::ProjectileDiagnostics,
    },
};
use serde::Serialize;
use serde_json::json;
use spacewars_ai::{
    combat_pilot::CombatIntent,
    mission_pilot::{MissionGoal, MissionTelemetry},
};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};

const RESPONSE_TICKS: u64 = 30;

fn response_seat(value: &str, reporting_seat: usize) -> usize {
    match value {
        "reporting" => reporting_seat,
        "0" => 0,
        "1" => 1,
        _ => panic!("--probe-projectile-response-seat must be reporting, 0 or 1"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Observe,
    Brake,
    Left,
    Right,
    GuardedBrake,
}

#[path = "projectile_brake_selection.rs"]
mod brake_selection;
use brake_selection::BrakeSelection;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct TransferKey {
    started_tick: u64,
    deadline_tick: u64,
    selected_tick: u64,
    vehicle: VehicleId,
    destination: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(untagged)]
enum ResponseKey {
    Escape(TransferKey),
    Native {
        goal_since: u64,
        vehicle: VehicleId,
        destination: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Escape,
    Transfer,
}

fn response_scope(value: &str) -> Scope {
    match value {
        "escape" => Scope::Escape,
        "transfer" => Scope::Transfer,
        _ => panic!("--probe-projectile-response-scope must be escape or transfer"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct Threat {
    id: DebrisId,
    spawn_tick: u64,
    entry_seconds: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct Attempt {
    source: ResponseKey,
    started_tick: u64,
    end_tick: u64,
    threat: Threat,
    finished_tick: Option<u64>,
    reason: Option<&'static str>,
    applied_ticks: u32,
}

fn ready(o: &MissionObservationV1, m: &MissionTelemetry) -> Option<TransferKey> {
    let f = &o.local.combat.recovery.flight;
    let p = &f.pilot;
    let a = m.escape_travel.as_ref()?.last?;
    let key = TransferKey {
        started_tick: a.started_tick,
        deadline_tick: a.deadline_tick,
        selected_tick: a.selected_tick?,
        vehicle: a.vehicle,
        destination: a.destination?,
    };
    (o.match_rules
        && !o.match_context.as_ref().is_some_and(|c| c.finished)
        && m.goal == MissionGoal::Transfer
        && m.capture.is_none()
        && m.recovery.is_none()
        && m.target == Some(key.destination)
        && a.finished_tick.is_none()
        && a.observed_tick == p.tick
        && p.tick < a.deadline_tick
        && p.controls_armed
        && p.queries_ready
        && f.flight.enabled
        && p.ship_available
        && p.ship_form == ShipForm::Ship
        && p.vehicle == key.vehicle
        && matches!(p.location, PilotLocation::Aboard(_))
        && p.landing.phase == LandingPhase::Flying
        && p.landing.supported_feet == 0)
        .then_some(key)
}

fn response_key(
    scope: Scope,
    o: &MissionObservationV1,
    m: &MissionTelemetry,
) -> Option<ResponseKey> {
    if let Some(key) = ready(o, m) {
        return Some(ResponseKey::Escape(key));
    }
    let f = &o.local.combat.recovery.flight;
    let p = &f.pilot;
    let destination = m.target?;
    (scope == Scope::Transfer
        && o.match_rules
        && !o.match_context.as_ref().is_some_and(|c| c.finished)
        && m.goal == MissionGoal::Transfer
        && m.capture.is_none()
        && m.recovery.is_none()
        && p.controls_armed
        && p.queries_ready
        && f.flight.enabled
        && p.ship_available
        && p.ship_form == ShipForm::Ship
        && matches!(p.location, PilotLocation::Aboard(_))
        && p.landing.phase == LandingPhase::Flying
        && p.landing.supported_feet == 0)
        .then_some(ResponseKey::Native {
            goal_since: m.goal_since,
            vehicle: p.vehicle,
            destination,
        })
}

// Same two-second, constant-relative-velocity circle screen as the frozen
// offline diagnostic. f64 arithmetic consumes the serialized f32 observations.
fn circle_entry(
    position: engine_core::Vec2,
    velocity: engine_core::Vec2,
    radius: f64,
) -> Option<f64> {
    let [x, y, vx, vy] = [position.x, position.y, velocity.x, velocity.y].map(f64::from);
    let distance2 = x * x + y * y;
    if distance2 <= radius * radius {
        return Some(0.0);
    }
    let speed2 = vx * vx + vy * vy;
    let dot = x * vx + y * vy;
    let discriminant = dot * dot - speed2 * (distance2 - radius * radius);
    if speed2 <= 1.0e-10 || dot >= 0.0 || discriminant < 0.0 {
        return None;
    }
    let entry = (-dot - discriminant.sqrt()) / speed2;
    (0.0..=2.0).contains(&entry).then_some(entry)
}

fn first_threat(d: &ProjectileDiagnostics) -> Option<Threat> {
    d.projectiles
        .iter()
        .filter_map(|p| {
            circle_entry(
                p.relative_position,
                p.relative_velocity,
                f64::from(d.observer_radius) + f64::from(p.collision_radius),
            )
            .map(|entry_seconds| Threat {
                id: p.id,
                spawn_tick: p.spawn_tick,
                entry_seconds,
            })
        })
        .min_by(|a, b| {
            a.entry_seconds
                .total_cmp(&b.entry_seconds)
                .then_with(|| a.id.cmp(&b.id))
        })
}

fn apply(mode: Mode, intent: &mut CombatIntent) {
    let c = &mut intent.flight.controls;
    match mode {
        Mode::Observe => return,
        Mode::Brake => {
            c.primary_held = false;
            c.brake_held = true;
        }
        Mode::Left | Mode::Right => {
            c.horizontal = if mode == Mode::Left { -1.0 } else { 1.0 };
            // Preserve native speed/safety braking rather than opposing it with thrust.
            c.primary_held = !c.brake_held;
        }
        Mode::GuardedBrake => unreachable!("selector must choose a concrete action"),
    }
    intent.flight.wings.closed = false;
}

#[derive(Debug, Clone)]
struct Pulse {
    mode: Mode,
    selected_mode: Option<Mode>,
    last_tick: Option<u64>,
    attempt: Option<Attempt>,
}
impl Pulse {
    fn step(
        &mut self,
        tick: u64,
        key: Option<ResponseKey>,
        threat: Option<Threat>,
        intent: &mut CombatIntent,
    ) -> bool {
        assert!(
            self.last_tick.is_none_or(|last| tick > last),
            "one forward observation per tick"
        );
        self.last_tick = Some(tick);
        if self.attempt.is_none()
            && let (Some(source), Some(threat)) = (key, threat)
        {
            self.attempt = Some(Attempt {
                source,
                started_tick: tick,
                end_tick: tick + RESPONSE_TICKS,
                threat,
                finished_tick: None,
                reason: None,
                applied_ticks: 0,
            });
        }
        let Some(a) = &mut self.attempt else {
            return false;
        };
        if a.finished_tick.is_some() {
            return false;
        }
        if tick >= a.end_tick || key != Some(a.source) {
            a.finished_tick = Some(tick);
            a.reason = Some(if tick >= a.end_tick {
                "pulse complete"
            } else {
                "native priority or transfer identity changed"
            });
            return false;
        }
        let mode = if self.mode == Mode::GuardedBrake {
            self.selected_mode.expect("first warning selects once")
        } else {
            self.mode
        };
        if mode == Mode::Observe {
            return false;
        }
        apply(mode, intent);
        a.applied_ticks += 1;
        true
    }
}

pub struct ResponseProbe {
    seat: usize,
    scope: Scope,
    pulse: Pulse,
    selection: Option<BrakeSelection>,
    log: BufWriter<File>,
}
impl ResponseProbe {
    pub fn from_args(out: &Path, seat: usize) -> Option<Self> {
        let mode = match crate::arg("--probe-projectile-response", "none").as_str() {
            "none" => return None,
            "observe" => Mode::Observe,
            "brake" => Mode::Brake,
            "left" => Mode::Left,
            "right" => Mode::Right,
            "guarded_brake" => Mode::GuardedBrake,
            _ => panic!(
                "--probe-projectile-response must be none, observe, brake, left, right or guarded_brake"
            ),
        };
        assert_eq!(crate::arg("--mode", "quiet"), "duel");
        assert_eq!(crate::arg("--trace-capture-evidence", "false"), "true");
        assert_eq!(crate::arg("--trace-projectiles", "false"), "true");
        assert_eq!(crate::arg("--trace-impact", "false"), "false");
        assert_eq!(crate::arg("--continue-successor", "none"), "none");
        Some(Self {
            scope: response_scope(&crate::arg("--probe-projectile-response-scope", "escape")),
            seat: response_seat(
                &crate::arg("--probe-projectile-response-seat", "reporting"),
                seat,
            ),
            pulse: Pulse {
                mode,
                selected_mode: None,
                last_tick: None,
                attempt: None,
            },
            selection: None,
            log: BufWriter::new(
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(out.join("projectile-response.jsonl"))
                    .unwrap(),
            ),
        })
    }

    pub fn observe(
        &mut self,
        seat: usize,
        state: &SurfaceSortieState,
        o: &MissionObservationV1,
        m: &MissionTelemetry,
        intent: &mut CombatIntent,
    ) {
        if seat != self.seat {
            return;
        }
        let p = &o.local.combat.recovery.flight.pilot;
        let key = response_key(self.scope, o, m);
        let diagnostic = if key.is_some() && self.pulse.attempt.is_none() {
            let d = state
                .projectile_diagnostics(seat)
                .expect("ready physical observer");
            assert_eq!(d.tick, p.tick);
            assert_eq!(d.observer, p.ship);
            Some(d)
        } else {
            None
        };
        let threat = diagnostic.as_ref().and_then(first_threat);
        let active = self
            .pulse
            .attempt
            .is_some_and(|a| a.finished_tick.is_none());
        let original = *intent;
        let choosing = self.pulse.mode == Mode::GuardedBrake
            && self.pulse.attempt.is_none()
            && threat.is_some();
        if choosing {
            let selection = brake_selection::select(o, diagnostic.as_ref().unwrap(), &original);
            self.pulse.selected_mode = Some(selection.action);
            self.selection = Some(selection);
        }
        let applied = self.pulse.step(p.tick, key, threat, intent);
        let context = active || diagnostic.is_some();
        let mut record = json!({"schema":1,"tick":p.tick,"seat":seat,
            "mode":self.pulse.mode,"ready":key,"attempt":self.pulse.attempt,"applied":applied,
            "original_actions":original.encode(p.owner),"actions":intent.encode(p.owner),
            "observation":context.then_some(o),"goal":m.goal,
            "capture_active":m.capture.is_some(),"recovery_active":m.recovery.is_some(),"target":m.target,
            "travel":context.then_some(m.escape_travel.as_ref().and_then(|s|s.last)),
            "diagnostic":diagnostic,"damage":state.damage_observation(seat)});
        if self.scope == Scope::Transfer {
            record["scope"] = json!("transfer");
            record["goal_since"] = json!(m.goal_since);
            record["flight_enabled"] = json!(o.local.combat.recovery.flight.flight.enabled);
            record["match_rules"] = json!(o.match_rules);
        }
        if self.pulse.mode == Mode::GuardedBrake {
            record["selection"] = json!(choosing.then_some(self.selection.as_ref()).flatten());
        }
        serde_json::to_writer(&mut self.log, &record).unwrap();
        writeln!(self.log).unwrap();
    }

    pub fn finish(mut self, state: &SurfaceSortieState) {
        if let Some(a) = &mut self.pulse.attempt
            && a.finished_tick.is_none()
        {
            a.finished_tick = Some(state.tick());
            a.reason = Some("match ended");
        }
        let mut record = json!({"schema":1,"final_tick":state.tick(),
            "seat":self.seat,"mode":self.pulse.mode,"attempt":self.pulse.attempt,
            "round":state.match_observation(),"damage":state.damage_observation(self.seat)});
        if self.pulse.mode == Mode::GuardedBrake {
            record["selection"] = json!(self.selection);
        }
        serde_json::to_writer(&mut self.log, &record).unwrap();
        writeln!(self.log).unwrap();
        self.log.flush().unwrap();
    }
}

#[cfg(test)]
#[path = "projectile_response_tests.rs"]
mod tests;
