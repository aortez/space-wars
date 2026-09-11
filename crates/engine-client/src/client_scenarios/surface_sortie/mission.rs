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

pub(crate) const ARENA_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-arena",
    capabilities: ScenarioCapabilities {
        game_over: true,
        ..TRAVEL_REGISTRATION.capabilities
    },
    controls_help: "Generated three-planet arena: P1 human, P2 mission bot. The launch seed reproduces planet sizes, spacing and motion. Land, exit and raise a flag where you stand; destroying its footing returns ownership to neutral. The bot chooses destinations, captures on foot, boards and travels onward. A/Space thrusts or jumps; left/right turns or walks; Down/S brakes; B/X exits or boards. Hold RB/J for swept cruise. RT/LB or E fires the laser aboard and mines on foot; gamepad X or K launches a missile. On foot, right stick aims, Y/T changes cut size, and holding A in the air uses the jetpack. Stand still on owned ground to rebuild after losing your ship. Hold A+B+Down for three seconds to scuttle a stranded full ship. Settings adjust asteroid arrivals and strength across all three planets. Start/Esc pauses; R restarts the same seed. Pilot health persists on foot and in the pod. Ship loss ejects you with 3 seconds of protection; later laser, missile, hard-impact and solar heat damage can kill the pilot. Pilot death ends the round even with owned planets; simultaneous deaths draw. Losing your ship and flags alone does not eliminate you. Use Play again to restart the same seed. Select spacewars-terrain-arena-duel to watch two mission bots.",
    create: create_arena_human,
    ..TRAVEL_REGISTRATION
};
pub(crate) const ARENA_DUEL_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-arena-duel",
    create: create_arena_duel,
    ..ARENA_REGISTRATION
};

struct MaterialMissionClientScenario {
    sortie: SurfaceSortieClientScenario,
    pilots: [MaterialMissionPilot; 2],
    duel: bool,
    arena: bool,
}
fn create(seed: u64, settings: &Settings, duel: bool, arena: bool) -> Box<dyn ClientScenario> {
    let mut state = if arena {
        SurfaceSortieScenario::init_material_match(seed)
    } else {
        SurfaceSortieScenario::init_material_travel(seed, false)
    };
    state.set_asteroid_pressure(settings.material_combat.asteroids);
    Box::new(MaterialMissionClientScenario {
        sortie: SurfaceSortieClientScenario { state },
        duel,
        arena,
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
    Ok(create(seed, settings, false, false))
}
fn create_duel(
    seed: u64,
    settings: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(create(seed, settings, true, false))
}
fn create_arena_human(
    seed: u64,
    settings: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(create(seed, settings, false, true))
}
fn create_arena_duel(
    seed: u64,
    settings: &Settings,
    _: Viewport,
    _: ScenarioStartMode,
    _: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(create(seed, settings, true, true))
}
impl ClientScenario for MaterialMissionClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        if self.arena {
            return if self.duel {
                &ARENA_DUEL_REGISTRATION
            } else {
                &ARENA_REGISTRATION
            };
        }
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
        if dt.is_zero() || self.is_game_over() {
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
    fn is_game_over(&self) -> bool {
        self.sortie.state.match_outcome().is_some()
    }
    fn game_over_message(&self) -> Option<String> {
        use scenario_spacewars::surface_sortie::match_rules::MatchOutcome;
        self.sortie
            .state
            .match_outcome()
            .map(|outcome| match outcome {
                MatchOutcome::Winner(owner) => {
                    format!("Player {} wins / opposing pilot lost", owner.index() + 1)
                }
                MatchOutcome::Draw => "Draw / both pilots lost".to_owned(),
            })
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
    fn physical_finished_match_supplies_menu_result_freezes_bots_and_restarts_healthy() {
        // A recorded lethal laser encounter exercises the terminal UI. The
        // former fixture relied on a missile flinging a pod into the boundary.
        let mut state = SurfaceSortieScenario::init_material_combat(7);
        state.enable_match_rules();
        let mut fighters = [0, 1].map(|seat| {
            RulePilotV4::with_combat_breaks(
                BrainReset {
                    actor: PlayerId::from_index(seat).unwrap(),
                    episode_seed: 7,
                },
                engine_common::CombatBreakSettings {
                    interval_seconds: 0,
                    duration_seconds: 4,
                },
            )
        });
        for _ in 0..180 * 60 {
            let mut actions = Vec::new();
            for (seat, brain) in fighters.iter_mut().enumerate() {
                let o = state.combat_observation(seat, brain.site_request());
                actions.extend(brain.intent(&o).encode(PlayerId::from_index(seat).unwrap()));
            }
            SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
            if state.match_outcome().is_some() {
                break;
            }
        }
        assert!(
            state.match_outcome().is_some(),
            "physical match did not finish"
        );
        let settings = Settings::default();
        let mut client = create(42, &settings, true, true);
        let client = client
            .as_any_mut()
            .downcast_mut::<MaterialMissionClientScenario>()
            .unwrap();
        client.sortie.state = state;
        assert!(client.is_game_over());
        assert_eq!(
            client.game_over_message().as_deref(),
            Some("Player 2 wins / opposing pilot lost")
        );
        let before = SurfaceSortieScenario::observe(&client.sortie.state);
        let brains_before = client.pilots.each_ref().map(|p| p.telemetry().clone());
        client.step(
            &[SurfaceSortieAction {
                primary_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            Duration::from_secs(1),
        );
        assert_eq!(
            SurfaceSortieScenario::observe(&client.sortie.state).payload,
            before.payload
        );
        assert_eq!(
            client.pilots.each_ref().map(|p| p.telemetry().clone()),
            brains_before
        );
        let reset = create(42, &settings, true, true);
        assert!(!reset.is_game_over());
        assert_eq!(reset.game_over_message(), None);
        let reset = reset
            .as_any()
            .downcast_ref::<MaterialMissionClientScenario>()
            .unwrap();
        assert_eq!(reset.sortie.state.observation(0).tick, 0);
        assert!(
            reset
                .sortie
                .state
                .match_observation()
                .unwrap()
                .pilots
                .iter()
                .all(|p| p.alive() && p.health == 100.0)
        );
    }

    #[test]
    fn mission_hosts_own_bot_inputs_preserve_pause_and_reset_and_render_destinations() {
        for (duel, arena) in [(false, false), (true, false), (false, true), (true, true)] {
            let settings = Settings::default();
            let mut host = create(42, &settings, duel, arena);
            let host = host
                .as_any_mut()
                .downcast_mut::<MaterialMissionClientScenario>()
                .unwrap();
            let before = host.pilots[1].telemetry().clone();
            assert_eq!(host.registration().capabilities.game_over, arena);
            assert_eq!(host.sortie.state.match_observation().is_some(), arena);
            assert!(!host.is_game_over());
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
            if arena {
                assert_eq!(
                    host.sortie.state.mission_observation(0, None).planets.len(),
                    3
                );
                assert!(host.pilots[1].telemetry().target.is_some());
                assert_eq!(host.pilots[0].telemetry().target.is_some(), duel);
            } else {
                assert_eq!(host.pilots[1].telemetry().target, Some(0));
                assert_eq!(
                    host.pilots[0].telemetry().target,
                    if duel { Some(1) } else { None }
                );
            }
            assert_eq!(host.sortie.state.combat_telemetry(1).shells_fired, 0);
            let frames = host.render_frames(RenderBackend::Raster, Viewport::new(800.0, 480.0));
            let expected = format!("AI: {}", host.pilots[1].label());
            assert!(
                frames[1].layers.iter().flat_map(|l| &l.primitives).any(
                    |p| matches!(p,engine_common::RenderPrimitive::Text(t) if t.text==expected)
                )
            );
            let reset = create(42, &settings, duel, arena);
            let reset = reset
                .as_any()
                .downcast_ref::<MaterialMissionClientScenario>()
                .unwrap();
            assert_eq!(reset.pilots[1].telemetry(), &before);
            assert!(!reset.is_game_over());
            if arena {
                assert!(
                    reset
                        .sortie
                        .state
                        .match_observation()
                        .unwrap()
                        .pilots
                        .iter()
                        .all(|p| p.health == 100.0 && p.alive())
                );
            }
        }
    }
}
