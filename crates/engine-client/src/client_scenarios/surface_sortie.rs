use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_spacewars::PlayerId;
use scenario_spacewars::surface_sortie::{
    SurfaceMiningAction, SurfaceMotionPreset, SurfaceSortieAction, SurfaceSortieScenario,
    SurfaceSortieState,
};
use spacewars_ai::{
    BrainReset,
    pilot::{PilotBrain, RulePilotV1},
};

use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode, spaceling_lab::spaceling_controls,
};
use crate::{
    input::{ClientInput, GameKey},
    render::{FrameLayout, Viewport},
};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "surface-sortie",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        pointer_input: false,
        player_zoom: false,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Surface Sortie: left/right or A/D turns aboard, walks on foot. A / Space thrusts aboard and jumps on foot. Down / S brakes. Face away from the planet for landing assist; settle on both rear feet. B / X exits or boards at the cyan hatch. Release controls after transferring. Stand beside the amber terminal for 3 seconds to capture; leaving, jumping, or knockdown resets progress. A friendly outpost repairs a nearby landed ship at 5%/s. The ship starts at 75% health. Minimap: red ship, orange spaceling, cyan planet, square outpost (owner color). Start / Esc pauses; R restarts. No weapons or rebuilding.",
    create,
};

/// Another selectable preset of the same scenario, input mapping and renderer.
/// Keeping a distinct registry ID makes the choice persist and remain reachable
/// through ordinary keyboard/gamepad launcher navigation without new buttons.
pub(super) const ORBIT_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "surface-sortie-orbit",
    create: create_orbit,
    ..REGISTRATION
};

pub(super) const GENERATED_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "surface-sortie-generated",
    controls_help: "Compatibility diagnostic, not yet a playable surface preset. Uses ordinary generated gravity and motion with the unchanged lab controls: takeoff and on-foot support can fail. Planet 0, initially away from the sun; use the launch seed to reproduce. A / Space thrusts or jumps; left/right turns or walks; B / X exits or boards a landed ship. Start / Esc pauses; R restarts. Use surface-sortie or surface-sortie-orbit for the tuned gameplay loop.",
    create: create_generated,
    ..REGISTRATION
};

pub(super) const WORLD_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "surface-sortie-world",
    controls_help: "Experimental Surface V1 world: generated planet sizes/layout with gentler gravity, bounded spin, sun-matched orbits and extra flight clearance. Same natural landing, capture and repair controls as Surface Sortie; no change to ordinary Spacewars. Planet 0, initially away from the sun; the launch seed reproduces it. A / Space thrusts or jumps; left/right turns or walks; Down / S brakes; B / X exits or boards beside a landed ship's cyan hatch. Stand by the amber terminal to capture and repair. Start / Esc pauses; R restarts. Passive landings and long-idle support still have known limits; see docs/surface-compatibility.md.",
    create: create_world,
    ..REGISTRATION
};

pub(super) const EXPEDITION_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "surface-expedition",
    controls_help: concat!(
        "Surface Expedition: land rear-first, B / X to get out, then stand still on the surface for 3 seconds to claim a neutral planet. Your flag is planted where you stand. An existing enemy flag must be approached (within 3 units) and lowered for 3 seconds before your own flag can rise for another 3 seconds. Opponents near the flag pause progress. Leaving, jumping or losing balance resets the unfinished stage; a fully lowered flag leaves the planet neutral. Minimap colors show planet ownership and triangles locate flags. ",
        "A / Space thrusts or jumps; left/right turns or walks; Down / S brakes; B / X boards your own landed ship or pod at the cyan hatch. Choose 1 or 2 players in Settings. P2 keyboard: numpad 4/6 turns/walks, 8 thrusts/jumps, 5 brakes, 2 boards/exits. Each assigned gamepad controls its own pilot; release controls after transfers and vehicle loss/replacement. Start / Esc pauses; R restarts. ",
        "Loss drill: hold A+B+Down (Space+X+S; P2 numpad 8+2+5) for 3 seconds to destroy your assigned full ship; release early to cancel. An occupied ship leaves a flyable pod; an empty ship leaves the existing spaceling alive, not an empty pod. Land the pod rear-first and exit, even on a neutral planet. With no full ship, stand still on an owned planet for 8 seconds to rebuild nearby; no outpost or flag proximity is required. Losing support, balance or ownership resets progress. If space is blocked, move to another clear spot. Board the replacement normally after it settles. Ships start at full health; pods and spacelings remain invulnerable in this lab. No outposts, repair, weapons, ship swapping or remote rescue; ordinary Spacewars is unchanged."
    ),
    create: create_expedition,
    ..REGISTRATION
};

struct SurfaceSortieClientScenario {
    state: SurfaceSortieState,
}

pub(super) const TERRAIN_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain",
    controls_help: "Destructible Expedition: land on both rear feet, exit, and stand still 3s to raise your planet flag. A/Space thrusts or jumps; B/X exits or boards. Left/right turns or walks; Down/S brakes. Right stick aims the mining beam; RT or LB mines; Y changes cut size (one cell, radius 1, radius 3). Keyboard E mines, T changes size; arrows aim. P2 uses numpad 4/6, 8, 5, 2 for move, thrust/jump, brake, transfer; End mines, PageDown changes size. A missing flag footing neutralizes the planet. Land an escape pod and stand on owned ground 8s to rebuild. Hold A+B+Down 3s for the loss drill. Select 1 or 2 players in Settings. Start/Esc pauses. No landing pad or repair terminal.",
    create: create_material,
    ..EXPEDITION_REGISTRATION
};

pub(super) const PILOT_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-ai",
    controls_help: "Material pilot AI playtest: P1 is human; P2 flies, lands, exits, claims a neutral planet, boards and departs using the same controls and physics. Watch its goal in the P2 view. After one sortie it holds above the planet. Enemy flag navigation, mining and vehicle recovery are future AI slices; a blocked goal is shown explicitly. P1 controls match Destructible Expedition: A/Space thrusts or jumps; B/X transfers; left/right turns or walks; Down/S brakes; right stick aims, RT/LB mines, Y changes size. Start/Esc pauses; R restarts both pilots. Select spacewars-terrain for one or two human pilots.",
    create: create_pilot,
    ..TERRAIN_REGISTRATION
};

/// Host-owned policy. The scenario still consumes only ordinary encoded actions.
struct MaterialPilotClientScenario {
    sortie: SurfaceSortieClientScenario,
    brain: RulePilotV1,
}

fn create_pilot(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    use scenario_spacewars::surface_sortie::pilot::MaterialFlightStart;
    Ok(Box::new(MaterialPilotClientScenario {
        sortie: SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init_material_flight(
                seed,
                2,
                &[(
                    PlayerId::PLAYER_2,
                    MaterialFlightStart {
                        bearing: std::f32::consts::PI,
                        altitude: 45.0,
                        radial_speed: -5.0,
                        lateral_speed: 3.0,
                        heading_offset: -0.45,
                    },
                )],
            ),
        },
        brain: RulePilotV1::new(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: seed,
        }),
    }))
}

fn human_pilot_actions(actions: &[Action]) -> Vec<Action> {
    actions
        .iter()
        .filter(|action| {
            SurfaceSortieAction::decode(action)
                .is_some_and(|(owner, _)| owner == PlayerId::PLAYER_1)
                || SurfaceMiningAction::decode(action).is_some_and(|(seat, _)| seat == 0)
        })
        .cloned()
        .collect()
}

impl ClientScenario for MaterialPilotClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &PILOT_REGISTRATION
    }
    fn tick_model(&self) -> TickModel {
        self.sortie.tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        if dt.is_zero() {
            return self.sortie.step(&[], dt);
        }
        let observation = self
            .sortie
            .state
            .pilot_observation(1, self.brain.site_request());
        let mut actions = human_pilot_actions(actions);
        actions.push(self.brain.intent(&observation).encode(PlayerId::PLAYER_2));
        self.sortie.step(&actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, benchmark: bool) -> Vec<Action> {
        human_pilot_actions(&self.sortie.map_input(input, benchmark))
    }
    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        use engine_common::{RenderColor, RenderPrimitive};
        let mut frames = self.sortie.render_frames(renderer, viewport);
        // Reuse the existing HUD strip; replacing the control hint leaves the
        // physical viewport and other pilot's controls unobscured at 800x480.
        for layer in &mut frames[1].layers {
            for primitive in &mut layer.primitives {
                if let RenderPrimitive::Text(text) = primitive
                    && text.text == "A: thrust/jump  B: board/exit"
                {
                    let status = self.brain.telemetry();
                    text.text = format!(
                        "AI: {}",
                        status.blocked_reason.unwrap_or(status.goal.label())
                    );
                    text.color = RenderColor::rgb(1.0, 0.82, 0.25);
                }
            }
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

fn create_material(
    seed: u64,
    settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init_material(
            seed,
            settings.surface_expedition.players.count(),
        ),
    }))
}

fn create_expedition(
    seed: u64,
    settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init_expedition(
            seed,
            settings.surface_expedition.players.count(),
        ),
    }))
}

fn create(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init(SurfaceMotionPreset::default(), seed),
    }))
}

fn create_orbit(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init(SurfaceMotionPreset::Orbit, seed),
    }))
}

fn create_generated(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init(SurfaceMotionPreset::Generated, seed),
    }))
}

fn create_world(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(SurfaceSortieClientScenario {
        state: SurfaceSortieScenario::init(SurfaceMotionPreset::GeneratedSurfaceV1, seed),
    }))
}

impl ClientScenario for SurfaceSortieClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        if self.state.has_material_ground() {
            return &TERRAIN_REGISTRATION;
        }
        if self.state.travel_enabled() {
            return &EXPEDITION_REGISTRATION;
        }
        match self.state.motion_preset() {
            SurfaceMotionPreset::Orbit => &ORBIT_REGISTRATION,
            SurfaceMotionPreset::Generated => &GENERATED_REGISTRATION,
            SurfaceMotionPreset::GeneratedSurfaceV1 => &WORLD_REGISTRATION,
            _ => &REGISTRATION,
        }
    }
    fn tick_model(&self) -> TickModel {
        SurfaceSortieScenario::tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        SurfaceSortieScenario::step(&mut self.state, actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, _benchmark: bool) -> Vec<Action> {
        let mut actions: Vec<_> = (0..self.state.player_count())
            .map(|player| {
                let (horizontal, primary_held, interact_held) = surface_controls(input, player);
                SurfaceSortieAction {
                    horizontal,
                    primary_held,
                    interact_held,
                    brake_held: input.surface_sortie_brake_held(player),
                }
                .encode(PlayerId::from_index(player).expect("bounded player seat"))
            })
            .collect();
        if self.state.has_material_ground() {
            for player in 0..self.state.player_count() {
                let (aim, held, cycle) = input.surface_mining_input(player);
                actions.push(
                    SurfaceMiningAction { aim, held, cycle }
                        .encode(PlayerId::from_index(player).expect("bounded seat")),
                );
            }
        }
        actions
    }
    fn render_frames(&self, _renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let count = self.state.player_count();
        let aspect = viewport.aspect_ratio() / count as f32;
        let mut frames = Vec::with_capacity(count * 2);
        for player in 0..count {
            frames.push(SurfaceSortieScenario::player_frame(&self.state, player));
        }
        for player in 0..count {
            frames.push(SurfaceSortieScenario::minimap_frame(
                &self.state,
                player,
                aspect,
            ));
        }
        frames
    }
    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::PlayerViewsWithMinimaps
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

fn surface_controls(input: &ClientInput, player: usize) -> (f32, bool, bool) {
    if player == 0 {
        return spaceling_controls(input);
    }
    let (gamepad_walk, gamepad_primary, gamepad_interact) = input.spaceling_gamepad_input(player);
    let horizontal = match (
        input.is_pressed(GameKey::P2TurnLeft),
        input.is_pressed(GameKey::P2TurnRight),
    ) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        (true, true) => 0.0,
        (false, false) => gamepad_walk,
    };
    (
        horizontal,
        input.is_pressed(GameKey::P2Thrust)
            || input.is_pressed(GameKey::P2Laser)
            || gamepad_primary,
        input.is_pressed(GameKey::P2Reverse) || gamepad_interact,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{GameKey, GamepadInput, GamepadSeatInput};
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn material_ai_host_ignores_p2_hardware_pauses_and_restarts_its_policy() {
        let make = || {
            PILOT_REGISTRATION
                .create(
                    42,
                    &Settings::default(),
                    Viewport::new(800.0, 480.0),
                    ScenarioStartMode::Normal,
                )
                .unwrap()
        };
        let mut host = make();
        let host = host
            .as_any_mut()
            .downcast_mut::<MaterialPilotClientScenario>()
            .unwrap();
        let mut reference = host.sortie.state.clone();
        let mut brain = host.brain.clone();
        let interfering = [
            SurfaceSortieAction {
                horizontal: 1.0,
                primary_held: true,
                interact_held: true,
                brake_held: true,
            }
            .encode(PlayerId::PLAYER_2),
            SurfaceMiningAction {
                aim: engine_core::Vec2::Y,
                held: true,
                cycle: true,
            }
            .encode(PlayerId::PLAYER_2),
        ];
        let before = host.brain.telemetry().clone();
        host.step(&interfering, Duration::ZERO);
        assert_eq!(host.brain.telemetry(), &before);
        assert_eq!(host.sortie.state.observation(1).tick, 0);
        for _ in 0..120 * 60 {
            let o = reference.pilot_observation(1, brain.site_request());
            let action = brain.intent(&o);
            SurfaceSortieScenario::step(
                &mut reference,
                &[action.encode(PlayerId::PLAYER_2)],
                Duration::from_nanos(16_666_667),
            );
            host.step(&interfering, Duration::from_nanos(16_666_667));
            assert_eq!(host.sortie.state.observation(1), reference.observation(1));
            if host.brain.telemetry().completed_tick.is_some() {
                break;
            }
        }
        assert!(
            host.brain.telemetry().completed_tick.is_some(),
            "{:?}",
            host.brain.telemetry()
        );
        let frames = host.render_frames(RenderBackend::Raster, Viewport::new(800.0, 480.0));
        assert_eq!(frames.len(), 4);
        assert!(frames[1].layers.iter().flat_map(|l| &l.primitives).any(
            |p| matches!(p, engine_common::RenderPrimitive::Text(t) if t.text.starts_with("AI: "))
        ));
        let restarted = make();
        let restarted = restarted
            .as_any()
            .downcast_ref::<MaterialPilotClientScenario>()
            .unwrap();
        assert_eq!(restarted.brain.telemetry(), &before);
        assert_eq!(restarted.sortie.state.observation(1).tick, 0);
    }

    #[test]
    fn material_seats_map_mining_without_ship_weapons_and_release_on_disconnect() {
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        let mut input = ClientInput::new(Rc::clone(&pads));
        let mut settings = Settings::default();
        settings.surface_expedition.players = engine_common::SurfaceExpeditionPlayers::Two;
        let mut scenario = TERRAIN_REGISTRATION
            .create(
                42,
                &settings,
                Viewport::new(800.0, 480.0),
                ScenarioStartMode::Normal,
            )
            .unwrap();
        pads.borrow_mut().set_seat(
            1,
            GamepadSeatInput {
                connected: true,
                right_stick_y: -1.0,
                right_trigger: 1.0,
                north: true,
                ..Default::default()
            },
        );
        let actions = scenario.map_input(&mut input, false);
        let mining: Vec<_> = actions
            .iter()
            .filter_map(SurfaceMiningAction::decode)
            .collect();
        assert_eq!(mining[0], (0, SurfaceMiningAction::default()));
        assert_eq!(
            mining[1],
            (
                1,
                SurfaceMiningAction {
                    aim: -engine_core::Vec2::Y,
                    held: true,
                    cycle: true
                }
            )
        );
        assert!(
            actions
                .iter()
                .all(|a| scenario_spacewars::SpacewarsAction::decode(a).is_none())
        );
        scenario.step(&actions, Duration::from_secs_f64(1.0 / 60.0));
        assert_eq!(scenario.registration().id, "spacewars-terrain");
        assert_eq!(
            scenario
                .render_frames(RenderBackend::Raster, Viewport::new(800.0, 480.0))
                .len(),
            4
        );
        pads.borrow_mut().disconnect_seat(1);
        let released = scenario.map_input(&mut input, false);
        assert!(
            released
                .iter()
                .filter_map(SurfaceMiningAction::decode)
                .all(|(_, action)| action == SurfaceMiningAction::default())
        );
    }

    #[test]
    fn material_claim_and_excavation_render_with_the_shared_pilot_hud() {
        let mut scenario = SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init_material(42, 1),
        };
        let dt = Duration::from_nanos(16_666_667);
        for _ in 0..90 {
            scenario.step(&[], dt);
        }
        scenario.step(
            &[SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_1)],
            dt,
        );
        for _ in 0..220 {
            scenario.step(
                &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
                dt,
            );
        }
        let observation = scenario.state.observation(0);
        assert_eq!(
            observation.planet_claim.as_ref().unwrap().owner,
            Some(PlayerId::PLAYER_1)
        );
        check_render(
            &scenario,
            Viewport::new(800.0, 480.0),
            "on-foot-material-claimed",
        );
        let flag = observation.planet_claim.unwrap().flag.unwrap();
        for tick in 0..25 {
            scenario.step(
                &[SurfaceMiningAction {
                    aim: flag.position - scenario.state.observation(0).position,
                    held: tick > 1,
                    cycle: tick == 0,
                }
                .encode(PlayerId::PLAYER_1)],
                dt,
            );
        }
        assert!(scenario.state.terrain_diagnostics().removed_cells > 0);
        check_render(
            &scenario,
            Viewport::new(800.0, 480.0),
            "on-foot-material-excavated",
        );
    }

    #[test]
    fn expedition_seats_have_independent_keyboard_pad_and_disconnect_controls() {
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        let mut input = ClientInput::new(Rc::clone(&pads));
        let settings = Settings {
            surface_expedition: engine_common::SurfaceExpeditionSettings {
                players: engine_common::SurfaceExpeditionPlayers::Two,
            },
            ..Settings::default()
        };
        let scenario = EXPEDITION_REGISTRATION
            .create(
                0,
                &settings,
                Viewport::new(1280.0, 720.0),
                ScenarioStartMode::Normal,
            )
            .unwrap();
        input.press(GameKey::P2TurnLeft);
        input.press(GameKey::P2Thrust);
        input.press(GameKey::P2Reverse);
        let actions = scenario.map_input(&mut input, false);
        assert_eq!(actions.len(), 2);
        assert_eq!(
            SurfaceSortieAction::decode(&actions[0]),
            Some((PlayerId::PLAYER_1, SurfaceSortieAction::default()))
        );
        assert_eq!(
            SurfaceSortieAction::decode(&actions[1]),
            Some((
                PlayerId::PLAYER_2,
                SurfaceSortieAction {
                    horizontal: -1.0,
                    primary_held: true,
                    interact_held: true,
                    brake_held: false,
                }
            ))
        );
        input.release(GameKey::P2TurnLeft);
        input.release(GameKey::P2Thrust);
        input.release(GameKey::P2Reverse);
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_right: true,
                ..GamepadSeatInput::default()
            },
        );
        pads.borrow_mut().set_seat(
            1,
            GamepadSeatInput {
                connected: true,
                dpad_left: true,
                dpad_down: true,
                south: true,
                east: true,
                ..GamepadSeatInput::default()
            },
        );
        let actions = scenario.map_input(&mut input, false);
        assert_eq!(
            SurfaceSortieAction::decode(&actions[0])
                .unwrap()
                .1
                .horizontal,
            1.0
        );
        assert_eq!(
            SurfaceSortieAction::decode(&actions[1]).unwrap().1,
            SurfaceSortieAction {
                horizontal: -1.0,
                primary_held: true,
                interact_held: true,
                brake_held: true,
            }
        );
        pads.borrow_mut().disconnect_seat(1);
        let actions = scenario.map_input(&mut input, false);
        assert_eq!(
            SurfaceSortieAction::decode(&actions[1]).unwrap().1,
            SurfaceSortieAction::default()
        );
        assert_eq!(
            SurfaceSortieAction::decode(&actions[0])
                .unwrap()
                .1
                .horizontal,
            1.0
        );
        let solo = EXPEDITION_REGISTRATION
            .create(
                0,
                &Settings::default(),
                Viewport::new(1280.0, 720.0),
                ScenarioStartMode::Normal,
            )
            .unwrap();
        assert_eq!(solo.map_input(&mut input, false).len(), 1);
        let fixture = REGISTRATION
            .create(
                0,
                &settings,
                Viewport::new(1280.0, 720.0),
                ScenarioStartMode::Normal,
            )
            .unwrap();
        assert_eq!(
            fixture.map_input(&mut input, false).len(),
            1,
            "Expedition settings cannot change the pinned labs"
        );
    }

    #[test]
    fn expedition_two_player_frames_have_separate_cameras_maps_and_readable_huds() {
        let mut scenario = SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init_expedition(0, 2),
        };
        for _ in 0..120 {
            scenario.step(&[], Duration::from_secs_f64(1.0 / 60.0));
        }
        for on_foot in [false, true] {
            if on_foot {
                scenario.step(
                    &[PlayerId::PLAYER_1, PlayerId::PLAYER_2].map(|player| {
                        SurfaceSortieAction {
                            interact_held: true,
                            ..Default::default()
                        }
                        .encode(player)
                    }),
                    Duration::from_secs_f64(1.0 / 60.0),
                );
                for player in 0..2 {
                    assert_eq!(
                        scenario.state.location(player),
                        scenario_spacewars::surface_sortie::PilotLocation::OnFoot
                    );
                }
            }
            for viewport in [Viewport::new(1280.0, 720.0), Viewport::new(800.0, 1280.0)] {
                let frames = scenario.render_frames(RenderBackend::Vector, viewport);
                assert_eq!(frames.len(), 4);
                assert_ne!(frames[0].camera.center, frames[1].camera.center);
                let layout = scenario.frame_layout();
                let panes = crate::render::frame_viewports(viewport, 4, layout);
                assert_eq!(panes[0].width, viewport.width / 2.0);
                assert_eq!(panes[1].x, viewport.width / 2.0);
                for player in 0..2 {
                    assert!(panes[player + 2].x >= panes[player].x);
                    assert!(
                        panes[player + 2].x + panes[player + 2].width
                            <= panes[player].x + panes[player].width
                    );
                    let footprint = frames[player + 2]
                        .ordered_layers()
                        .into_iter()
                        .find(|layer| layer.z == 0)
                        .unwrap();
                    let engine_common::RenderPrimitive::Polygon(polygon) = &footprint.primitives[0]
                    else {
                        panic!("camera footprint");
                    };
                    let width = polygon.points[1].x - polygon.points[0].x;
                    assert!(
                        (width - frames[player].camera.height * panes[player].aspect_ratio()).abs()
                            < 0.001
                    );
                }
                let presentation = crate::render::scene_presentation_from_frames_with_layout(
                    &frames, viewport, layout,
                );
                assert_eq!(presentation.minimaps.len(), 2);
                assert!(
                    presentation
                        .minimaps
                        .iter()
                        .all(|map| !map.primitives.is_empty())
                );
                let overlay = crate::render::raster_text_overlay(&frames, viewport, layout);
                for player in 0..2 {
                    let label = format!(
                        "P{}  {}",
                        player + 1,
                        if on_foot { "ON FOOT" } else { "ABOARD" }
                    );
                    assert!(
                        overlay
                            .iter()
                            .any(|primitive| primitive.text.starts_with(&label))
                    );
                }
                let image = crate::raster::RasterRenderer::new().image_from_frames_with_layout(
                    &frames,
                    viewport,
                    layout,
                    crate::raster::RasterOptions::default(),
                );
                let pixels = image.to_rgb8().unwrap();
                for player in 0..2 {
                    let count = pixels
                        .as_slice()
                        .iter()
                        .enumerate()
                        .filter(|(i, pixel)| {
                            let x = i % pixels.width() as usize;
                            let y = i / pixels.width() as usize;
                            let own_color = if player == 0 {
                                pixel.r > 200 && pixel.g < 80
                            } else {
                                pixel.g > 200 && pixel.r < 80
                            };
                            x / (pixels.width() as usize / 2) == player
                                && y > pixels.height() as usize / 3
                                && y < pixels.height() as usize * 3 / 4
                                && own_color
                                && pixel.b < 80
                        })
                        .count();
                    assert!(
                        count > 100,
                        "player {player} geometry missing at {viewport:?}: {count}"
                    );
                }
                if let Some(directory) = std::env::var_os("SPACEWARS_SORTIE_ARTIFACTS") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let file = std::fs::File::create(directory.join(format!(
                        "expedition-pair-{}-foot-{on_foot}.png",
                        viewport.width
                    )))
                    .unwrap();
                    let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
                    encoder.set_color(png::ColorType::Rgb);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .unwrap()
                        .write_image_data(pixels.as_bytes())
                        .unwrap();
                }
            }
        }
    }

    #[test]
    fn generated_diagnostic_is_selectable_reproducible_and_renders_the_whole_world() {
        use scenario_spacewars::surface_sortie::compatibility::GeneratedSurfaceCase;
        for seed in [0, 2] {
            let mut scenario = GENERATED_REGISTRATION
                .create(
                    seed,
                    &Settings::default(),
                    Viewport::new(1280.0, 720.0),
                    ScenarioStartMode::Normal,
                )
                .unwrap();
            assert_eq!(scenario.registration().id, "surface-sortie-generated");
            for _ in 0..60 {
                scenario.step(
                    &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
                    Duration::from_secs_f64(1.0 / 60.0),
                );
            }
            let scenario = scenario
                .as_any()
                .downcast_ref::<SurfaceSortieClientScenario>()
                .unwrap();
            assert_eq!(
                scenario.state.observation(0).generated_case,
                Some(GeneratedSurfaceCase::new(seed, 0, 0))
            );
            let mut replay = SurfaceSortieClientScenario {
                state: GeneratedSurfaceCase::new(seed, 0, 0).init().unwrap(),
            };
            for _ in 0..60 {
                replay.step(
                    &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
                    Duration::from_secs_f64(1.0 / 60.0),
                );
            }
            assert_eq!(scenario.state.observation(0), replay.state.observation(0));
            for viewport in [Viewport::new(1280.0, 720.0), Viewport::new(1280.0, 1400.0)] {
                let frames = scenario.render_frames(RenderBackend::Vector, viewport);
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Raster, viewport)
                );
                let overlay =
                    crate::render::raster_text_overlay(&frames, viewport, scenario.frame_layout());
                assert!(
                    overlay
                        .iter()
                        .any(|primitive| primitive.text.starts_with("GENERATED / UNTUNED"))
                );
                let planets = frames[1]
                    .ordered_layers()
                    .iter()
                    .filter(|layer| layer.z == -10)
                    .map(|layer| layer.primitives.len())
                    .sum::<usize>();
                assert_eq!(planets, GeneratedSurfaceCase::planet_count(seed));
                check_render(
                    scenario,
                    viewport,
                    &format!(
                        "generated-seed{seed}-{}x{}",
                        viewport.width, viewport.height
                    ),
                );
            }
        }
    }

    #[test]
    fn keyboard_and_nes_pad_produce_the_same_context_neutral_intent() {
        let scenario = SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init(SurfaceMotionPreset::default(), 0),
        };
        let mut keyboard = ClientInput::default();
        for key in [
            GameKey::P1TurnLeft,
            GameKey::P1Laser,
            GameKey::P1Reverse,
            GameKey::P1Brake,
        ] {
            keyboard.press(key);
        }
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_left: true,
                dpad_down: true,
                south: true,
                east: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(Rc::clone(&pads));
        assert_eq!(
            scenario.map_input(&mut keyboard, false),
            scenario.map_input(&mut input, false)
        );
        pads.borrow_mut().set_seat(0, GamepadSeatInput::default());
        assert_eq!(
            scenario.map_input(&mut input, false),
            vec![SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)]
        );
    }

    #[test]
    fn outpost_capture_progress_and_owner_flag_render_in_both_backends() {
        for state in [
            SurfaceMotionPreset::Stationary,
            SurfaceMotionPreset::Orbit,
            SurfaceMotionPreset::GeneratedSurfaceV1,
        ]
        .map(|preset| SurfaceSortieScenario::init(preset, 0))
        .into_iter()
        {
            let mut scenario = SurfaceSortieClientScenario { state };
            let dt = Duration::from_secs_f64(1.0 / 60.0);
            for frame in 0..120 {
                scenario.step(
                    &[SurfaceSortieAction {
                        interact_held: frame == 60,
                        ..SurfaceSortieAction::default()
                    }
                    .encode(PlayerId::PLAYER_1)],
                    dt,
                );
            }
            for _ in 0..600 {
                let observation = scenario.state.observation(0);
                if observation
                    .position
                    .distance_to(observation.outpost.as_ref().unwrap().position)
                    < 2.35
                {
                    break;
                }
                scenario.step(
                    &[SurfaceSortieAction {
                        horizontal: 1.0,
                        ..SurfaceSortieAction::default()
                    }
                    .encode(PlayerId::PLAYER_1)],
                    dt,
                );
            }
            for _ in 0..60 {
                scenario.step(
                    &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
                    dt,
                );
            }
            let partial = scenario.state.observation(0).outpost.unwrap();
            assert!(partial.capture_progress > 0.0 && partial.capture_progress < 1.0);
            let viewport = Viewport::new(1280.0, 720.0);
            check_render(&scenario, viewport, "on-foot-capturing");
            for _ in 0..180 {
                scenario.step(
                    &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
                    dt,
                );
            }
            let observation = scenario.state.observation(0);
            assert_eq!(
                observation.outpost.as_ref().unwrap().owner,
                Some(observation.owner)
            );
            assert!(observation.outpost.as_ref().unwrap().repaired_health > 0.0);
            for viewport in [viewport, Viewport::new(1280.0, 1400.0)] {
                let frames = scenario.render_frames(RenderBackend::Vector, viewport);
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Raster, viewport)
                );
                let overlay =
                    crate::render::raster_text_overlay(&frames, viewport, scenario.frame_layout());
                assert!(
                    overlay
                        .iter()
                        .any(|primitive| primitive.text.starts_with("OUTPOST P1"))
                );
                assert!(
                    overlay
                        .iter()
                        .any(|primitive| primitive.text == "P1 OUTPOST")
                );
                let name = if viewport.height > viewport.width {
                    "on-foot-secured-portrait"
                } else {
                    "on-foot-secured"
                };
                check_render(&scenario, viewport, name);
                // Check the actual owner-colored flag, not just a text/model entry.
                let up = observation.outpost.as_ref().unwrap().surface_normal;
                let flag = observation.outpost.as_ref().unwrap().position
                    + up * 3.15
                    + engine_core::Vec2::new(up.y, -up.x) * 1.3;
                let projected = frames[0].camera.world_to_viewport(
                    engine_common::RenderPoint::new(flag.x, flag.y),
                    viewport.aspect_ratio(),
                );
                let image = crate::raster::RasterRenderer::new().image_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                    crate::raster::RasterOptions::default(),
                );
                let pixels = image.to_rgb8().unwrap();
                let x = (projected.x * pixels.width() as f32) as usize;
                let y = (projected.y * pixels.height() as f32) as usize;
                let pixel = pixels.as_slice()[y * pixels.width() as usize + x];
                assert!(
                    pixel.r > 200 && pixel.g < 80 && pixel.b < 80,
                    "missing owner flag: {pixel:?}"
                );
            }
        }
    }

    #[test]
    fn expedition_flags_and_planet_ownership_render_without_outposts() {
        use engine_common::{RenderColor, RenderPrimitive};
        use scenario_spacewars::surface_sortie::PlanetClaimPhase;
        let dt = Duration::from_secs_f64(1.0 / 60.0);
        for players in [1, 2] {
            let mut scenario = SurfaceSortieClientScenario {
                state: SurfaceSortieScenario::init_expedition(0, players),
            };
            for _ in 0..120 {
                scenario.step(&[], dt);
            }
            scenario.step(
                &[SurfaceSortieAction {
                    interact_held: true,
                    ..Default::default()
                }
                .encode(PlayerId::PLAYER_1)],
                dt,
            );
            for secured in [false, true] {
                for _ in 0..if secured { 180 } else { 90 } {
                    scenario.step(
                        &[SurfaceSortieAction::default().encode(PlayerId::PLAYER_1)],
                        dt,
                    );
                }
                let observation = scenario.state.observation(0);
                assert!(observation.outposts.is_empty());
                assert!(observation.outpost.is_none());
                let claim = observation.planet_claim.as_ref().unwrap();
                let flag = claim.flag.unwrap();
                assert_eq!(claim.owner, secured.then_some(PlayerId::PLAYER_1));
                assert_eq!(
                    claim.phase,
                    if secured {
                        PlanetClaimPhase::Idle
                    } else {
                        PlanetClaimPhase::Raising
                    }
                );
                if !secured {
                    assert!(flag.raised_fraction > 0.25 && flag.raised_fraction < 1.0);
                }
                for viewport in [
                    Viewport::new(1280.0, 720.0),
                    Viewport::new(800.0, 480.0),
                    Viewport::new(800.0, 1280.0),
                ] {
                    let frames = scenario.render_frames(RenderBackend::Vector, viewport);
                    assert_eq!(
                        frames,
                        scenario.render_frames(RenderBackend::Raster, viewport)
                    );
                    let layout = scenario.frame_layout();
                    let panes = crate::render::frame_viewports(viewport, frames.len(), layout);
                    let presentation = crate::render::scene_presentation_from_frames_with_layout(
                        &frames, viewport, layout,
                    );
                    assert_eq!(presentation.minimaps.len(), players);
                    let overlay = crate::render::raster_text_overlay(&frames, viewport, layout);
                    assert!(overlay.iter().any(|p| p.text
                        == if secured {
                            "Planet 0: P1".to_owned()
                        } else {
                            format!("Planet 0: Neutral / raising {:.0}%", claim.progress * 100.0)
                        }));
                    assert!(
                        overlay
                            .iter()
                            .all(|p| !p.text.to_lowercase().contains("terminal")
                                && !p.text.to_lowercase().contains("repair"))
                    );
                    // Both overviews agree: only the claimed planet changes color.
                    for map in &frames[players..] {
                        let planets = map
                            .ordered_layers()
                            .into_iter()
                            .find(|layer| layer.z == -10)
                            .unwrap();
                        assert_eq!(planets.primitives.len(), observation.planet_claims.len());
                        for (planet, primitive) in planets.primitives.iter().enumerate() {
                            let RenderPrimitive::Circle(circle) = primitive else {
                                panic!("planet circle");
                            };
                            let color = circle.fill.unwrap().color;
                            if secured && planet == 0 {
                                assert_eq!(color, RenderColor::rgb(1.0, 0.0, 0.0));
                            } else {
                                assert_eq!(color, RenderColor::rgb(0.25, 0.93, 0.8));
                            }
                        }
                        let markers = map
                            .ordered_layers()
                            .into_iter()
                            .find(|layer| layer.z == 2)
                            .unwrap();
                        assert!(markers.primitives.iter().any(|p| matches!(p, RenderPrimitive::Polygon(polygon) if polygon.points.len() == 3 && polygon.fill.unwrap().color == RenderColor::rgb(1.0, 0.0, 0.0))));
                    }
                    let image = crate::raster::RasterRenderer::new().image_from_frames_with_layout(
                        &frames,
                        viewport,
                        layout,
                        crate::raster::RasterOptions::default(),
                    );
                    let pixels = image.to_rgb8().unwrap();
                    // Sample inside the actual flag cloth, above the spaceling.
                    let up = flag.normal;
                    let point = flag.position
                        + up * (0.45 + flag.raised_fraction * 2.7)
                        + engine_core::Vec2::new(up.y, -up.x) * 0.6;
                    let projected = frames[0].camera.world_to_viewport(
                        engine_common::RenderPoint::new(point.x, point.y),
                        panes[0].aspect_ratio(),
                    );
                    let x = (panes[0].x + projected.x * panes[0].width) as usize;
                    let y = (panes[0].y + projected.y * panes[0].height) as usize;
                    let pixel = pixels.as_slice()[y * pixels.width() as usize + x];
                    assert!(
                        pixel.r > 200 && pixel.g < 80 && pixel.b < 80,
                        "missing flag cloth: players={players} secured={secured} viewport={viewport:?} {pixel:?}"
                    );
                    if let Some(directory) = std::env::var_os("SPACEWARS_SORTIE_ARTIFACTS") {
                        let directory = std::path::PathBuf::from(directory);
                        std::fs::create_dir_all(&directory).unwrap();
                        let file = std::fs::File::create(directory.join(format!(
                            "planet-claim-p{players}-secured-{secured}-{}x{}.png",
                            viewport.width, viewport.height
                        )))
                        .unwrap();
                        let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
                        encoder.set_color(png::ColorType::Rgb);
                        encoder.set_depth(png::BitDepth::Eight);
                        encoder
                            .write_header()
                            .unwrap()
                            .write_image_data(pixels.as_bytes())
                            .unwrap();
                    }
                }
            }
        }
    }

    #[test]
    fn expedition_recovery_is_playable_with_the_minimal_pad_and_visible_in_both_backends() {
        use scenario_spacewars::ShipForm;
        use scenario_spacewars::surface_sortie::{LandingPhase, PilotLocation};
        let mut scenario = SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init_expedition(0, 2),
        };
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        let mut input = ClientInput::new(Rc::clone(&pads));
        let advance =
            |scenario: &mut SurfaceSortieClientScenario, input: &mut ClientInput, ticks| {
                for _ in 0..ticks {
                    let actions = scenario.map_input(input, false);
                    scenario.step(&actions, Duration::from_secs_f64(1.0 / 60.0));
                }
            };
        advance(&mut scenario, &mut input, 120);
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                south: true,
                east: true,
                dpad_down: true,
                ..Default::default()
            },
        );
        advance(&mut scenario, &mut input, 181);
        assert_eq!(
            scenario.state.observation(0).vehicle_form,
            ShipForm::EscapePod
        );
        assert!(scenario.state.observation(1).ship_available);
        pads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                ..Default::default()
            },
        );
        advance(&mut scenario, &mut input, 300);
        assert_eq!(
            scenario.state.observation(0).landing.phase,
            LandingPhase::Landed
        );
        for stage in ["pod", "rebuilding", "replacement"] {
            if stage == "rebuilding" {
                pads.borrow_mut().set_seat(
                    0,
                    GamepadSeatInput {
                        connected: true,
                        east: true,
                        ..Default::default()
                    },
                );
                advance(&mut scenario, &mut input, 1);
                pads.borrow_mut().set_seat(
                    0,
                    GamepadSeatInput {
                        connected: true,
                        ..Default::default()
                    },
                );
                advance(&mut scenario, &mut input, 400);
                assert_eq!(
                    scenario.state.observation(0).location,
                    PilotLocation::OnFoot
                );
                let progress = scenario
                    .state
                    .observation(0)
                    .recovery
                    .unwrap()
                    .rebuild_progress;
                assert!(progress > 0.1 && progress < 0.9, "{progress}");
            } else if stage == "replacement" {
                advance(&mut scenario, &mut input, 500);
                assert!(scenario.state.observation(0).ship_available);
                assert_eq!(scenario.state.observation(0).recovery.unwrap().rebuilds, 1);
            }
            for viewport in [
                Viewport::new(1280.0, 720.0),
                Viewport::new(800.0, 480.0),
                Viewport::new(800.0, 1280.0),
            ] {
                let frames = scenario.render_frames(RenderBackend::Raster, viewport);
                assert_eq!(
                    frames,
                    scenario.render_frames(RenderBackend::Vector, viewport)
                );
                let presentation = crate::render::scene_presentation_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                );
                assert_eq!(presentation.minimaps.len(), 2);
                let labels =
                    crate::render::raster_text_overlay(&frames, viewport, scenario.frame_layout());
                assert!(labels.iter().any(|p| p.text.starts_with(if stage == "pod" {
                    "P1  POD"
                } else {
                    "P1  ON FOOT"
                })));
                assert!(labels.iter().any(|p| p.text.starts_with("P2  ABOARD")));
                assert!(
                    !labels
                        .iter()
                        .any(|p| p.text.contains("Vehicle lost; restart"))
                );
                if stage == "rebuilding" {
                    assert!(labels.iter().any(|p| p.text.starts_with("Rebuild ")));
                }
                let image = crate::raster::RasterRenderer::new().image_from_frames_with_layout(
                    &frames,
                    viewport,
                    scenario.frame_layout(),
                    crate::raster::RasterOptions::default(),
                );
                let pixels = image.to_rgb8().unwrap();
                assert!(
                    pixels
                        .as_slice()
                        .iter()
                        .filter(|p| if stage == "pod" {
                            // The existing pod mesh has a blue cockpit/nose.
                            p.b > 200 && p.r < 80 && p.g < 80
                        } else {
                            p.r > 200 && p.g < 80 && p.b < 80
                        })
                        .count()
                        > 20,
                    "missing P1 actor: {stage} {viewport:?}"
                );
                if let Some(directory) = std::env::var_os("SPACEWARS_SORTIE_ARTIFACTS") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let file = std::fs::File::create(directory.join(format!(
                        "recovery-{stage}-{}x{}.png",
                        viewport.width, viewport.height
                    )))
                    .unwrap();
                    let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
                    encoder.set_color(png::ColorType::Rgb);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder
                        .write_header()
                        .unwrap()
                        .write_image_data(pixels.as_bytes())
                        .unwrap();
                }
            }
        }
    }

    #[test]
    fn registered_fixture_renders_and_restarts_in_both_backends() {
        for registration in [
            &REGISTRATION,
            &ORBIT_REGISTRATION,
            &WORLD_REGISTRATION,
            &EXPEDITION_REGISTRATION,
            &TERRAIN_REGISTRATION,
        ] {
            let viewport = Viewport::new(1280.0, 720.0);
            let mut scenario = registration
                .create(0, &Settings::default(), viewport, ScenarioStartMode::Normal)
                .unwrap();
            let initial = scenario.render_frames(RenderBackend::Vector, viewport);
            for tick in 0..90 {
                scenario.step(
                    &[SurfaceSortieAction {
                        interact_held: tick == 60,
                        ..SurfaceSortieAction::default()
                    }
                    .encode(PlayerId::PLAYER_1)],
                    Duration::from_secs_f64(1.0 / 60.0),
                );
                if tick == 59 {
                    check_render(&*scenario, viewport, "aboard");
                    check_render(&*scenario, Viewport::new(1280.0, 1400.0), "aboard-portrait");
                }
            }
            let frames = scenario.render_frames(RenderBackend::Vector, viewport);
            assert_eq!(
                frames,
                scenario.render_frames(RenderBackend::Raster, viewport)
            );
            assert_ne!(frames, initial);
            check_render(&*scenario, viewport, "on-foot");
            check_render(
                &*scenario,
                Viewport::new(1280.0, 1400.0),
                "on-foot-portrait",
            );
            assert!(matches!(
                scenario
                    .as_any()
                    .downcast_ref::<SurfaceSortieClientScenario>()
                    .unwrap()
                    .state
                    .location(0),
                scenario_spacewars::surface_sortie::PilotLocation::OnFoot
            ));
            let fresh = registration
                .create(0, &Settings::default(), viewport, ScenarioStartMode::Normal)
                .unwrap();
            assert_eq!(
                initial,
                fresh.render_frames(RenderBackend::Vector, viewport)
            );
        }
    }

    fn check_render(scenario: &dyn ClientScenario, viewport: Viewport, name: &str) {
        use crate::raster::{RasterOptions, RasterRenderer};
        let frames = scenario.render_frames(RenderBackend::Vector, viewport);
        let presentation = crate::render::scene_presentation_from_frames_with_layout(
            &frames,
            viewport,
            scenario.frame_layout(),
        );
        assert!(!presentation.main_primitives.is_empty());
        assert_eq!(presentation.minimaps.len(), 1);
        assert!(!presentation.minimaps[0].primitives.is_empty());
        let overlay =
            crate::render::raster_text_overlay(&frames, viewport, scenario.frame_layout());
        let label = if name.starts_with("on-foot") {
            "ON FOOT"
        } else {
            "ABOARD"
        };
        let label = if matches!(
            scenario.registration().id,
            "surface-expedition" | "spacewars-terrain"
        ) {
            format!("P1  {label}")
        } else {
            label.to_owned()
        };
        assert!(overlay.iter().any(|p| p.text.starts_with(&label)));
        let image = RasterRenderer::new().image_from_frames_with_layout(
            &frames,
            viewport,
            scenario.frame_layout(),
            RasterOptions::default(),
        );
        let pixels = image.to_rgb8().unwrap();
        // Preserve failing frames too: visibility assertions should be inspectable.
        if let Some(directory) = std::env::var_os("SPACEWARS_SORTIE_ARTIFACTS") {
            let directory = std::path::PathBuf::from(directory);
            let directory = if directory.is_absolute() {
                directory
            } else {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(directory)
            };
            std::fs::create_dir_all(&directory).unwrap();
            let file = std::fs::File::create(
                directory.join(format!("{name}-{}.png", scenario.registration().id)),
            )
            .unwrap();
            let mut encoder = png::Encoder::new(file, pixels.width(), pixels.height());
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(pixels.as_bytes())
                .unwrap();
        }
        let ship_pixels = pixels
            .as_slice()
            .iter()
            .filter(|p| p.r > 200 && p.g < 80 && p.b < 80)
            .count();
        assert!(ship_pixels > 100, "missing ship in {name}: {ship_pixels}");
        if name.starts_with("on-foot") {
            let position = scenario
                .as_any()
                .downcast_ref::<SurfaceSortieClientScenario>()
                .unwrap()
                .state
                .observation(0)
                .position;
            let projected = frames[0].camera.world_to_viewport(
                engine_common::RenderPoint::new(position.x, position.y),
                viewport.aspect_ratio(),
            );
            let center_x = projected.x * pixels.width() as f32;
            let center_y = projected.y * pixels.height() as f32;
            let suit_pixels = pixels
                .as_slice()
                .iter()
                .enumerate()
                .filter(|(i, p)| {
                    let x = i % pixels.width() as usize;
                    let y = i / pixels.width() as usize;
                    (x as f32 - center_x).abs() < 60.0
                        && (y as f32 - center_y).abs() < 60.0
                        && p.r > 200
                        && p.g > 80
                        && p.g < 190
                        && p.b < 100
                })
                .count();
            assert!(suit_pixels > 20, "missing on-foot actor: {suit_pixels}");
        }
    }
}
