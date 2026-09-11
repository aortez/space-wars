use std::time::Duration;

use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_spacewars::PlayerId;
use scenario_spacewars::surface_sortie::{
    SurfaceMiningAction, SurfaceMotionPreset, SurfaceSortieAction, SurfaceSortieScenario,
    SurfaceSortieState, SurfaceWingAction,
    combat::SurfaceWeaponAction,
    impact::{ImpactKind, SurfaceImpactAction},
};
use spacewars_ai::{
    BrainReset, combat_pilot::RulePilotV4, flight_pilot::RulePilotV2,
    jetpack_crossing::JetpackCrossingPilot, recovery_pilot::RulePilotV3,
    tactical_capture::TacticalCapturePilot,
};

mod mission;
pub(super) use mission::{
    ARENA_DUEL_REGISTRATION, ARENA_REGISTRATION, MATCH_REGISTRATION, TRAVEL_DUEL_REGISTRATION,
    TRAVEL_REGISTRATION,
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
    controls_help: "Destructible Expedition: land on both rear feet, exit, and stand still 3s to raise your planet flag. A/Space thrusts or jumps; B/X exits or boards. Left/right turns or walks; Down/S brakes. Hold RB/J to sweep wings and cruise; release to open, then brake for landing. Open wings turn faster; sweeping adds speed. P2 uses PageDown for wings aboard. Right stick aims the mining beam; RT or LB mines; Y changes cut size (one cell, radius 1, radius 3). Keyboard E mines, T changes size; arrows aim. P2 uses numpad 4/6, 8, 5, 2 for move, thrust/jump, brake, transfer; End mines, PageDown changes size. A missing flag footing neutralizes the planet. Land an escape pod and stand on owned ground 8s to rebuild. Gamepad X calls a light asteroid; RB+X calls a heavy one (keyboard K / J+K; P2 Home / PageDown+Home). One rock per press, 3s cooldown. Hold A+B+Down 3s for the loss drill. Select 1 or 2 players in Settings. Start/Esc pauses. No landing pad or repair terminal.",
    create: create_material,
    ..EXPEDITION_REGISTRATION
};

pub(super) const PILOT_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-ai",
    controls_help: "Material pilot AI playtest: P1 is human; P2 takes off, flies a fast swept-wing circuit, opens and brakes, returns, lands, exits, claims a neutral planet, boards and departs using the same controls and physics. Watch its goal in the P2 view. After one sortie it holds above the planet. Enemy flag navigation and mining are outside this flight demo; a blocked goal is shown explicitly. Select spacewars-terrain-recovery for the loss and rebuilding demo. P1 controls match Destructible Expedition: A/Space thrusts or jumps; B/X transfers; left/right turns or walks; Down/S brakes; hold RB/J for swept-wing cruise, release to open; right stick aims, RT/LB mines, Y changes size. Start/Esc pauses; R restarts both pilots. Select spacewars-terrain for one or two human pilots.",
    create: create_pilot,
    ..TERRAIN_REGISTRATION
};

/// Host-owned policy. The scenario still consumes only ordinary encoded actions.
struct MaterialPilotClientScenario {
    sortie: SurfaceSortieClientScenario,
    brain: RulePilotV2,
}

pub(super) const RECOVERY_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-recovery",
    controls_help: "Recovery AI playtest: P1 is human; P2 flies a swept-wing circuit, lands, claims and departs. A heavy asteroid then strikes its ship. The bot steadies and lands its escape pod, exits, rebuilds on owned ground, boards and flies again. Watch its goal in the P2 view. Only one automatic strike is scheduled; restart to repeat. Human controls match Destructible Expedition. X on the gamepad (K on keyboard) calls a light asteroid toward your assigned ship; hold RB with X (J+K) for a heavy strike. P2 human keyboard uses Home and PageDown+Home. Each press calls one rock, with a 3s cooldown. These are controlled collision drills. Pods and spacelings remain invulnerable. Enemy flag routes and rescue from arbitrary caverns remain outside this task. Start/Esc pauses; R restarts.",
    create: create_recovery_pilot,
    ..TERRAIN_REGISTRATION
};

struct MaterialRecoveryClientScenario {
    sortie: SurfaceSortieClientScenario,
    brain: RulePilotV3,
    strike_sent: bool,
}

pub(super) const JETPACK_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-jetpack",
    controls_help: "Jetpack crossing trial: P2 exits, flies over its parked ship, lands and claims, recharges, then crosses back and boards. Both pilots have the same jetpack. Tap A/Space to jump or get up; hold while airborne for lift. Left/right steers. Charge refills while standing still with jump released. B/X transfers at a settled ship's hatch. Watch the jet flames, charge and P2 goal. P1 is human; mining and flight controls match Destructible Expedition. Start/Esc pauses; restart repeats the trial.",
    create: create_jetpack_pilot,
    ..TERRAIN_REGISTRATION
};

struct MaterialJetpackClientScenario {
    sortie: SurfaceSortieClientScenario,
    brain: JetpackCrossingPilot,
}

fn create_jetpack_pilot(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(MaterialJetpackClientScenario {
        sortie: SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init_material_jetpack(seed, 2),
        },
        brain: JetpackCrossingPilot::new(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: seed,
        }),
    }))
}

impl ClientScenario for MaterialJetpackClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &JETPACK_REGISTRATION
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
            .jetpack_crossing_observation(1, self.brain.direction());
        let mut actions = human_pilot_actions(actions);
        actions.push(self.brain.step(&observation).encode(PlayerId::PLAYER_2));
        self.sortie.step(&actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, benchmark: bool) -> Vec<Action> {
        human_pilot_actions(&self.sortie.map_input(input, benchmark))
    }
    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let mut frames = self.sortie.render_frames(renderer, viewport);
        pilot_hud(&mut frames, self.brain.label());
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

fn create_recovery_pilot(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(MaterialRecoveryClientScenario {
        sortie: SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init_material(seed, 2),
        },
        brain: RulePilotV3::new(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: seed,
        }),
        strike_sent: false,
    }))
}

fn create_pilot(
    seed: u64,
    _settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(MaterialPilotClientScenario {
        sortie: SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init_material(seed, 2),
        },
        brain: RulePilotV2::new(BrainReset {
            actor: PlayerId::PLAYER_2,
            episode_seed: seed,
        }),
    }))
}

pub(super) const COMBAT_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-combat",
    controls_help: "Material combat: P1 human versus P2 combat bot. Set Bot mission to Capture in launcher Settings to intercept P2 as it seeks shelter, lands, captures, boards and departs. After its attempt it fights or recovers. A/Space thrusts; left/right or A/D turns; Down/S brakes; hold RB/J to sweep wings and cruise. RT or LB fires the forward laser; X (west face) launches a missile. Keyboard E laser, K missiles. Two visible rounds share an energy supply: each automatic reload costs 25% and takes 2s, one round at a time. Energy regenerates at 10%/s; laser draws 12%/s and resumes at 10% after depletion. Loaded rounds can fire with an empty battery. Recoil is tuned for this flight scale; shells excavate terrain and solid ground blocks laser shots. Land rear-first, B/X to exit or board. On foot, right stick aims, RT/LB mines, Y changes cut size; E/T on keyboard. Stand still 3s to claim neutral ground. Ship loss leaves a pod; land, exit and stand on owned ground 8s to rebuild, then board normally. Destroying the flag footing neutralizes ownership. Launcher Settings adjust Bot combat breaks (Off, 8s, 15s or 30s of combat on average) and break duration. During a flyby the bot keeps moving with weapons off and remains vulnerable. The bot pursues full occupied ships, routes around the planet and uses the recovery task after losing its ship. Pods and spacelings remain invulnerable. On foot, tap A/Space to jump or get up; hold in the air for jetpack lift and use left/right to steer. Recharge by standing still with jump released. Both pilots have the same jetpack. Capture and recovery bots choose measured walking, jumping or flight over their parked ship to reach flags and hatches. Blocked hatches, large gaps and caves can still stop a route. Launcher Settings can enable random asteroids and adjust their arrival interval and strength. In a tipped, grounded pod, hold brake plus thrust for a short recovery lift, then turn upright. Release before another lift. Start/Esc pauses; R restarts.",
    create: create_combat_pilot,
    ..TERRAIN_REGISTRATION
};
struct MaterialCombatClientScenario {
    sortie: SurfaceSortieClientScenario,
    brain: RulePilotV4,
    p1_brain: Option<RulePilotV4>,
    tactical: Option<TacticalCapturePilot>,
}
fn create_combat_pilot(
    seed: u64,
    settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(MaterialCombatClientScenario {
        sortie: SurfaceSortieClientScenario {
            state: material_combat_state(seed, settings),
        },
        brain: RulePilotV4::with_combat_breaks(
            BrainReset {
                actor: PlayerId::PLAYER_2,
                episode_seed: seed,
            },
            settings.combat_breaks,
        ),
        p1_brain: None,
        tactical: capture_pilot(seed, PlayerId::PLAYER_2, settings),
    }))
}

pub(super) const DUEL_REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "spacewars-terrain-duel",
    controls_help: "Watch two material combat bots use ordinary controls, weapons and recovery. Damage, pod ejection, landing, claims and rebuilding are physical gameplay; no hits or ownership are scripted. Each view shows its bot's current task. Set Bot mission to Capture in launcher Settings for P1 to attempt a sheltered landing, capture and departure while P2 intercepts. After its attempt P1 fights or recovers. Launcher Settings adjust combat breaks and environmental asteroid arrivals and strength. A tipped pod can use brake plus thrust for a brief recovery lift. Start/Esc pauses; R restarts. Select spacewars-terrain-combat to fly P1 against the bot. A three-minute run may end during another recovery; bots can approach flags and hatches over measured ground or jetpack over their parked ship; blocked hatches, large gaps and caves can still stop a route.",
    create: create_combat_duel,
    ..COMBAT_REGISTRATION
};
fn create_combat_duel(
    seed: u64,
    settings: &Settings,
    _viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    Ok(Box::new(MaterialCombatClientScenario {
        sortie: SurfaceSortieClientScenario {
            state: material_combat_state(seed, settings),
        },
        brain: RulePilotV4::with_combat_breaks(
            BrainReset {
                actor: PlayerId::PLAYER_2,
                episode_seed: seed,
            },
            settings.combat_breaks,
        ),
        p1_brain: Some(RulePilotV4::with_combat_breaks(
            BrainReset {
                actor: PlayerId::PLAYER_1,
                episode_seed: seed,
            },
            settings.combat_breaks,
        )),
        tactical: capture_pilot(seed, PlayerId::PLAYER_1, settings),
    }))
}

fn material_combat_state(seed: u64, settings: &Settings) -> SurfaceSortieState {
    let mut state = SurfaceSortieScenario::init_material_combat(seed);
    state.set_asteroid_pressure(settings.material_combat.asteroids);
    state
}

fn capture_pilot(seed: u64, actor: PlayerId, settings: &Settings) -> Option<TacticalCapturePilot> {
    (settings.material_combat.mission == engine_common::MaterialCombatMission::Capture).then(|| {
        TacticalCapturePilot::new(
            BrainReset {
                actor,
                episode_seed: seed,
            },
            settings.combat_breaks,
        )
    })
}

fn human_pilot_actions(actions: &[Action]) -> Vec<Action> {
    human_seat_actions(actions, [false, true])
}

fn human_seat_actions(actions: &[Action], bots: [bool; 2]) -> Vec<Action> {
    actions
        .iter()
        .filter(|action| {
            SurfaceWeaponAction::decode(action).is_some_and(|(owner, _)| !bots[owner.index()])
                || SurfaceSortieAction::decode(action)
                    .is_some_and(|(owner, _)| !bots[owner.index()])
                || SurfaceWingAction::decode(action).is_some_and(|(owner, _)| !bots[owner.index()])
                || SurfaceMiningAction::decode(action).is_some_and(|(seat, _)| !bots[seat])
                || SurfaceImpactAction::decode(action)
                    .is_some_and(|(owner, _)| !bots[owner.index()])
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
            .flight_pilot_observation(1, self.brain.site_request());
        let mut actions = human_pilot_actions(actions);
        actions.extend(self.brain.intent(&observation).encode(PlayerId::PLAYER_2));
        self.sortie.step(&actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, benchmark: bool) -> Vec<Action> {
        human_pilot_actions(&self.sortie.map_input(input, benchmark))
    }
    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let mut frames = self.sortie.render_frames(renderer, viewport);
        pilot_hud(&mut frames, self.brain.label());
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

impl ClientScenario for MaterialCombatClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        if self.p1_brain.is_some() {
            &DUEL_REGISTRATION
        } else {
            &COMBAT_REGISTRATION
        }
    }
    fn tick_model(&self) -> TickModel {
        self.sortie.tick_model()
    }
    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        if dt.is_zero() {
            return self.sortie.step(&[], dt);
        }
        if let Some(tactical) = &mut self.tactical {
            let (seat, owner) = if self.p1_brain.is_some() {
                (0, PlayerId::PLAYER_1)
            } else {
                (1, PlayerId::PLAYER_2)
            };
            let observation = self
                .sortie
                .state
                .tactical_sortie_observation(seat, tactical.site_request());
            let mut actions = if seat == 1 {
                human_pilot_actions(actions)
            } else {
                Vec::new()
            };
            actions.extend(tactical.intent(&observation).encode(owner));
            if seat == 0 {
                let opponent = self
                    .sortie
                    .state
                    .combat_observation(1, self.brain.site_request());
                actions.extend(self.brain.intent(&opponent).encode(PlayerId::PLAYER_2));
            }
            return self.sortie.step(&actions, dt);
        }
        let observation = self
            .sortie
            .state
            .combat_observation(1, self.brain.site_request());
        let mut actions = if let Some(brain) = &mut self.p1_brain {
            let o = self
                .sortie
                .state
                .combat_observation(0, brain.site_request());
            brain.intent(&o).encode(PlayerId::PLAYER_1).to_vec()
        } else {
            human_pilot_actions(actions)
        };
        actions.extend(self.brain.intent(&observation).encode(PlayerId::PLAYER_2));
        self.sortie.step(&actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, benchmark: bool) -> Vec<Action> {
        if self.p1_brain.is_some() {
            return Vec::new();
        }
        human_pilot_actions(&self.sortie.map_input(input, benchmark))
    }
    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let mut frames = self.sortie.render_frames(renderer, viewport);
        if let Some(tactical) = &self.tactical {
            let seat = if self.p1_brain.is_some() { 0 } else { 1 };
            pilot_hud_for(&mut frames, seat, tactical.label());
            if seat == 0 {
                pilot_hud(&mut frames, self.brain.label());
            }
        } else {
            pilot_hud(&mut frames, self.brain.label());
            if let Some(brain) = &self.p1_brain {
                pilot_hud_for(&mut frames, 0, brain.label());
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

fn pilot_hud(frames: &mut [RenderFrame], label: &str) {
    pilot_hud_for(frames, 1, label);
}
fn pilot_hud_for(frames: &mut [RenderFrame], player: usize, label: &str) {
    use engine_common::{RenderColor, RenderPrimitive};
    // Reuse the control hint so the physical viewport stays clear at 800x480.
    for layer in &mut frames[player].layers {
        for primitive in &mut layer.primitives {
            if let RenderPrimitive::Text(text) = primitive
                && text.text.starts_with("A: thrust")
            {
                text.text = format!("AI: {label}");
                text.color = RenderColor::rgb(1.0, 0.82, 0.25);
            }
        }
    }
}

impl ClientScenario for MaterialRecoveryClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &RECOVERY_REGISTRATION
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
            .recovery_task_observation(1, self.brain.site_request());
        let mut actions = human_pilot_actions(actions);
        actions.extend(self.brain.intent(&observation).encode(PlayerId::PLAYER_2));
        // The demonstration schedules the hazard. The policy sees the same
        // damage/recovery observations as any future match bot would receive.
        let strike = !self.strike_sent && self.brain.telemetry().flight.completed_tick.is_some();
        self.strike_sent |= strike;
        actions.push(
            SurfaceImpactAction {
                held: strike,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_2),
        );
        self.sortie.step(&actions, dt)
    }
    fn map_input(&self, input: &mut ClientInput, benchmark: bool) -> Vec<Action> {
        human_pilot_actions(&self.sortie.map_input(input, benchmark))
    }
    fn render_frames(&self, renderer: RenderBackend, viewport: Viewport) -> Vec<RenderFrame> {
        let mut frames = self.sortie.render_frames(renderer, viewport);
        pilot_hud(&mut frames, self.brain.label());
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
                if self.state.combat_enabled() {
                    let (_, laser, _) = input.surface_mining_input(player);
                    actions.push(
                        SurfaceWeaponAction {
                            laser,
                            cannon: input.surface_impact_held(player),
                        }
                        .encode(PlayerId::from_index(player).expect("bounded seat")),
                    );
                } else {
                    actions.push(
                        SurfaceImpactAction {
                            held: input.surface_impact_held(player),
                            kind: if input.surface_wings_held(player) {
                                ImpactKind::Heavy
                            } else {
                                ImpactKind::Light
                            },
                            oblique: false,
                        }
                        .encode(PlayerId::from_index(player).expect("bounded seat")),
                    );
                }
                actions.push(
                    SurfaceWingAction {
                        closed: input.surface_wings_held(player),
                    }
                    .encode(PlayerId::from_index(player).expect("bounded seat")),
                );
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
    fn capture_mission_owns_the_expected_seat_and_pause_restart_preserve_boundaries() {
        for registration in [&COMBAT_REGISTRATION, &DUEL_REGISTRATION] {
            let make = || {
                registration
                    .create(
                        42,
                        &Settings {
                            material_combat: engine_common::MaterialCombatSettings {
                                mission: engine_common::MaterialCombatMission::Capture,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        Viewport::new(800.0, 480.0),
                        ScenarioStartMode::Normal,
                    )
                    .unwrap()
            };
            let mut host = make();
            let host = host
                .as_any_mut()
                .downcast_mut::<MaterialCombatClientScenario>()
                .unwrap();
            let seat = if registration.id == DUEL_REGISTRATION.id {
                0
            } else {
                1
            };
            let initial = host.tactical.as_ref().unwrap().telemetry().clone();
            let input = [SurfaceWeaponAction {
                laser: true,
                cannon: true,
            }
            .encode(PlayerId::from_index(seat).unwrap())];
            host.step(&input, Duration::ZERO);
            assert_eq!(host.tactical.as_ref().unwrap().telemetry(), &initial);
            assert_eq!(host.sortie.state.observation(0).tick, 0);
            for _ in 0..120 {
                host.step(&input, Duration::from_nanos(16_666_667));
            }
            assert!(
                host.tactical
                    .as_ref()
                    .unwrap()
                    .telemetry()
                    .started_tick
                    .is_some()
            );
            assert_eq!(host.sortie.state.combat_telemetry(seat).shells_fired, 0);
            assert_eq!(host.sortie.state.combat_telemetry(seat).laser_hit_ticks, 0);
            let frames = host.render_frames(RenderBackend::Raster, Viewport::new(800.0, 480.0));
            let expected = format!("AI: {}", host.tactical.as_ref().unwrap().label());
            assert!(frames[seat].layers.iter().flat_map(|l| &l.primitives).any(
                |p| matches!(p, engine_common::RenderPrimitive::Text(t) if t.text == expected)
            ));
            let reset = make();
            let reset = reset
                .as_any()
                .downcast_ref::<MaterialCombatClientScenario>()
                .unwrap();
            assert_eq!(reset.tactical.as_ref().unwrap().telemetry(), &initial);
        }
    }

    #[test]
    fn combat_and_duel_hosts_apply_break_settings_to_every_bot() {
        for registration in [&COMBAT_REGISTRATION, &DUEL_REGISTRATION] {
            for interval in [0, 8, 15, 30] {
                let settings = Settings {
                    combat_breaks: engine_common::CombatBreakSettings {
                        interval_seconds: interval,
                        duration_seconds: 6,
                    },
                    ..Default::default()
                };
                let mut host = registration
                    .create(
                        42,
                        &settings,
                        Viewport::new(800.0, 480.0),
                        ScenarioStartMode::Normal,
                    )
                    .unwrap();
                let host = host
                    .as_any_mut()
                    .downcast_mut::<MaterialCombatClientScenario>()
                    .unwrap();
                assert_eq!(host.brain.telemetry().breaks.config, settings.combat_breaks);
                if let Some(p1) = &host.p1_brain {
                    assert_eq!(p1.telemetry().breaks.config, settings.combat_breaks);
                }
            }
        }
    }

    #[test]
    fn combat_host_maps_weapons_owns_p2_and_pauses_without_advancing_policy() {
        let mut host = COMBAT_REGISTRATION
            .create(
                42,
                &Settings::default(),
                Viewport::new(800.0, 480.0),
                ScenarioStartMode::Normal,
            )
            .unwrap();
        let host = host
            .as_any_mut()
            .downcast_mut::<MaterialCombatClientScenario>()
            .unwrap();
        let mut input = ClientInput::default();
        input.press(GameKey::TerrainDrill);
        input.press(GameKey::P1Cannon);
        let actions = host.map_input(&mut input, false);
        assert_eq!(
            actions
                .iter()
                .filter_map(SurfaceWeaponAction::decode)
                .collect::<Vec<_>>(),
            vec![(
                PlayerId::PLAYER_1,
                SurfaceWeaponAction {
                    laser: true,
                    cannon: true
                }
            )]
        );
        assert!(
            !actions
                .iter()
                .any(|a| SurfaceImpactAction::decode(a).is_some())
        );
        let interference = SurfaceWeaponAction {
            laser: true,
            cannon: true,
        }
        .encode(PlayerId::PLAYER_2);
        assert!(human_pilot_actions(&[interference]).is_empty());
        let before = host.brain.telemetry().clone();
        host.step(&actions, Duration::ZERO);
        assert_eq!(host.brain.telemetry(), &before);
        assert_eq!(host.sortie.state.observation(0).tick, 0);
        host.step(&actions, Duration::from_nanos(16_666_667));
        assert_eq!(host.sortie.state.observation(0).tick, 1);
        let frames = host.render_frames(RenderBackend::Vector, Viewport::new(800.0, 480.0));
        assert_eq!(frames.len(), 4);
    }

    #[test]
    fn recovery_host_owns_p2_actions_and_resets_the_single_strike_driver() {
        let make = || {
            RECOVERY_REGISTRATION
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
            .downcast_mut::<MaterialRecoveryClientScenario>()
            .unwrap();
        let initial = host.brain.telemetry().clone();
        let interference = [
            SurfaceImpactAction {
                held: true,
                ..Default::default()
            }
            .encode(PlayerId::PLAYER_2),
            SurfaceSortieAction {
                primary_held: true,
                horizontal: 1.0,
                interact_held: true,
                brake_held: true,
            }
            .encode(PlayerId::PLAYER_2),
        ];
        host.step(&interference, Duration::ZERO);
        assert_eq!(host.brain.telemetry(), &initial);
        assert!(!host.strike_sent);
        for _ in 0..180 * 60 {
            host.step(&interference, Duration::from_nanos(16_666_667));
            if host.brain.telemetry().departed_tick.is_some() {
                break;
            }
        }
        assert!(
            host.brain.telemetry().departed_tick.is_some(),
            "{:?}",
            host.brain.telemetry()
        );
        assert_eq!(host.sortie.state.damage_observation(1).strikes, 1);
        assert_eq!(host.sortie.state.damage_observation(0).strikes, 0);
        let before = host.brain.telemetry().clone();
        host.step(&[], Duration::ZERO);
        assert_eq!(host.brain.telemetry(), &before);
        let frames = host.render_frames(RenderBackend::Raster, Viewport::new(800.0, 480.0));
        assert!(frames[1].layers.iter().flat_map(|l| &l.primitives).any(|p|
            matches!(p, engine_common::RenderPrimitive::Text(t) if t.text == "AI: recovered / flying again")));
        let fresh = make();
        let fresh = fresh
            .as_any()
            .downcast_ref::<MaterialRecoveryClientScenario>()
            .unwrap();
        assert_eq!(fresh.brain.telemetry(), &initial);
        assert!(!fresh.strike_sent);
    }

    #[test]
    fn impact_keys_and_gamepads_are_seat_local_and_release_on_disconnect() {
        let pads = Rc::new(RefCell::new(GamepadInput::default()));
        let mut input = ClientInput::new(Rc::clone(&pads));
        let host = SurfaceSortieClientScenario {
            state: SurfaceSortieScenario::init_material(42, 2),
        };
        input.press(GameKey::P1Cannon);
        input.press(GameKey::P1Wing);
        input.press(GameKey::P2ZoomOut);
        let impacts = |actions: Vec<Action>| {
            actions
                .iter()
                .filter_map(SurfaceImpactAction::decode)
                .collect::<Vec<_>>()
        };
        let actions = impacts(host.map_input(&mut input, false));
        assert_eq!(
            (actions[0].0, actions[0].1.held, actions[0].1.kind),
            (PlayerId::PLAYER_1, true, ImpactKind::Heavy)
        );
        assert_eq!(
            (actions[1].0, actions[1].1.held, actions[1].1.kind),
            (PlayerId::PLAYER_2, true, ImpactKind::Light)
        );
        input.release(GameKey::P1Cannon);
        input.release(GameKey::P1Wing);
        input.release(GameKey::P2ZoomOut);
        pads.borrow_mut().set_seat(
            1,
            GamepadSeatInput {
                connected: true,
                west: true,
                right_bumper: true,
                ..Default::default()
            },
        );
        let actions = impacts(host.map_input(&mut input, false));
        assert!(!actions[0].1.held);
        assert!(actions[1].1.held);
        assert_eq!(actions[1].1.kind, ImpactKind::Heavy);
        pads.borrow_mut().disconnect_seat(1);
        assert!(
            impacts(host.map_input(&mut input, false))
                .iter()
                .all(|(_, action)| !action.held)
        );
    }

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
            SurfaceWingAction { closed: true }.encode(PlayerId::PLAYER_2),
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
        for _ in 0..180 * 60 {
            let o = reference.flight_pilot_observation(1, brain.site_request());
            let action = brain.intent(&o);
            SurfaceSortieScenario::step(
                &mut reference,
                &action.encode(PlayerId::PLAYER_2),
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
                right_bumper: true,
                north: true,
                ..Default::default()
            },
        );
        let actions = scenario.map_input(&mut input, false);
        let mining: Vec<_> = actions
            .iter()
            .filter_map(SurfaceMiningAction::decode)
            .collect();
        let wings: Vec<_> = actions
            .iter()
            .filter_map(SurfaceWingAction::decode)
            .collect();
        assert_eq!(
            wings,
            vec![
                (PlayerId::PLAYER_1, SurfaceWingAction::default()),
                (PlayerId::PLAYER_2, SurfaceWingAction { closed: true })
            ]
        );
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
                .filter_map(SurfaceWingAction::decode)
                .all(|(_, a)| !a.closed)
        );
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
