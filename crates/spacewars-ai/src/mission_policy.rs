//! Material mission identities shared by interactive and headless hosts.
use crate::{BrainReset, mission_pilot::MaterialMissionPilot};
use engine_common::CombatBreakSettings;
use scenario_spacewars::surface_sortie::landing_objective::ObjectivePlanning;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum MissionPolicy {
    #[default]
    #[serde(rename = "material_mission_v9")]
    Legacy,
    #[serde(rename = "material_mission_v10")]
    Planner,
    #[serde(rename = "material_mission_v11")]
    JetpackPlanner,
}
impl MissionPolicy {
    pub const ALL: [Self; 3] = [Self::Legacy, Self::Planner, Self::JetpackPlanner];
    pub fn id(self) -> &'static str {
        match self {
            Self::Legacy => "material_mission_v9",
            Self::Planner => "material_mission_v10",
            Self::JetpackPlanner => "material_mission_v11",
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Legacy => "Legacy bot v9",
            Self::Planner => "Planner bot v10",
            Self::JetpackPlanner => "Jetpack bot v11",
        }
    }
    pub fn objective_planning(self) -> ObjectivePlanning {
        match self {
            Self::Legacy => ObjectivePlanning::Legacy,
            Self::Planner => ObjectivePlanning::JointRoundTrip,
            Self::JetpackPlanner => ObjectivePlanning::JetpackRoundTrip,
        }
    }
    pub fn descriptor(self) -> PolicyDescriptor {
        PolicyDescriptor {
            policy: self,
            sensor_profile: match self {
                Self::Legacy => "mission_cadenced_sequential_routes_v1",
                Self::Planner => "mission_cadenced_joint_routes_v1",
                Self::JetpackPlanner => "mission_cadenced_jetpack_routes_v1",
            },
            // Cadence bounds frequency, not work. No equal-budget claim yet.
            planning_work_quota: None,
        }
    }
}
impl std::str::FromStr for MissionPolicy {
    type Err = String;
    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|p| p.id() == id)
            .ok_or_else(|| format!("unknown mission policy {id:?}; expected material_mission_v9, material_mission_v10 or material_mission_v11"))
    }
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PolicyDescriptor {
    pub policy: MissionPolicy,
    pub sensor_profile: &'static str,
    pub planning_work_quota: Option<u32>,
}

/// Selection owns its sensor semantics as well as its controller. The legacy
/// constructor remains pinned; shared flight/combat/recovery tasks are reused.
#[derive(Debug, Clone)]
pub struct MissionBot(MaterialMissionPilot);
impl MissionBot {
    pub fn new(policy: MissionPolicy, context: BrainReset, breaks: CombatBreakSettings) -> Self {
        Self(MaterialMissionPilot::with_policy(context, breaks, policy))
    }
    /// Opt-in first-site deadline and local waiting guidance for comparison.
    pub fn with_bounded_acquisition(mut self, enabled: bool) -> Self {
        self.0.bounded_acquisition = enabled;
        self
    }
    /// Opt-in headless experiment; standard policy selection leaves it disabled.
    pub fn with_pursuit_disengagement(mut self, enabled: bool) -> Self {
        self.0.enable_pursuit_disengagement(enabled);
        self
    }
    /// Records successor forecasts without changing the mission's decisions.
    /// Requires an enabled pursuit-disengagement experiment.
    pub fn with_disengagement_handoff_probe(mut self, enabled: bool) -> Self {
        self.0.configure_handoff_probe(enabled);
        self
    }
    /// Request diagnostic destination material/cover evidence during escape.
    /// Requires a host using the shared live-planning adapter.
    pub fn with_destination_cover_probe(mut self, enabled: bool) -> Self {
        self.0.configure_destination_cover_probe(enabled);
        self
    }
    /// Experimental wall-aware escape and successor-transfer guidance. Requires
    /// pursuit disengagement; default policies retain their previous controls.
    pub fn with_disengagement_boundary_guidance(mut self, enabled: bool) -> Self {
        self.0.configure_disengagement_boundary(enabled);
        self
    }
}
impl std::ops::Deref for MissionBot {
    type Target = MaterialMissionPilot;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for MissionBot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use scenario_spacewars::{PlayerId, surface_sortie::SurfaceSortieScenario};
    use std::time::Duration;

    #[test]
    fn identity_sensor_profile_clone_and_reset_remain_bound_to_selection() {
        let mut state = SurfaceSortieScenario::init_material_travel(42, false);
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let context = BrainReset {
            actor: PlayerId::PLAYER_1,
            episode_seed: 42,
        };
        for policy in MissionPolicy::ALL {
            assert_eq!(policy.id().parse::<MissionPolicy>().unwrap(), policy);
            let mut bot = MissionBot::new(policy, context, CombatBreakSettings::default());
            assert_eq!(
                bot.sensor_request().objective_planning,
                policy.objective_planning()
            );
            let o =
                state.mission_observation_with_cadence(0, bot.sensor_request(), Default::default());
            let mut copy = bot.clone();
            let action = bot.intent(&o);
            assert_eq!(copy.intent(&o), action);
            assert_eq!(bot.intent(&o), action);
            assert_eq!(bot.telemetry(), copy.telemetry());
            bot.reset(context);
            assert_eq!(bot.telemetry().policy, policy.id());
            assert_eq!(
                bot.sensor_request().objective_planning,
                policy.objective_planning()
            );
            assert_eq!(
                bot.telemetry(),
                MissionBot::new(policy, context, CombatBreakSettings::default()).telemetry()
            );
        }
        assert!("material_mission_latest".parse::<MissionPolicy>().is_err());
        assert_eq!(
            MaterialMissionPilot::new(context, CombatBreakSettings::default())
                .telemetry()
                .policy,
            MissionPolicy::Legacy.id()
        );
    }
}
