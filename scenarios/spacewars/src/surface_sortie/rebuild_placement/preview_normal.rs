//! Default-off handoff of a selected preview's measured direction. The retained
//! point identifies the site; native placement still starts at current support.
use super::*;
use serde_json::{Value, json};

const MAX_SITE_DISTANCE: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RebuildPreviewNormal {
    seat: usize,
    captured_tick: u64,
    arrival_tick: Option<u64>,
    planet: usize,
    revision: u64,
    ships_lost: u64,
    bearing: u16,
    position: Vec2,
    pub(super) normal: Vec2,
    offset: f32,
}

impl SurfaceSortieState {
    /// Capture only a current accepted survey, validating its selected offset
    /// once against the same measured node. This does not activate the handoff.
    pub fn capture_rebuild_preview_normal(
        &mut self,
        player: usize,
        survey: &RebuildRelocationSurvey,
    ) -> bool {
        let Some((context, report)) = self.measure_rebuild_preview_normal(player, survey) else {
            return false;
        };
        self.clear_rebuild_preview_normal(player, "new_destination");
        self.rebuild_preview_events
            .push(json!({"tick":self.tick(),"seat":player,
            "kind":"captured","context":context,"validation":report,"offset_checks":1}));
        self.pilots[player].rebuild_preview_normal = Some(context);
        true
    }

    fn measure_rebuild_preview_normal(
        &self,
        player: usize,
        survey: &RebuildRelocationSurvey,
    ) -> Option<(RebuildPreviewNormal, RebuildPlacementReport)> {
        let pilot = self.pilots.get(player)?;
        let site = survey.site?;
        if survey.tick != self.tick()
            || self.world.physics.material_queries_dirty
            || pilot.rebuild_radial_placement
            || self.vehicle_available(player)
            || pilot.planet != site.planet
            || self
                .world
                .terrain
                .planets
                .get(&site.planet)?
                .field
                .revision()
                != site.revision
        {
            return None;
        }
        let attempt = survey.attempts.iter().find(|a| {
            a.placement.as_ref().is_some_and(|p| {
                p.tick == survey.tick
                    && p.planet == site.planet
                    && p.revision == Some(site.revision)
                    && p.standing.distance_to(site.position) < 0.001
                    && p.selected_offset.is_some()
                    && p.radial_up.is_none()
            })
        })?;
        let preview = attempt.placement.as_ref()?;
        let offset = preview.selected_offset?;
        let selected = preview.attempts.iter().find(|a| a.offset == offset)?;
        if selected.rejection.is_some() {
            return None;
        }
        let actor = self.spaceling_snapshot(player)?;
        let map = self.local_ground_map(player, site.planet, actor.motion.position, false)?;
        let node = map.nodes.iter().find(|n| n.id == attempt.bearing)?;
        // Both sources are native samples from the same scene. Avoid allowing
        // a near node or a direction supplied by the caller to stand in for it.
        if node.position != site.position {
            return None;
        }
        let frame = motion::SurfaceFrame::read(&self.world.physics, site.planet);
        let base = self.rebuild_ground_map(player, site.planet, actor.motion.position)?;
        let (pose, report) = self.find_rebuild_placement_offsets(
            player,
            site.planet,
            frame.position + node.position.rotate_radians(frame.angle),
            node.normal.rotate_radians(frame.angle),
            Some(&base),
            &[offset],
        );
        if pose.is_none() || report.attempts.first() != Some(selected) {
            return None;
        }
        Some((
            RebuildPreviewNormal {
                seat: player,
                captured_tick: self.tick(),
                arrival_tick: None,
                planet: site.planet,
                revision: site.revision,
                ships_lost: pilot.recovery.as_ref()?.observation().ships_lost,
                bearing: node.id,
                position: node.position,
                normal: node.normal,
                offset,
            },
            report,
        ))
    }

    /// Called when the existing task reports arrival. Independently require the
    /// same supported planet and the task's strict 0.12-unit foot-distance test.
    pub fn arrive_rebuild_preview_normal(&mut self, player: usize) -> bool {
        let Some(context) = self
            .pilots
            .get(player)
            .and_then(|p| p.rebuild_preview_normal.as_ref())
        else {
            return false;
        };
        if context.arrival_tick.is_some() || !self.preview_context_current(player, context) {
            return false;
        }
        let Some(actor) = self.spaceling_snapshot(player) else {
            return false;
        };
        if actor
            .support
            .and_then(|s| physics::planet_surface_support_index(s.collider))
            != Some(context.planet)
        {
            return false;
        }
        let frame = motion::SurfaceFrame::read(&self.world.physics, context.planet);
        let foot = (actor.motion.position - actor.up * Self::spec().half_height() - frame.position)
            .rotate_radians(-frame.angle);
        let distance = foot.distance_to(context.position);
        if !distance.is_finite() || distance >= 0.12 {
            return false;
        }
        let tick = self.tick();
        let context = self.pilots[player].rebuild_preview_normal.as_mut().unwrap();
        context.arrival_tick = Some(tick);
        self.rebuild_preview_events
            .push(json!({"tick":tick,"seat":player,
            "kind":"arrived","context":context,"foot":foot,"distance":distance}));
        true
    }

    pub fn clear_rebuild_preview_normal(&mut self, player: usize, reason: &str) {
        let Some(context) = self
            .pilots
            .get_mut(player)
            .and_then(|p| p.rebuild_preview_normal.take())
        else {
            return;
        };
        if self.rebuild_selection_uses_preview_normal(player) {
            self.cancel_rebuild_selection(player, "preview_normal_cleared");
        }
        self.rebuild_preview_events
            .push(json!({"tick":self.tick(),"seat":player,
            "kind":"cleared","reason":reason,"context":context}));
    }

    pub fn take_rebuild_preview_normal_events(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.rebuild_preview_events)
    }

    fn preview_context_current(&self, player: usize, context: &RebuildPreviewNormal) -> bool {
        let pilot = &self.pilots[player];
        context.seat == player
            && pilot.planet == context.planet
            && !pilot.rebuild_radial_placement
            && !self.vehicle_available(player)
            && self.world.planets[context.planet].owner_id == Some(pilot.owner.index())
            && self
                .world
                .terrain
                .planets
                .get(&context.planet)
                .is_some_and(|p| p.field.revision() == context.revision)
            && pilot
                .recovery
                .as_ref()
                .is_some_and(|r| r.observation().ships_lost == context.ships_lost)
    }

    pub(in crate::surface_sortie) fn refresh_rebuild_preview_normal(&mut self, player: usize) {
        if self.pilots[player]
            .rebuild_preview_normal
            .as_ref()
            .is_some_and(|p| !self.preview_context_current(player, p))
        {
            self.clear_rebuild_preview_normal(player, "context_changed");
        }
    }

    pub(super) fn active_rebuild_preview_normal(
        &self,
        player: usize,
        planet: usize,
        point: Vec2,
    ) -> Option<&RebuildPreviewNormal> {
        let context = self.pilots[player].rebuild_preview_normal.as_ref()?;
        if context.arrival_tick.is_none()
            || planet != context.planet
            || self.world.physics.material_queries_dirty
            || !self.preview_context_current(player, context)
        {
            return None;
        }
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let local = (point - frame.position).rotate_radians(-frame.angle);
        (local.distance_to(context.position) <= MAX_SITE_DISTANCE).then_some(context)
    }

    pub(in crate::surface_sortie) fn find_native_rebuild_placement(
        &self,
        player: usize,
        planet: usize,
        point: Vec2,
        contact_up: Vec2,
        map: Option<&GroundMap>,
    ) -> (Option<RebuildPose>, RebuildPlacementReport) {
        let context = self.active_rebuild_preview_normal(player, planet, point);
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let up = context.map_or(contact_up, |p| p.normal.rotate_radians(frame.angle));
        let (pose, mut report) = self.find_rebuild_placement(player, planet, point, up, map);
        report.preview_normal = context.cloned();
        (pose, report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: Duration = Duration::from_nanos(16_666_667);

    fn preparing() -> SurfaceSortieState {
        let mut state = native_forecast::tests::preparing_build(false);
        for _ in 0..2 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
        }
        state.set_rebuild_refinement(0, true);
        state
    }

    fn survey(state: &SurfaceSortieState) -> RebuildRelocationSurvey {
        let (planet, _, _) = state.rebuild_candidate(0).unwrap();
        let actor = state.spaceling_snapshot(0).unwrap();
        let frame = motion::SurfaceFrame::read(&state.world.physics, planet);
        let map = state
            .local_ground_map(0, planet, actor.motion.position, false)
            .unwrap();
        let base = state
            .rebuild_ground_map(0, planet, actor.motion.position)
            .unwrap();
        let (node, report) = map
            .nodes
            .iter()
            .find_map(|node| {
                let (pose, report) = state.find_rebuild_placement(
                    0,
                    planet,
                    frame.position + node.position.rotate_radians(frame.angle),
                    node.normal.rotate_radians(frame.angle),
                    Some(&base),
                );
                pose.map(|_| (node, report))
            })
            .unwrap();
        RebuildRelocationSurvey {
            tick: state.tick(),
            checked: 1,
            attempts: vec![RebuildRelocationAttempt {
                bearing: node.id,
                route: None,
                placement: Some(report),
            }],
            site: Some(RebuildStandingSite {
                precise: true,
                planet,
                revision: map.revision,
                position: node.position,
                walk_length: 0.0,
                flight_length: 0.0,
                jetpack_flights: 0,
                hatch_walk_length: 0.0,
            }),
            refinement: None,
            search: None,
            staging: None,
            staging_map: None,
        }
    }

    fn local_foot(state: &SurfaceSortieState) -> Vec2 {
        let actor = state.spaceling_snapshot(0).unwrap();
        let frame = motion::SurfaceFrame::read(&state.world.physics, state.pilots[0].planet);
        (actor.motion.position
            - actor.up * SurfaceSortieState::spec().half_height()
            - frame.position)
            .rotate_radians(-frame.angle)
    }

    // Lifecycle fixtures use the actual supported foot, independently of the
    // capture test's selected native map node and accepted offset.
    fn arrived() -> SurfaceSortieState {
        let mut state = preparing();
        let foot = local_foot(&state);
        let (planet, _, up) = state.rebuild_candidate(0).unwrap();
        let frame = motion::SurfaceFrame::read(&state.world.physics, planet);
        state.pilots[0].rebuild_preview_normal = Some(RebuildPreviewNormal {
            seat: 0,
            captured_tick: state.tick(),
            arrival_tick: None,
            planet,
            revision: state.world.terrain.planets[&planet].field.revision(),
            ships_lost: state.pilots[0]
                .recovery
                .as_ref()
                .unwrap()
                .observation()
                .ships_lost,
            bearing: 0,
            position: foot,
            normal: up.rotate_radians(-frame.angle),
            offset: -8.0,
        });
        assert!(state.arrive_rebuild_preview_normal(0));
        state
    }

    #[test]
    fn rebuild_preview_capture_requires_a_current_native_selected_node_and_offset() {
        let mut state = preparing();
        let selected = survey(&state);
        let before = state.world.physics.snapshot_bytes();
        let contact = state.rebuild_contact_diagnostics(0);
        let observation = state.observation(0);
        for kind in 0..7 {
            let mut invalid = selected.clone();
            match kind {
                0 => invalid.tick += 1,
                1 => invalid.site.as_mut().unwrap().revision += 1,
                2 => invalid.site.as_mut().unwrap().position.x += 0.01,
                3 => invalid.attempts[0].bearing = u16::MAX,
                4 => {
                    invalid.attempts[0]
                        .placement
                        .as_mut()
                        .unwrap()
                        .selected_offset = Some(123.0)
                }
                5 => state.world.physics.material_queries_dirty = true,
                _ => state
                    .set_rebuild_radial_placement(0, true)
                    .then_some(())
                    .unwrap(),
            }
            assert!(!state.capture_rebuild_preview_normal(0, &invalid));
            state.world.physics.material_queries_dirty = false;
            state.set_rebuild_radial_placement(0, false);
        }
        assert!(!state.capture_rebuild_preview_normal(99, &selected));
        assert!(state.capture_rebuild_preview_normal(0, &selected));
        let (_, point, _) = state.rebuild_candidate(0).unwrap();
        assert!(
            state
                .active_rebuild_preview_normal(0, selected.site.unwrap().planet, point)
                .is_none()
        );
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        assert_eq!(state.rebuild_contact_diagnostics(0), contact);
        assert_eq!(state.observation(0), observation);
        assert_eq!(state.take_rebuild_preview_normal_events().len(), 1);
        assert!(
            state.pilots[0]
                .rebuild_preview_normal
                .as_ref()
                .unwrap()
                .arrival_tick
                .is_none()
        );
    }

    #[test]
    fn rebuild_preview_arrival_is_precise_and_native_queries_keep_actual_standing() {
        let mut state = arrived();
        assert!(!state.arrive_rebuild_preview_normal(0));
        let (planet, point, up) = state.rebuild_candidate(0).unwrap();
        let frame = motion::SurfaceFrame::read(&state.world.physics, planet);
        let map = state.rebuild_ground_map(0, planet, point).unwrap();
        let (expected, original) = state.find_rebuild_placement(0, planet, point, up, Some(&map));
        let (actual, report) = state.find_native_rebuild_placement(
            0,
            planet,
            point,
            up.rotate_radians(0.7),
            Some(&map),
        );
        assert_eq!(
            actual.as_ref().map(|p| p.center),
            expected.as_ref().map(|p| p.center)
        );
        assert_eq!(report.standing, original.standing);
        assert_eq!(report.attempts, original.attempts);
        assert!(report.anchor.is_none() && report.anchor_up.is_none());
        assert!(report.preview_normal.is_some());
        assert!(
            state
                .active_rebuild_preview_normal(0, planet, point + up * 2.0)
                .is_none()
        );
        assert!(
            state
                .active_rebuild_preview_normal(0, planet + 1, point)
                .is_none()
        );
        state.world.physics.material_queries_dirty = true;
        let (pose, report) = state.find_native_rebuild_placement(0, planet, point, up, Some(&map));
        assert!(pose.is_none() && report.preview_normal.is_none());
        assert_eq!(
            report.attempts[0].rejection,
            Some(RebuildRejection::QueriesPending)
        );
        state.world.physics.material_queries_dirty = false;
        let foot = local_foot(&state);
        let context = state.pilots[0].rebuild_preview_normal.as_mut().unwrap();
        context.arrival_tick = None;
        context.position = foot + Vec2::new(0.121, 0.0);
        assert!(!state.arrive_rebuild_preview_normal(0));
        state.pilots[0]
            .rebuild_preview_normal
            .as_mut()
            .unwrap()
            .position = foot;
        assert!(state.arrive_rebuild_preview_normal(0));
        let context = state.pilots[0].rebuild_preview_normal.as_ref().unwrap();
        assert!(context.normal.distance_to(up.rotate_radians(-frame.angle)) < 0.00001);
    }

    #[test]
    fn rebuild_preview_context_is_discarded_for_revision_owner_planet_and_loss_changes() {
        for kind in 0..5 {
            let mut state = arrived();
            match kind {
                0 => {
                    state.pilots[0]
                        .rebuild_preview_normal
                        .as_mut()
                        .unwrap()
                        .revision += 1
                }
                1 => state.world.planets[0].owner_id = None,
                2 => {
                    state.pilots[0]
                        .rebuild_preview_normal
                        .as_mut()
                        .unwrap()
                        .planet += 1
                }
                3 => {
                    state.pilots[0]
                        .rebuild_preview_normal
                        .as_mut()
                        .unwrap()
                        .ships_lost += 1
                }
                _ => {
                    state.set_rebuild_radial_placement(0, true);
                }
            }
            state.refresh_rebuild_preview_normal(0);
            assert!(state.pilots[0].rebuild_preview_normal.is_none());
            assert_eq!(
                state.take_rebuild_preview_normal_events().last().unwrap()["reason"],
                "context_changed"
            );
        }
    }

    #[test]
    fn rebuild_preview_normal_survives_forecast_and_keeps_native_timer_and_diagnostics() {
        let mut state = arrived();
        state.set_rebuild_forecast_selection(0, true);
        let start = state.tick();
        let mut built = false;
        for _ in 0..900 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
            if state.vehicle_available(0) {
                built = true;
                break;
            }
        }
        assert!(built && state.tick() >= start + 478 + 40);
        let report = state.observation(0).recovery.unwrap().placement.unwrap();
        assert!(report.preview_normal.is_some() && report.anchor_up.is_some());
        let before = state.world.physics.snapshot_bytes();
        let round = state.rebuild_round_foot_diagnostics(0);
        assert_eq!(round["physics_unchanged"], true, "{round}");
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        let events = state.take_rebuild_selection_events();
        assert!(events.iter().any(|e| e["kind"] == "evaluated"
            && e["accepted"] == true
            && e["prediction"] == true
            && e["revalidation"]["launch_matches"] == true));
        assert!(
            events
                .iter()
                .filter(|e| e["kind"] == "work")
                .all(|e| e["steps"] == 4)
        );
        SurfaceSortieScenario::step(&mut state, &[], DT);
        assert!(state.pilots[0].rebuild_preview_normal.is_none());
    }

    #[test]
    fn rebuild_preview_context_change_cancels_a_pending_forecast() {
        let mut state = arrived();
        state.set_rebuild_forecast_selection(0, true);
        for _ in 0..900 {
            SurfaceSortieScenario::step(&mut state, &[], DT);
            if state.rebuild_selection_pending(0) {
                break;
            }
        }
        assert!(state.rebuild_selection_pending(0));
        state.clear_rebuild_preview_normal(0, "new_destination");
        assert!(!state.rebuild_selection_pending(0));
        assert!(!state.vehicle_available(0));
        assert!(
            state
                .take_rebuild_selection_events()
                .iter()
                .any(|e| e["kind"] == "cancelled" && e["reason"] == "preview_normal_cleared")
        );
    }
}
