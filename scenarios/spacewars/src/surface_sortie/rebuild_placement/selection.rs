//! Default-off native placement experiment. Forecast work has a fixed quota;
//! a result is used only on its scheduled construction tick after fresh queries.
use super::*;
use serde_json::{Value, json};

const MAX_ANCHOR_DISTANCE: f32 = 1.0;

#[derive(Clone, Default)]
pub(in crate::surface_sortie) struct SelectionState {
    enabled: [bool; 2],
    searches: [Option<Search>; 2],
    events: Vec<Value>,
}

#[derive(Clone)]
struct Search {
    tick: u64,
    planet: usize,
    revision: Option<u64>,
    anchor: Vec2,
    up: Vec2,
    offsets: Vec<f32>,
    next: usize,
    last_report: RebuildPlacementReport,
    job: Option<RebuildLocalForecast>,
}

impl SurfaceSortieState {
    pub fn set_rebuild_forecast_selection(&mut self, player: usize, enabled: bool) -> bool {
        if player >= self.player_count() {
            return false;
        }
        self.rebuild_selection
            .get_or_insert_with(Default::default)
            .enabled[player] = enabled;
        if !enabled {
            self.cancel_rebuild_selection(player, "disabled");
        }
        true
    }

    pub fn take_rebuild_selection_events(&mut self) -> Vec<Value> {
        self.rebuild_selection
            .as_mut()
            .map(|s| std::mem::take(&mut s.events))
            .unwrap_or_default()
    }

    pub(in crate::surface_sortie) fn rebuild_selection_enabled(&self, player: usize) -> bool {
        self.rebuild_selection
            .as_ref()
            .is_some_and(|s| s.enabled[player])
    }

    pub(in crate::surface_sortie) fn rebuild_selection_pending(&self, player: usize) -> bool {
        self.rebuild_selection
            .as_ref()
            .is_some_and(|s| s.searches[player].is_some())
    }

    pub(in crate::surface_sortie) fn cancel_rebuild_selection(
        &mut self,
        player: usize,
        reason: &str,
    ) {
        let tick = self.tick();
        if let Some(probe) = &mut self.rebuild_selection
            && let Some(search) = probe.searches[player].take()
        {
            probe.events.push(json!({"tick":tick,"seat":player,"kind":"cancelled","reason":reason,"search_tick":search.tick,
                "forecast":search.job.as_ref().map(RebuildLocalForecast::diagnostics)}));
        }
    }

    fn selection_event(&mut self, player: usize, mut event: Value) {
        event["tick"] = json!(self.tick());
        event["seat"] = json!(player);
        self.rebuild_selection.as_mut().unwrap().events.push(event);
    }

    pub(in crate::surface_sortie) fn select_forecast_rebuild(
        &mut self,
        player: usize,
        planet: usize,
        point: Vec2,
        up: Vec2,
    ) -> Option<(Option<RebuildPose>, RebuildPlacementReport)> {
        let start = std::time::Instant::now();
        let result = self.select_forecast_rebuild_inner(player, planet, point, up);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        self.selection_event(player,json!({"kind":"update","elapsed_ms":elapsed,"pending":self.rebuild_selection_pending(player)}));
        result
    }

    fn select_forecast_rebuild_inner(
        &mut self,
        player: usize,
        planet: usize,
        point: Vec2,
        up: Vec2,
    ) -> Option<(Option<RebuildPose>, RebuildPlacementReport)> {
        let old = self.rebuild_selection.as_mut().unwrap().searches[player].take();
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let local = |p: Vec2| (p - frame.position).rotate_radians(-frame.angle);
        let revision = self
            .world
            .terrain
            .planets
            .get(&planet)
            .map(|t| t.field.revision());
        let Some(mut search) = old else {
            let map = self.rebuild_ground_map(player, planet, point);
            let (pose, report) =
                self.find_rebuild_placement(player, planet, point, up, map.as_ref());
            let Some(pose) = pose else {
                return Some((None, report));
            };
            let preferred = report.selected_offset.unwrap();
            let mut offsets = vec![preferred];
            for &offset in REBUILD_OFFSETS
                .iter()
                .chain(if self.pilots[player].rebuild_refinement {
                    refinement::EXTRA_OFFSETS.as_slice()
                } else {
                    &[]
                })
            {
                if !offsets.contains(&offset) {
                    offsets.push(offset);
                }
            }
            let job = RebuildLocalForecast::candidate(self, player, &pose, report.clone());
            self.selection_event(player,json!({"kind":"started","anchor":local(point),"revision":revision,
                "offsets":offsets,"preferred":preferred,"target_tick":self.tick()+local_forecast::START_DELAY as u64,"report":report}));
            self.rebuild_selection.as_mut().unwrap().searches[player] = Some(Search {
                tick: self.tick(),
                planet,
                revision,
                anchor: local(point),
                up: up.rotate_radians(-frame.angle),
                offsets,
                next: 1,
                last_report: report,
                job: Some(job),
            });
            return None;
        };
        if planet != search.planet
            || revision != search.revision
            || self.world.physics.material_queries_dirty
            || local(point).distance_to(search.anchor) > MAX_ANCHOR_DISTANCE
        {
            self.selection_event(
                player,
                json!({"kind":"invalidated","search_tick":search.tick,"reason":"site_changed",
                "anchor_distance":local(point).distance_to(search.anchor),"revision":revision,
                "forecast":search.job.as_ref().map(RebuildLocalForecast::diagnostics)}),
            );
            reject(&mut search.last_report, RebuildRejection::ForecastStale);
            return Some((None, search.last_report));
        }
        if let Some(mut job) = search.job.take() {
            let work = job.advance(4);
            self.selection_event(player,json!({"kind":"work","search_tick":search.tick,"forecast_tick":job.tick(),"steps":work}));
            if !job.is_complete() {
                search.job = Some(job);
                self.rebuild_selection.as_mut().unwrap().searches[player] = Some(search);
                return None;
            }
            let prediction = job.predicts_settling();
            let offset = search.offsets[search.next - 1];
            let mut chosen = None;
            let mut revalidation = Value::Null;
            if prediction == Some(true) {
                let (pose, report) = self.check_anchored_rebuild(player, &search, point, offset);
                let matches = pose
                    .as_ref()
                    .is_some_and(|pose| job.launch_matches(self, pose));
                revalidation = json!({"tick":self.tick(),"report":report,"launch_matches":matches});
                if matches {
                    chosen = pose;
                    search.last_report = report;
                } else {
                    search.last_report = report;
                    reject(&mut search.last_report, RebuildRejection::ForecastStale);
                }
            } else {
                reject(
                    &mut search.last_report,
                    if prediction.is_some() {
                        RebuildRejection::ForecastRejected
                    } else {
                        RebuildRejection::ForecastUnavailable
                    },
                );
            }
            self.selection_event(player,json!({"kind":"evaluated","search_tick":search.tick,"offset":offset,
                "accepted":chosen.is_some(),"prediction":prediction,"revalidation":revalidation,"forecast":job.diagnostics()}));
            if let Some(pose) = chosen {
                return Some((Some(pose), search.last_report));
            }
        } else if search.next < search.offsets.len() {
            // At most one new offset is queried per frame; the ground map and
            // obstruction queries are current, even though the anchor is fixed.
            let offset = search.offsets[search.next];
            search.next += 1;
            let (pose, report) = self.check_anchored_rebuild(player, &search, point, offset);
            self.selection_event(player,json!({"kind":"candidate","search_tick":search.tick,"offset":offset,"report":report}));
            if let Some(pose) = pose {
                search.job = Some(RebuildLocalForecast::candidate(
                    self,
                    player,
                    &pose,
                    report.clone(),
                ));
            }
            search.last_report = report;
        }
        if search.job.is_none() && search.next == search.offsets.len() {
            self.selection_event(
                player,
                json!({"kind":"exhausted","search_tick":search.tick,"report":search.last_report}),
            );
            return Some((None, search.last_report));
        }
        self.rebuild_selection.as_mut().unwrap().searches[player] = Some(search);
        None
    }

    fn check_anchored_rebuild(
        &self,
        player: usize,
        search: &Search,
        standing: Vec2,
        offset: f32,
    ) -> (Option<RebuildPose>, RebuildPlacementReport) {
        let frame = motion::SurfaceFrame::read(&self.world.physics, search.planet);
        let map = self.rebuild_ground_map(player, search.planet, standing);
        self.find_rebuild_placement_from(
            player,
            search.planet,
            PlacementOrigin {
                point: frame.position + search.anchor.rotate_radians(frame.angle),
                up: search.up.rotate_radians(frame.angle),
                standing,
                anchored: true,
            },
            map.as_ref(),
            &[offset],
        )
    }
}

fn reject(report: &mut RebuildPlacementReport, reason: RebuildRejection) {
    if let Some(offset) = report.selected_offset.take()
        && let Some(attempt) = report.attempts.iter_mut().find(|a| a.offset == offset)
    {
        attempt.rejection = Some(reason);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: Duration = Duration::from_nanos(16_666_667);

    fn pending() -> SurfaceSortieState {
        let mut state = native_forecast::tests::preparing_build(false);
        assert!(state.set_rebuild_forecast_selection(0, true));
        assert!(!state.set_rebuild_forecast_selection(9, true));
        for _ in 0..900 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
            if state.rebuild_selection_pending(0) {
                return state;
            }
        }
        panic!("no candidate");
    }

    #[test]
    fn rebuild_selection_waits_for_future_launch_and_matches_native_settling() {
        let mut state = pending();
        let mut projected = state.rebuild_selection.as_ref().unwrap().searches[0]
            .as_ref()
            .unwrap()
            .job
            .clone()
            .unwrap();
        while !projected.is_complete() {
            projected.advance(4);
        }
        let start = state.tick();
        for _ in 1..40 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
            assert!(!state.vehicle_available(0));
            assert_eq!(
                state.pilots[0]
                    .recovery
                    .as_ref()
                    .unwrap()
                    .observation()
                    .status,
                SurfaceRecoveryStatus::Rebuilding
            );
        }
        SurfaceSortieScenario::step(&mut state, &[], DT);
        assert!(
            state.vehicle_available(0),
            "{:?}",
            state.take_rebuild_selection_events()
        );
        assert_eq!(state.tick(), start + 40);
        let motion = state
            .world
            .physics
            .world
            .motion(state.world.physics.ship_body(0))
            .unwrap();
        let pose = RebuildPose {
            center: motion.position,
            normal: Vec2::Y.rotate_radians(motion.angle),
        };
        assert!(projected.launch_matches(&state, &pose));
        let saved = state.world.planets[0].mass;
        state.world.planets[0].mass *= 1.01;
        assert!(!projected.launch_matches(&state, &pose));
        state.world.planets[0].mass = saved;
        let events = state.take_rebuild_selection_events();
        let result = events.iter().find(|e| e["kind"] == "evaluated").unwrap();
        assert_eq!(result["accepted"], true);
        assert_eq!(result["revalidation"]["launch_matches"], true);
        assert_eq!(events.iter().filter(|e| e["kind"] == "work").count(), 40);
        assert!(
            events
                .iter()
                .filter(|e| e["kind"] == "work")
                .all(|e| e["steps"] == 4)
        );
        let forecast = &result["forecast"];
        assert_eq!(forecast["warmup_steps"], 40);
        assert_eq!(forecast["steps"], 120);
        assert_eq!(forecast["launch_tick"], state.tick());
        assert_eq!(
            forecast["samples"][0]["planet"],
            json!(state.pilot_observation(0, None).planet.motion)
        );
        let initial = state.pilot_observation(0, None).ship;
        let expected = &forecast["samples"][0]["ship"]["position"];
        assert!(
            initial.position.distance_to(Vec2::new(
                expected["x"].as_f64().unwrap() as f32,
                expected["y"].as_f64().unwrap() as f32
            )) < 0.002
        );
        let released = [
            SurfaceSortieAction::default().encode(PlayerId::PLAYER_1),
            SurfaceWingAction::default().encode(PlayerId::PLAYER_1),
            combat::SurfaceWeaponAction::default().encode(PlayerId::PLAYER_1),
            SurfaceMiningAction::default().encode(PlayerId::PLAYER_1),
            impact::SurfaceImpactAction::default().encode(PlayerId::PLAYER_1),
        ];
        let mut settled = None;
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &released, DT);
            if state.vehicle_settled(0) {
                settled.get_or_insert(state.tick());
            }
        }
        assert!(settled.is_some());
        assert_eq!(forecast["first_settled_tick"], json!(settled));
    }

    #[test]
    fn rebuild_selection_rejects_dirty_queries_and_moved_footing_without_building() {
        let original = pending();
        for moved in [false, true] {
            let mut state = original.clone();
            let (planet, mut point, up) = state.rebuild_candidate(0).unwrap();
            if moved {
                point += Vec2::new(2.0, 0.0);
            } else {
                state.world.physics.material_queries_dirty = true;
            }
            let before = state.world.physics.snapshot_bytes();
            let (pose, report) = state.select_forecast_rebuild(0, planet, point, up).unwrap();
            assert!(pose.is_none() && report.selected_offset.is_none());
            assert!(!state.rebuild_selection_pending(0));
            assert_eq!(state.world.physics.snapshot_bytes(), before);
            assert!(!state.vehicle_available(0));
        }
    }

    #[test]
    fn rebuild_selection_cancels_on_lost_ownership_and_rejects_early_results() {
        let mut state = pending();
        let mut job = state.rebuild_selection.as_ref().unwrap().searches[0]
            .as_ref()
            .unwrap()
            .job
            .clone()
            .unwrap();
        while !job.is_complete() {
            job.advance(4);
        }
        assert_eq!(job.predicts_settling(), Some(true));
        let (planet, point, up) = state.rebuild_candidate(0).unwrap();
        let map = state.rebuild_ground_map(0, planet, point);
        let pose = state
            .find_rebuild_placement(0, planet, point, up, map.as_ref())
            .0
            .unwrap();
        assert!(!job.launch_matches(&state, &pose));
        state.world.planets[planet].owner_id = None;
        SurfaceSortieScenario::step(&mut state, &[], DT);
        assert!(!state.rebuild_selection_pending(0));
        assert!(!state.vehicle_available(0));
        assert_eq!(
            state.pilots[0]
                .recovery
                .as_ref()
                .unwrap()
                .observation()
                .status,
            SurfaceRecoveryStatus::NeedOwnedPlanet
        );
        assert!(
            state
                .take_rebuild_selection_events()
                .iter()
                .any(|e| e["kind"] == "cancelled")
        );
    }
}
