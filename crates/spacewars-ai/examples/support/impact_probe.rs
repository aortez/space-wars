//! Tick-level impact evidence, with explicit optional pod-control comparisons.
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{SurfaceSortieAction, SurfaceSortieState, mission::MissionObservationV1},
};
use serde_json::json;
use spacewars_ai::{combat_pilot::CombatIntent, mission_pilot::MissionTelemetry};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};

pub struct ImpactProbe {
    file: BufWriter<File>,
    start: u64,
    end: u64,
    seat: usize,
    control: String,
    control_from: u64,
}
impl ImpactProbe {
    pub fn from_args(out: &Path, seat: usize) -> Option<Self> {
        let control = crate::arg("--impact-pod-control", "bot");
        assert!(["bot", "brake", "coast"].contains(&control.as_str()));
        if !crate::arg("--trace-impact", "false")
            .parse::<bool>()
            .unwrap()
        {
            assert_eq!(control, "bot", "pod override requires impact tracing");
            return None;
        }
        let start = crate::arg("--impact-start-tick", "0").parse().unwrap();
        let end = crate::arg("--impact-end-tick", "36001").parse().unwrap();
        let control_from = crate::arg("--impact-control-from-tick", "0")
            .parse()
            .unwrap();
        assert!(start < end);
        assert!(control == "bot" || (start..end).contains(&control_from));
        Some(Self {
            file: BufWriter::new(
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(out.join("impact.jsonl"))
                    .unwrap(),
            ),
            start,
            end,
            seat,
            control,
            control_from,
        })
    }

    pub fn apply_and_observe(
        &mut self,
        seat: usize,
        state: &SurfaceSortieState,
        o: &MissionObservationV1,
        mission: &MissionTelemetry,
        intent: &mut CombatIntent,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        let selected = intent.flight.controls;
        let overridden = seat == self.seat
            && p.tick >= self.control_from
            && p.ship_form == ShipForm::EscapePod
            && p.actor.is_none()
            && self.control != "bot";
        if overridden {
            intent.flight.controls = SurfaceSortieAction {
                brake_held: self.control == "brake",
                ..Default::default()
            };
        }
        if !(self.start..self.end).contains(&p.tick) {
            return;
        }
        let a = intent.flight.controls;
        let record = json!({"schema":1,"tick":p.tick,"seat":seat,
            "motion":state.impact_motion(seat), "ship":p.ship,
            "form":p.ship_form,"location":p.location,"controls_armed":p.controls_armed,
            "planet":{"index":p.planet.index,"motion":p.planet.motion},"gravity":p.gravity,
            "flight":o.local.combat.recovery.flight.flight,
            "damage":state.damage_observation(seat),
            "vitals":state.match_observation().map(|r|r.pilots[seat]),
            "controls":{"turn":a.horizontal,"thrust":a.primary_held,
                "brake":a.brake_held,"interact":a.interact_held},
            "bot_controls":{"turn":selected.horizontal,"thrust":selected.primary_held,
                "brake":selected.brake_held,"interact":selected.interact_held},
            "overridden":overridden,
            "actions":intent.encode(p.owner),"goal":mission.goal,"recovery":mission.recovery});
        serde_json::to_writer(&mut self.file, &record).unwrap();
        writeln!(self.file).unwrap();
    }

    pub fn finish(mut self, state: &SurfaceSortieState) {
        let record = json!({"schema":1,"final_tick":state.tick(),
            "config":{"seat":self.seat,"control":self.control,"control_from_tick":self.control_from,
                "trace_start_tick":self.start,"trace_end_tick":self.end},
            "motion":[state.impact_motion(0),state.impact_motion(1)],
            "damage":[state.damage_observation(0),state.damage_observation(1)],
            "round":state.match_observation()});
        serde_json::to_writer(&mut self.file, &record).unwrap();
        writeln!(self.file).unwrap();
        self.file.flush().unwrap();
    }
}
