//! Opt-in completed-frame placement experiment and read-only contact evidence.
use super::*;
use serde_json::{Value, json};

impl SurfaceSortieState {
    /// Change only the native placement anchor, after the existing eligibility
    /// checks. Disabled by default pending paired recovery qualification.
    pub fn set_rebuild_contact_frame(&mut self, player: usize, enabled: bool) -> bool {
        let Some(pilot) = self.pilots.get_mut(player) else {
            return false;
        };
        pilot.rebuild_contact_frame = enabled;
        true
    }

    /// Selected support in both solver and completed frames. Kept outside bot
    /// observations; reads neither step physics nor refresh terrain queries.
    pub fn rebuild_contact_diagnostics(&self, player: usize) -> Value {
        let Some(pilot) = self.pilots.get(player) else {
            return Value::Null;
        };
        let support = self.spaceling_snapshot(player).and_then(|s| s.support);
        let contact = support.and_then(|s| {
            let planet = physics::planet_surface_support_index(s.collider)
                .filter(|&p| p < self.world.planets.len())?;
            let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
            let current = frame.position + s.local_surface.position.rotate_radians(frame.angle);
            Some(json!({
                "planet": planet, "frame_position": frame.position, "frame_angle": frame.angle,
                "solver_position": s.position, "solver_normal": s.normal,
                "local_position": s.local_surface.position, "local_normal": s.local_surface.normal,
                "current_position": current,
                "current_normal": s.local_surface.normal.rotate_radians(frame.angle),
                "offset": current.distance_to(s.position),
            }))
        });
        json!({"tick": self.world.tick, "seat": player, "enabled": pilot.rebuild_contact_frame,
            "contact": contact, "candidate": self.rebuild_candidate(player)})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: Duration = Duration::from_nanos(16_666_667);

    fn idle(state: &mut SurfaceSortieState, ticks: usize) {
        for _ in 0..ticks {
            SurfaceSortieScenario::step(state, &[], DT);
        }
    }

    fn stranded(preset: SurfaceMotionPreset, bearing: f32) -> SurfaceSortieState {
        let world = SurfaceSortieScenario::init(preset, 7).world;
        let mut state = SurfaceSortieScenario::on_surface(
            world,
            preset,
            0,
            Vec2::Y.rotate_radians(bearing),
            None,
        );
        state.enable_recovery();
        state.world.planets[0].owner_id = Some(0);
        idle(&mut state, 120);
        assert!(state.vehicle_settled(0));
        SurfaceSortieScenario::step(
            &mut state,
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            DT,
        );
        idle(&mut state, 120);
        state.world.ships[0].translate_life(-state.world.ships[0].life_max);
        idle(&mut state, 2);
        assert!(state.rebuild_candidate(0).is_ok());
        state
    }

    #[test]
    fn rebuild_contact_frame_follows_translating_and_rotating_support_without_mutation() {
        for preset in [
            SurfaceMotionPreset::Stationary,
            SurfaceMotionPreset::Translating,
            SurfaceMotionPreset::Orbit,
        ] {
            for bearing in 0..4 {
                let mut state = stranded(preset, bearing as f32 * std::f32::consts::FRAC_PI_2);
                let snapshot = state.spaceling_snapshot(0).unwrap();
                let contact = snapshot.support.unwrap();
                let original = state.rebuild_candidate(0).unwrap();
                assert_eq!(original, (0, contact.position, contact.normal));
                let before = state.world.physics.snapshot_bytes();
                let recovery = state.observation(0).recovery;
                let tick = state.tick();
                assert!(!state.set_rebuild_contact_frame(99, true));
                assert!(state.set_rebuild_contact_frame(0, true));
                let corrected = state.rebuild_candidate(0).unwrap();
                let frame = state.planet_motion(0);
                let local = (corrected.1 - frame.position).rotate_radians(-frame.angle);
                assert!(local.distance_to(contact.local_surface.position) < 0.0001);
                assert!(
                    corrected
                        .2
                        .rotate_radians(-frame.angle)
                        .distance_to(contact.local_surface.normal)
                        < 0.0001
                );
                if preset == SurfaceMotionPreset::Orbit {
                    assert!(original.1.distance_to(corrected.1) > 0.1);
                }
                let diagnostic = state.rebuild_contact_diagnostics(0);
                assert_eq!(diagnostic, state.rebuild_contact_diagnostics(0));
                assert!(state.rebuild_contact_diagnostics(99).is_null());
                assert_eq!(state.clone().rebuild_contact_diagnostics(0), diagnostic);
                assert_eq!(state.world.physics.snapshot_bytes(), before);
                assert_eq!(state.spaceling_snapshot(0), Some(snapshot));
                assert_eq!(state.observation(0).recovery, recovery);
                assert_eq!(state.tick(), tick);
                assert!(state.set_rebuild_contact_frame(0, false));
                assert_eq!(state.rebuild_candidate(0).unwrap(), original);
            }
        }
    }

    #[test]
    fn rebuild_contact_frame_keeps_eligibility_and_full_build_interval() {
        let state = stranded(SurfaceMotionPreset::Orbit, 0.0);
        for enabled in [false, true] {
            let mut s = state.clone();
            s.set_rebuild_contact_frame(0, enabled);
            s.update_recovery(Duration::from_secs(4));
            assert!(!s.vehicle_available(0));
            assert!(s.observation(0).recovery.unwrap().rebuild_progress > 0.49);
            s.world.planets[0].owner_id = Some(1);
            assert_eq!(
                s.rebuild_candidate(0),
                Err(SurfaceRecoveryStatus::NeedOwnedPlanet)
            );
            s.update_recovery(Duration::from_secs(30));
            assert_eq!(s.observation(0).recovery.unwrap().rebuild_progress, 0.0);
            s.world.planets[0].owner_id = Some(0);
            s.update_recovery(Duration::from_secs(30));
            assert_eq!(s.observation(0).recovery.unwrap().rebuild_progress, 0.0);
            s.update_recovery(Duration::from_secs(4));
            assert_eq!(s.observation(0).recovery.unwrap().rebuild_progress, 0.5);
            assert!(!s.vehicle_available(0));
            let mut completed = s.clone();
            let point = completed.rebuild_candidate(0).unwrap().1;
            let frame = completed.planet_motion(0);
            completed.update_recovery(Duration::from_secs(4));
            let report = completed
                .observation(0)
                .recovery
                .unwrap()
                .placement
                .unwrap();
            let expected = (point - frame.position).rotate_radians(-frame.angle);
            assert!(report.standing.distance_to(expected) < 0.0001);
            let snapshot = s.spaceling_snapshot(0).unwrap();
            let body = s.pilots[0].body.as_ref().unwrap().body();
            s.world.physics.world.set_velocity(
                body,
                snapshot.motion.linear_velocity + Vec2::new(snapshot.up.y, -snapshot.up.x) * 5.0,
                snapshot.motion.angular_velocity,
                true,
            );
            idle(&mut s, 1);
            let rejected = s.rebuild_candidate(0);
            assert!(matches!(
                rejected,
                Err(SurfaceRecoveryStatus::NeedSettle
                    | SurfaceRecoveryStatus::NeedSupport
                    | SurfaceRecoveryStatus::NeedBalance)
            ));
            s.set_rebuild_contact_frame(0, !enabled);
            assert_eq!(s.rebuild_candidate(0), rejected);
            s.world.physics.world.set_pose(
                body,
                snapshot.motion.position + snapshot.up * 10.0,
                snapshot.motion.angle,
                true,
            );
            idle(&mut s, 1);
            assert_eq!(
                s.rebuild_candidate(0),
                Err(SurfaceRecoveryStatus::NeedSupport)
            );
            assert_eq!(s.observation(0).recovery.unwrap().rebuild_progress, 0.0);
        }
    }
}
