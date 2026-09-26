//! Offline, pre-intent sampling of first eligible alternative transfers. The
//! criteria and window are fixed before any nominated continuation is run.
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{PilotLocation, mission::MissionObservationV1},
};
use serde_json::{Value, json};
use spacewars_ai::{
    mission_evaluation::TransferSource, mission_pilot::MissionGoal, mission_policy::MissionBot,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

pub struct TransferSources {
    found: [bool; 2],
    file: BufWriter<File>,
}

impl TransferSources {
    pub fn from_args(out: &Path) -> Option<Self> {
        let enabled = match super::arg("--sample-transfer-sources", "false").as_str() {
            "true" => true,
            "false" => false,
            _ => panic!("--sample-transfer-sources must be true or false"),
        };
        enabled.then(|| {
            assert_eq!(super::arg("--mode", "quiet"), "duel");
            assert_eq!(super::arg("--match", "false"), "true");
            assert_eq!(super::arg("--probe-transfer-destination", "none"), "none");
            Self {
                found: [false; 2],
                file: BufWriter::new(File::create(out.join("transfer-sources.jsonl")).unwrap()),
            }
        })
    }

    pub fn observe(&mut self, seat: usize, bot: &MissionBot, o: &MissionObservationV1) {
        let p = &o.local.combat.recovery.flight.pilot;
        if self.found[seat]
            || !(60..10800).contains(&p.tick)
            || !p.controls_armed
            || !p.queries_ready
            || !p.ship_available
            || p.ship_health <= 0.0
            || p.ship_form != ShipForm::Ship
            || p.location == PilotLocation::OnFoot
            || !matches!(
                bot.telemetry().goal,
                MissionGoal::Launch | MissionGoal::Transfer | MissionGoal::Capture
            )
        {
            return;
        }
        let mut alternatives: Vec<_> = o
            .planets
            .iter()
            .filter(|planet| {
                planet.index != p.planet.index && bot.transfer_probe_gate(o, planet.index).is_ok()
            })
            .map(|planet| planet.index)
            .collect();
        alternatives.sort_unstable();
        if alternatives.is_empty() {
            return;
        }
        let truncated = alternatives.len() > 2;
        alternatives.truncate(2);
        let source = TransferSource::from_observation(o);
        serde_json::to_writer(&mut self.file, &json!({
            "seat":seat,"source_tick":p.tick,"current_target":bot.telemetry().target,
            "transfer_source":source,"mission_before":bot.telemetry(),"alternatives_truncated":truncated,
            "alternatives":alternatives.iter().map(|&destination| source.diagnose(destination)).collect::<Vec<_>>(),
        })).unwrap();
        writeln!(self.file).unwrap();
        self.found[seat] = true;
    }

    pub fn report(&mut self) -> Value {
        self.file.flush().unwrap();
        json!({"found":self.found,"window":[60,10800],"maximum_alternatives":2,
               "scope":"First pre-intent interplanetary alternatives per seat satisfying the nomination gates; higher-priority controls may still refuse. Fixed index order, no retries. Discovery changes no bot or world state; diagnostics and IO are outside live planner fuel."})
    }
}
