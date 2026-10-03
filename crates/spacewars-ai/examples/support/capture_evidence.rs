//! Optional dense evidence from observations already consumed by the mission.
use scenario_spacewars::surface_sortie::mission::MissionObservationV1;
use serde_json::json;
use spacewars_ai::{combat_pilot::CombatIntent, mission_pilot::MissionTelemetry};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
};

pub struct CaptureEvidence(BufWriter<File>);
impl CaptureEvidence {
    pub fn from_args(out: &Path) -> Option<Self> {
        crate::arg("--trace-capture-evidence", "false")
            .parse::<bool>()
            .unwrap()
            .then(|| {
                Self(BufWriter::new(
                    OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(out.join("capture-evidence.jsonl"))
                        .unwrap(),
                ))
            })
    }

    pub fn observe(
        &mut self,
        seat: usize,
        o: &MissionObservationV1,
        m: &MissionTelemetry,
        intent: CombatIntent,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        let mut pilot = json!(p);
        // Keep the query identity, but omit the bulky candidate geometry.
        pilot.as_object_mut().unwrap().remove("sites");
        let mut record = json!({"schema":1,"seat":seat,"pilot":pilot,
            "mission":{"policy":m.policy,"powered_capture":m.powered_capture,"goal":m.goal,
                "goal_since":m.goal_since,"target":m.target,"completed_sorties":m.completed_sorties},
            "capture":m.capture,"jetpack":o.local.combat.recovery.jetpack,
            "landing_objective":o.local.landing_objective,"objective_work":o.local.objective_work,
            "objective_evidence":o.local.objective_evidence,"cover":o.local.cover,
            "planets":o.planets,
            "match_context":o.match_context,"actions":intent.encode(p.owner)});
        if let Some(gate) = &m.pursuit_health
            && gate.last.is_some_and(|check| check.tick == p.tick)
        {
            record["pursuit_health"] = json!({"telemetry":gate,
                "target":o.local.combat.target,
                "last_hit_taken_tick":o.local.combat.weapons.last_hit_taken_tick});
        }
        serde_json::to_writer(&mut self.0, &record).unwrap();
        writeln!(self.0).unwrap();
    }

    pub fn finish(mut self) {
        self.0.flush().unwrap();
    }
}
