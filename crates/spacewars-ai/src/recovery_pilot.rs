//! Controlled sortie host policy with a reusable recovery task. The task only
//! restores a usable assigned ship; this policy chooses the following mission.
use crate::{
    BrainReset,
    flight_pilot::{FlightIntent, FlightTelemetry, RulePilotV2},
    recovery_task::{RecoverShipTask, RecoveryGoal, RecoveryTelemetry, TaskStatus},
};
use scenario_spacewars::{
    ShipForm,
    surface_sortie::{pilot::LandingSiteId, recovery_sensors::RecoveryTaskObservationV1},
};
use serde::Serialize;
pub const RULE_PILOT_V3_POLICY_ID: &str = "rule_pilot_v3";
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecoveryPilotTelemetry {
    pub policy: &'static str,
    pub flight: FlightTelemetry,
    pub recovery: Option<RecoveryTelemetry>,
    pub recovering: bool,
    pub completed_recoveries: u32,
    pub recovered_tick: Option<u64>,
    pub departed_tick: Option<u64>,
}
#[derive(Debug, Clone)]
pub struct RulePilotV3 {
    context: BrainReset,
    flight: RulePilotV2,
    task: Option<RecoverShipTask>,
    telemetry: RecoveryPilotTelemetry,
    seen_losses: u64,
    previous_tick: Option<u64>,
    previous_intent: FlightIntent,
}
impl RulePilotV3 {
    pub fn new(context: BrainReset) -> Self {
        let flight = RulePilotV2::new(context);
        Self {
            context,
            telemetry: RecoveryPilotTelemetry {
                policy: RULE_PILOT_V3_POLICY_ID,
                flight: flight.telemetry().clone(),
                recovery: None,
                recovering: false,
                completed_recoveries: 0,
                recovered_tick: None,
                departed_tick: None,
            },
            flight,
            task: None,
            seen_losses: 0,
            previous_tick: None,
            previous_intent: FlightIntent::default(),
        }
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::new(context);
    }
    pub fn telemetry(&self) -> &RecoveryPilotTelemetry {
        &self.telemetry
    }
    pub fn site_request(&self) -> Option<LandingSiteId> {
        self.task
            .as_ref()
            .map_or_else(|| self.flight.site_request(), |task| task.site_request())
    }
    pub fn label(&self) -> &'static str {
        if let Some(task) = &self.task {
            task.telemetry().label()
        } else if self.telemetry.departed_tick.is_some() {
            "recovered / flying again"
        } else if self.telemetry.recovered_tick.is_some() {
            "departing after recovery"
        } else {
            self.flight.label()
        }
    }
    pub fn intent(&mut self, o: &RecoveryTaskObservationV1) -> FlightIntent {
        let p = &o.flight.pilot;
        if o.version != 1
            || o.flight.version != 2
            || p.version != 1
            || p.owner != self.context.actor
        {
            return FlightIntent::default();
        }
        if self.previous_tick == Some(p.tick) {
            return self.previous_intent;
        }
        let losses = p.recovery.as_ref().map_or(0, |r| r.ships_lost);
        let replacing = self
            .task
            .as_ref()
            .is_some_and(|task| task.telemetry().goal == RecoveryGoal::Scuttle);
        if losses > self.seen_losses && !replacing
            || self.task.is_none() && (!p.ship_available || p.ship_form == ShipForm::EscapePod)
        {
            self.task = Some(RecoverShipTask::new(self.context));
            self.telemetry.recovering = true;
            self.telemetry.departed_tick = None;
        }
        self.seen_losses = losses;
        let intent = if let Some(task) = &mut self.task {
            let intent = task.step(o);
            self.telemetry.recovery = Some(task.telemetry().clone());
            if task.telemetry().status == TaskStatus::Succeeded {
                self.telemetry.completed_recoveries += 1;
                self.telemetry.recovered_tick = Some(p.tick);
                self.telemetry.recovering = false;
                self.task = None;
                // Interrupted circuits must start fresh from the recovered
                // landing. A completed sortie can resume its departure/hold.
                if self.flight.telemetry().completed_tick.is_none() {
                    self.flight.reset(self.context);
                }
            }
            intent
        } else {
            self.flight.intent(&o.flight)
        };
        self.telemetry.flight = self.flight.telemetry().clone();
        if self.telemetry.completed_recoveries > 0
            && !self.telemetry.recovering
            && p.ship.position.distance_to(p.planet.motion.position) > p.planet.radius + 55.0
        {
            self.telemetry.departed_tick.get_or_insert(p.tick);
        }
        self.previous_tick = Some(p.tick);
        self.previous_intent = intent;
        intent
    }
}
