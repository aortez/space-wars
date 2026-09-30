//! Optional compact physical trace for destination experiments. Reads the
//! already acquired observation; no queries, controls, or physics are added.
use scenario_spacewars::surface_sortie::{PilotLocation, mission::MissionObservationV1};
use serde_json::json;
use spacewars_ai::mission_pilot::{LandingHandoff, MissionTelemetry};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

pub struct BehaviorTrace {
    controls: BufWriter<File>,
    handoffs: BufWriter<File>,
    previous_handoff: [Option<LandingHandoff>; 2],
    previous_site: [Option<scenario_spacewars::surface_sortie::pilot::LandingSiteId>; 2],
}
impl BehaviorTrace {
    pub fn from_args(out: &Path) -> Option<Self> {
        match super::arg("--trace-destination-behavior", "false").as_str() {
            "true" => Some(Self {
                controls: BufWriter::new(
                    File::create(out.join("destination-behavior.jsonl")).unwrap(),
                ),
                handoffs: BufWriter::new(File::create(out.join("landing-handoffs.jsonl")).unwrap()),
                previous_handoff: [None, None],
                previous_site: [None, None],
            }),
            "false" => None,
            _ => panic!("--trace-destination-behavior must be true or false"),
        }
    }
    pub fn observe(
        &mut self,
        o: &MissionObservationV1,
        m: &MissionTelemetry,
        intent: &spacewars_ai::combat_pilot::CombatIntent,
    ) {
        let p = &o.local.combat.recovery.flight.pilot;
        let distance = m
            .target
            .and_then(|id| o.planets.iter().find(|planet| planet.index == id))
            .map(|planet| p.ship.position.distance_to(planet.motion.position));
        let visit = m
            .events
            .iter()
            .rev()
            .find(|e| e.kind == "selected" && e.planet == m.target)
            .map(|e| e.tick);
        serde_json::to_writer(
            &mut self.controls,
            &json!({
                "tick":p.tick,"seat":p.owner.index(),"policy":m.policy,
                "goal":m.goal,"target":m.target,"selected_tick":visit,
                "aboard":matches!(p.location, PilotLocation::Aboard(_)),
                "ship_available":p.ship_available,"ship_form":p.ship_form,
                "distance_to_target":distance,
                "owned_planets":o.match_context.as_ref().map(|m| m.owned_planets),
                "actions":intent.encode(p.owner),
            }),
        )
        .unwrap();
        writeln!(self.controls).unwrap();
        let handoff = m
            .destination_planning
            .as_ref()
            .and_then(|d| d.landing_handoff.as_ref());
        let site = m.capture.as_ref().and_then(|c| c.site);
        if self.previous_handoff[p.owner.index()].as_ref() != handoff
            || (handoff.is_some() && self.previous_site[p.owner.index()] != site)
        {
            serde_json::to_writer(
                &mut self.handoffs,
                &json!({
                "tick":p.tick, "seat":p.owner.index(), "handoff":handoff, "visit_tick":visit,
                    "observation":o, "mission":m,
                }),
            )
            .unwrap();
            writeln!(self.handoffs).unwrap();
            self.previous_handoff[p.owner.index()] = handoff.cloned();
            self.previous_site[p.owner.index()] = site;
        }
    }
    pub fn flush(&mut self) {
        self.controls.flush().unwrap();
        self.handoffs.flush().unwrap();
    }
}
