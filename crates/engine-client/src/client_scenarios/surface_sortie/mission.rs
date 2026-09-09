use super::*;
use spacewars_ai::mission_pilot::MaterialMissionPilot;

pub(crate) const TRAVEL_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-travel",
    controls_help: "Two-planet playtest: P1 human, P2 mission bot. Both start beside different neutral planets. The bot chooses another unowned planet, takes off, travels there, lands, exits, claims, boards and departs before choosing again. Once both planets are owned it patrols and fights; losing ownership creates a new objective. Ship loss delegates to the same pod and rebuilding controls. Watch its destination and current task. A/Space thrusts or jumps; left/right turns or walks; Down/S brakes; B/X exits or boards. Hold RB/J for swept cruise. RT/LB or E fires the laser aboard and mines on foot; gamepad X or K launches a missile. On foot, right stick aims, Y/T changes cut size, and holding A in the air uses the jetpack. Stand still to claim or rebuild. Settings adjust asteroid arrivals and strength across both planets; Off gives a quiet route trial. Start/Esc pauses; R restarts. This controlled experiment has no match victory screen; pods and spacelings remain invulnerable. Select spacewars-terrain-travel-duel to watch two mission bots.",
    create: create_human,
    ..TERRAIN_REGISTRATION
};
pub(crate) const TRAVEL_DUEL_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-travel-duel",
    controls_help: TRAVEL_REGISTRATION.controls_help,
    create: create_duel,
    ..TRAVEL_REGISTRATION
};

struct MaterialMissionClientScenario {
    sortie: SurfaceSortieClientScenario,
    pilots: [MaterialMissionPilot; 2],
    duel: bool,
}
fn create(seed: u64, settings: &Settings, duel: bool) -> Box<dyn ClientScenario> {
    let mut state = SurfaceSortieScenario::init_material_travel(seed, false);
    state.set_asteroid_pressure(settings.material_combat.asteroids);
    Box::new(MaterialMissionClientScenario {
        sortie: SurfaceSortieClientScenario { state },
        duel,
        pilots: std::array::from_fn(|seat| {
            MaterialMissionPilot::new(
                BrainReset {
                    actor: PlayerId::from_index(seat).unwrap(),
                    episode_seed: seed,
                },
                settings.combat_breaks,
            )
        }),
    })
}
fn create_human(
    seed: u64,
    settings: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(create(seed, settings, false))
}
fn create_duel(
    seed: u64,
    settings: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(create(seed, settings, true))
}
impl ClientScenario for MaterialMissionClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        if self.duel {
            &TRAVEL_DUEL_REGISTRATION
        } else {
            &TRAVEL_REGISTRATION
        }
    }
    fn tick_model(&self) -> TickModel {
        self.sortie.tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        if dt.is_zero() {
            return self.sortie.step(&[], dt);
        }
        let mut actions = if self.duel {
            Vec::new()
        } else {
            human_pilot_actions(actions)
        };
        for seat in if self.duel { 0..2 } else { 1..2 } {
            let o = self
                .sortie
                .state
                .mission_observation(seat, self.pilots[seat].site_request());
            actions.extend(
                self.pilots[seat]
                    .intent(&o)
                    .encode(PlayerId::from_index(seat).unwrap()),
            );
        }
        self.sortie.step(&actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, benchmark: bool) -> Vec<Action> {
        if self.duel {
            Vec::new()
        } else {
            human_pilot_actions(&self.sortie.map_input(input, benchmark))
        }
    }
    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let mut frames = self.sortie.render_frames(renderer, viewport);
        for seat in if self.duel { 0..2 } else { 1..2 } {
            pilot_hud_for(&mut frames, seat, &self.pilots[seat].label());
        }
        frames
    }
    fn frame_layout(&self) -> FrameLayout {
        self.sortie.frame_layout()
    }
    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    #[cfg(test)]
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mission_hosts_own_bot_inputs_preserve_pause_and_reset_and_render_destinations() {
        for duel in [false, true] {
            let settings = Settings::default();
            let mut host = create(42, &settings, duel);
            let host = host
                .as_any_mut()
                .downcast_mut::<MaterialMissionClientScenario>()
                .unwrap();
            let before = host.pilots[1].telemetry().clone();
            let input = [SurfaceWeaponAction {
                laser: true,
                cannon: true,
            }
            .encode(PlayerId::PLAYER_2)];
            host.step(&input, Duration::ZERO);
            assert_eq!(host.pilots[1].telemetry(), &before);
            assert_eq!(host.sortie.state.observation(0).tick, 0);
            for _ in 0..120 {
                host.step(&input, Duration::from_nanos(16_666_667));
            }
            assert_eq!(host.pilots[1].telemetry().target, Some(0));
            assert_eq!(
                host.pilots[0].telemetry().target,
                if duel { Some(1) } else { None }
            );
            assert_eq!(host.sortie.state.combat_telemetry(1).shells_fired, 0);
            let frames = host.render_frames(RenderBackend::Raster, Viewport::new(800.0, 480.0));
            let expected = format!("AI: {}", host.pilots[1].label());
            assert!(
                frames[1].layers.iter().flat_map(|l| &l.primitives).any(
                    |p| matches!(p,engine_common::RenderPrimitive::Text(t) if t.text==expected)
                )
            );
            let reset = create(42, &settings, duel);
            let reset = reset
                .as_any()
                .downcast_ref::<MaterialMissionClientScenario>()
                .unwrap();
            assert_eq!(reset.pilots[1].telemetry(), &before);
        }
    }
}
