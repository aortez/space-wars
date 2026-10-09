//! Diagnostic full-world rollout of a fresh replacement. This is deliberately
//! outside placement policy and bot sensors: it measures the value and cost of
//! native settling before choosing a production prediction model.
use super::*;
use pilot::LandingSiteQuery;
use serde_json::{Value, json};
use std::time::Instant;

const FORECAST_TICKS: usize = 120;
const DT: Duration = Duration::from_nanos(16_666_667);

fn neutral_actions(state: &SurfaceSortieState) -> Vec<Action> {
    state
        .pilots
        .iter()
        .flat_map(|pilot| {
            let owner = pilot.owner;
            [
                SurfaceSortieAction::default().encode(owner),
                SurfaceWingAction::default().encode(owner),
                combat::SurfaceWeaponAction::default().encode(owner),
                SurfaceMiningAction::default().encode(owner),
                impact::SurfaceImpactAction::default().encode(owner),
            ]
        })
        .collect()
}

fn sample(state: &SurfaceSortieState, player: usize) -> Value {
    let pilot = state.pilot_observation_with_query(player, LandingSiteQuery::NotRequested);
    json!({"tick":state.tick(),"pilot":pilot,
        "landing_diagnostics":state.landing_diagnostics(player,None),
        "native_settled":state.vehicle_available(player) && state.vehicle_settled(player)})
}

impl SurfaceSortieState {
    /// No future actions are accepted by this interface. All seats release all
    /// controls on the clone; existing hazards and native world updates remain.
    /// A negative result means only "not settled within two seconds".
    pub fn rebuild_native_forecast(&self, player: usize) -> Value {
        let total_start = Instant::now();
        let Some(pilot) = self.pilots.get(player) else {
            return json!({"unavailable":"invalid seat"});
        };
        let Some(report) = pilot
            .recovery
            .as_ref()
            .and_then(|r| r.observation().placement)
            .filter(|p| p.tick == self.tick() && p.selected_offset.is_some())
        else {
            return json!({"unavailable":"not a fresh accepted placement"});
        };
        if self.world.physics.material_queries_dirty
            || !self.vehicle_available(player)
            || self.match_outcome().is_some()
        {
            return json!({"unavailable":"queries pending, replacement unavailable or round finished"});
        }
        let verification_start = Instant::now();
        let before = self.world.physics.world.snapshot_bytes().unwrap();
        let observations = || {
            (0..self.player_count())
                .map(|p| self.pilot_observation_with_query(p, LandingSiteQuery::NotRequested))
                .collect::<Vec<_>>()
        };
        let pilots_before = observations();
        let contact_before = self.rebuild_contact_diagnostics(player);
        let mut verification_time = verification_start.elapsed();
        let clone_start = Instant::now();
        let mut forecast = self.clone();
        let clone_time = clone_start.elapsed();
        let actions = neutral_actions(&forecast);
        let mut samples = Vec::with_capacity(FORECAST_TICKS + 1);
        let diagnostic_start = Instant::now();
        samples.push(sample(&forecast, player));
        let mut diagnostic_time = diagnostic_start.elapsed();
        let mut step_time = Duration::ZERO;
        let mut first_settled_tick = None;
        let mut stop = "horizon";
        for _ in 0..FORECAST_TICKS {
            let start = Instant::now();
            SurfaceSortieScenario::step(&mut forecast, &actions, DT);
            step_time += start.elapsed();
            let start = Instant::now();
            let row = sample(&forecast, player);
            if forecast.vehicle_available(player) && forecast.vehicle_settled(player) {
                first_settled_tick.get_or_insert(forecast.tick());
            }
            samples.push(row);
            diagnostic_time += start.elapsed();
            if !forecast.vehicle_available(player) {
                stop = "vehicle_unavailable";
                break;
            }
            if forecast.match_outcome().is_some() {
                stop = "round_finished";
                break;
            }
        }
        let steps = samples.len() - 1;
        let prediction = if first_settled_tick.is_some() {
            Some(true)
        } else if steps == FORECAST_TICKS && stop == "horizon" {
            Some(false)
        } else {
            None
        };
        let verification_start = Instant::now();
        assert_eq!(self.world.physics.world.snapshot_bytes().unwrap(), before);
        assert_eq!(observations(), pilots_before);
        assert_eq!(self.rebuild_contact_diagnostics(player), contact_before);
        verification_time += verification_start.elapsed();
        let milliseconds = |d: Duration| d.as_secs_f64() * 1000.0;
        json!({"tick":self.tick(),"seat":player,"report":report,"horizon_ticks":FORECAST_TICKS,
            "step_nanoseconds":DT.as_nanos(),"steps":steps,"stop":stop,
            "action_policy":"all_seats_neutral","privileged_full_world":true,"future_actions_read":false,
            "settles_within_horizon":prediction,"first_settled_tick":first_settled_tick,"samples":samples,
            "read_only":true,"physics_unchanged":true,"pilots_unchanged":true,
            "physics_snapshot_bytes":before.len(),"bodies":self.world.physics.world.body_count(),
            "colliders":self.world.physics.world.collider_count(),
            "timing":{"clone_ms":milliseconds(clone_time),"native_steps_ms":milliseconds(step_time),
                "diagnostics_ms":milliseconds(diagnostic_time),"verification_ms":milliseconds(verification_time),
                "total_ms":milliseconds(total_start.elapsed())}})
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::surface_sortie::rebuild_placement) fn fresh_build(
        local_forecast: bool,
    ) -> SurfaceSortieState {
        let mut state = SurfaceSortieScenario::init_material(42, 1);
        assert!(state.rebuild_native_forecast(99)["unavailable"].is_string());
        assert!(state.rebuild_native_forecast(0)["unavailable"].is_string());
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            DT,
        );
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        state.enable_recovery();
        if local_forecast {
            state.enable_rebuild_local_forecasts();
        }
        state.world.planets[0].owner_id = Some(0);
        state.set_rebuild_contact_frame(0, true);
        state.world.ships[0].translate_life(-state.world.ships[0].life_max);
        for _ in 0..900 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
            if state.pilots[0]
                .recovery
                .as_ref()
                .unwrap()
                .observation()
                .rebuilds
                > 0
            {
                return state;
            }
        }
        panic!("native replacement did not build");
    }

    #[test]
    fn rebuild_native_forecast_is_bounded_repeatable_and_retains_live_physics() {
        let mut state = fresh_build(false);
        let before = state.world.physics.snapshot_bytes();
        let observation = SurfaceSortieScenario::observe(&state);
        let mut result = state.rebuild_native_forecast(0);
        let mut repeated = state.rebuild_native_forecast(0);
        for value in [&mut result, &mut repeated] {
            value.as_object_mut().unwrap().remove("timing");
        }
        assert_eq!(result, repeated);
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        assert_eq!(
            SurfaceSortieScenario::observe(&state).payload,
            observation.payload
        );
        assert_eq!(result["steps"], FORECAST_TICKS);
        assert_eq!(
            result["samples"].as_array().unwrap().len(),
            FORECAST_TICKS + 1
        );
        assert_eq!(result["settles_within_horizon"], true);
        assert!(result["first_settled_tick"].as_u64().unwrap() > state.tick());

        // Advance the live fixture with explicit released controls, independent
        // of the forecast's helper. Every projected ship pose/contact must match.
        let released = [
            SurfaceSortieAction::default().encode(PlayerId::PLAYER_1),
            SurfaceWingAction::default().encode(PlayerId::PLAYER_1),
            combat::SurfaceWeaponAction::default().encode(PlayerId::PLAYER_1),
            SurfaceMiningAction::default().encode(PlayerId::PLAYER_1),
            impact::SurfaceImpactAction::default().encode(PlayerId::PLAYER_1),
        ];
        state.world.physics.material_queries_dirty = true;
        assert!(state.rebuild_native_forecast(0)["unavailable"].is_string());
        state.world.physics.material_queries_dirty = false;
        for row in result["samples"].as_array().unwrap().iter().skip(1) {
            SurfaceSortieScenario::step(&mut state, &released, DT);
            assert_eq!(row["tick"], state.tick());
            assert_eq!(row["pilot"]["landing"], json!(state.pilots[0].landing));
            assert_eq!(
                row["landing_diagnostics"],
                state.landing_diagnostics(0, None)
            );
            assert_eq!(
                row["pilot"]["ship"],
                json!(state.pilot_observation(0, None).ship)
            );
        }
        assert!(state.rebuild_native_forecast(0)["unavailable"].is_string());
    }
}
