//! Client-side keyboard state and original Spacewars control mapping.

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::rc::Rc;

use engine_common::{Action, PointerPhase, RenderPoint};
use engine_core::Vec2;
use engine_nes::ControllerButtons;
use scenario_spacewars::{
    BodyId, PlanetId, PlayerId, ShipForm, ShipIntent, ShipIntentEncoder, ShipObservationV1,
    ShipSensorProfile, SpacewarsAction, SpacewarsScenario, SpacewarsState,
};
use slint::ComponentHandle;
use slint::winit_030::winit::event::{ElementState, WindowEvent};
use slint::winit_030::winit::keyboard::{KeyCode, PhysicalKey};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use spacewars_ai::{AvoidanceBody, BrainReset, BrainTelemetry, DEFAULT_BUILT_IN_POLICY, ShipBrain};

use crate::MainWindow;

pub(crate) type SharedInput = Rc<RefCell<ClientInput>>;
pub(crate) type SharedGamepadInput = Rc<RefCell<GamepadInput>>;
type SharedKeyboardState = Rc<RefCell<BTreeSet<GameKey>>>;

// Ten hertz is frequent enough to explain guidance and collision behavior
// without turning diagnostics into another per-frame workload. Event
// transitions are sampled immediately between these periodic samples.
const FLIGHT_SAMPLE_RATE_HZ: u32 = 10;
const FLIGHT_RECENT_CAPACITY: usize = 180;
const FLIGHT_ENCOUNTER_PRE_SAMPLES: usize = 60;
const FLIGHT_ENCOUNTER_CAPACITY: usize = 180;
// Rapier contact manifolds can disappear for a frame while two bodies remain
// in the same physical scrape. Keep one incident alive until contact has been
// absent for half a second at the nominal 60 Hz simulation rate.
const FLIGHT_CONTACT_REARM_TICKS: u64 = 30;
const BODY_PREDICTION_HORIZON_SECONDS: f32 = 5.0;

pub(crate) fn new_shared_input() -> (SharedInput, SharedGamepadInput) {
    let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
    let input = Rc::new(RefCell::new(ClientInput::new(Rc::clone(&gamepads))));
    (input, gamepads)
}

pub(crate) struct ClientInput {
    pressed: SharedKeyboardState,
    gamepads: SharedGamepadInput,
    spacewars_controls: SpacewarsControls,
    pointer_events: Vec<ScreenPointerEvent>,
    active_pointer: Option<RenderPoint>,
}

impl Default for ClientInput {
    fn default() -> Self {
        Self::new(Rc::new(RefCell::new(GamepadInput::default())))
    }
}

impl ClientInput {
    pub(crate) fn new(gamepads: SharedGamepadInput) -> Self {
        let pressed = Rc::new(RefCell::new(BTreeSet::new()));
        Self {
            spacewars_controls: SpacewarsControls::new(Rc::clone(&pressed), Rc::clone(&gamepads)),
            pressed,
            gamepads,
            pointer_events: Vec::new(),
            active_pointer: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct GamepadSeatInput {
    pub connected: bool,
    pub name: String,
    pub left_stick_x: f32,
    pub left_stick_y: f32,
    pub left_trigger: f32,
    pub right_trigger: f32,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub right_bumper: bool,
    pub south: bool,
    pub east: bool,
    pub west: bool,
    pub start: bool,
    pub select: bool,
}

#[derive(Debug, Default)]
pub(crate) struct GamepadInput {
    seats: [GamepadSeatInput; 2],
}

impl GamepadInput {
    pub(crate) fn seat(&self, player: usize) -> Option<&GamepadSeatInput> {
        self.seats.get(player)
    }

    pub(crate) fn set_seat(&mut self, player: usize, state: GamepadSeatInput) {
        if let Some(seat) = self.seats.get_mut(player) {
            *seat = state;
        }
    }

    pub(crate) fn disconnect_seat(&mut self, player: usize) {
        let Some(seat) = self.seats.get_mut(player) else {
            return;
        };
        let name = std::mem::take(&mut seat.name);
        *seat = GamepadSeatInput {
            name,
            ..GamepadSeatInput::default()
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScreenPointerEvent {
    pub position: RenderPoint,
    pub phase: PointerPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GameKey {
    Reset,
    Pause,
    ForcePause,
    Benchmark,
    Back,
    Controls,
    ReturnLauncher,
    NesUp,
    NesDown,
    NesLeft,
    NesRight,
    NesA,
    NesSelect,
    NesStart,
    P1Wing,
    P1Thrust,
    P1Brake,
    P1Reverse,
    P1TurnLeft,
    P1TurnRight,
    P1Laser,
    P1Cannon,
    P1ZoomIn,
    P1ZoomOut,
    P2Wing,
    P2Thrust,
    P2Brake,
    P2Reverse,
    P2TurnLeft,
    P2TurnRight,
    P2Laser,
    P2Cannon,
    P2ZoomIn,
    P2ZoomOut,
}

pub(crate) fn install_window_input(window: &MainWindow, input: SharedInput) {
    // `has_winit_window()` stays false until the native window is created during
    // `run()`. The event filter only needs the winit adapter, so install it
    // before the event loop starts.
    let keyboard_input = Rc::clone(&input);
    window.window().on_winit_window_event(move |_, event| {
        if matches!(event, WindowEvent::Focused(false)) {
            keyboard_input.borrow_mut().handle_focus_loss();
            return EventResult::Propagate;
        }

        let Some((key, state)) = mapped_key_event(event) else {
            return EventResult::Propagate;
        };

        match state {
            ElementState::Pressed => keyboard_input.borrow_mut().press(key),
            ElementState::Released => keyboard_input.borrow_mut().release(key),
        }

        EventResult::Propagate
    });

    window.on_scenario_pointer(move |x, y, phase| {
        let phase = match phase {
            0 => PointerPhase::Press,
            1 => PointerPhase::Drag,
            2 => PointerPhase::Release,
            3 => PointerPhase::Cancel,
            _ => return,
        };
        input.borrow_mut().push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(x, y),
            phase,
        });
    });
}

impl ClientInput {
    pub(crate) fn is_pressed(&self, key: GameKey) -> bool {
        self.pressed.borrow().contains(&key)
    }

    pub(crate) fn take_reset_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Reset)
    }

    pub(crate) fn take_pause_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Pause)
    }

    pub(crate) fn take_force_pause_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::ForcePause)
    }

    pub(crate) fn take_benchmark_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Benchmark)
    }

    pub(crate) fn take_back_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Back)
    }

    pub(crate) fn take_controls_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::Controls)
    }

    pub(crate) fn take_return_launcher_requested(&mut self) -> bool {
        self.pressed.borrow_mut().remove(&GameKey::ReturnLauncher)
    }

    pub(crate) fn actions_for_spacewars(
        &mut self,
        state: &SpacewarsState,
        benchmark_active: bool,
        bot_players: [bool; 2],
    ) -> Vec<Action> {
        let mut actions = self
            .spacewars_controls
            .actions(state, benchmark_active, bot_players);
        let pressed = self.pressed.borrow();
        for controls in [P1_CONTROLS, P2_CONTROLS] {
            if pressed.contains(&controls.zoom_in) {
                actions.push(SpacewarsAction::zoom_in(controls.player));
            } else if pressed.contains(&controls.zoom_out) {
                actions.push(SpacewarsAction::zoom_out(controls.player));
            }
        }
        actions
    }

    pub(crate) fn rover_gamepad_input(&self) -> (f32, bool, bool) {
        let gamepads = self.gamepads.borrow();
        let Some(gamepad) = gamepads.seat(0).filter(|gamepad| gamepad.connected) else {
            return (0.0, false, false);
        };
        let brake = gamepad.east || gamepad.dpad_down;
        let throttle = match (gamepad.dpad_left, gamepad.dpad_right) {
            (false, true) if !brake => 1.0,
            (true, false) if !brake => -1.0,
            _ => 0.0,
        };
        (throttle, brake, gamepad.south)
    }

    pub(crate) fn nes_controller_buttons(&self, player: usize) -> ControllerButtons {
        let mut buttons = ControllerButtons::NONE;
        let pressed = self.pressed.borrow();
        if player == 0 {
            if pressed.contains(&GameKey::NesUp) {
                buttons |= ControllerButtons::UP;
            }
            if pressed.contains(&GameKey::NesDown) {
                buttons |= ControllerButtons::DOWN;
            }
            if pressed.contains(&GameKey::NesLeft) {
                buttons |= ControllerButtons::LEFT;
            }
            if pressed.contains(&GameKey::NesRight) {
                buttons |= ControllerButtons::RIGHT;
            }
            if pressed.contains(&GameKey::NesA) || pressed.contains(&GameKey::P1Laser) {
                buttons |= ControllerButtons::A;
            }
            if pressed.contains(&GameKey::P1Reverse) {
                buttons |= ControllerButtons::B;
            }
            if pressed.contains(&GameKey::NesSelect) {
                buttons |= ControllerButtons::SELECT;
            }
            if pressed.contains(&GameKey::NesStart) {
                buttons |= ControllerButtons::START;
            }
        }
        drop(pressed);

        let gamepads = self.gamepads.borrow();
        let Some(gamepad) = gamepads.seat(player).filter(|gamepad| gamepad.connected) else {
            return buttons;
        };
        if gamepad.dpad_up {
            buttons |= ControllerButtons::UP;
        }
        if gamepad.dpad_down {
            buttons |= ControllerButtons::DOWN;
        }
        if gamepad.dpad_left {
            buttons |= ControllerButtons::LEFT;
        }
        if gamepad.dpad_right {
            buttons |= ControllerButtons::RIGHT;
        }
        if gamepad.south {
            buttons |= ControllerButtons::A;
        }
        if gamepad.east {
            buttons |= ControllerButtons::B;
        }
        if gamepad.select {
            buttons |= ControllerButtons::SELECT;
        }
        if gamepad.start {
            buttons |= ControllerButtons::START;
        }
        buttons
    }

    pub(crate) fn reset_spacewars_controls(&mut self) {
        self.spacewars_controls.reset();
    }

    pub(crate) fn runtime_diagnostics_revision(&self) -> u64 {
        self.spacewars_controls.diagnostics_revision
    }

    pub(crate) fn runtime_diagnostics_text(&self) -> String {
        let diagnostics = self
            .spacewars_controls
            .bot_diagnostics
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if diagnostics.is_empty() {
            "No active rule-bot diagnostics.".into()
        } else {
            diagnostics.join("\n\n")
        }
    }

    pub(crate) fn paused_runtime_diagnostics_text(&self) -> String {
        let mut diagnostics = self.runtime_diagnostics_text();
        diagnostics.push_str("\n\n");
        diagnostics.push_str(&self.spacewars_controls.flight_history_text());
        diagnostics
    }

    pub(crate) fn clear(&mut self) {
        self.clear_keyboard();
        self.pointer_events.clear();
        self.active_pointer = None;
    }

    fn clear_keyboard(&mut self) {
        self.pressed.borrow_mut().clear();
    }

    fn handle_focus_loss(&mut self) {
        // Focus changes are routine on desktop: switching applications and
        // invoking screenshot tools both generate them. Release momentary
        // keyboard/pointer input so controls cannot stick, but leave pausing to
        // an explicit request or an actual controller disconnect.
        self.cancel_pointer();
        self.clear_keyboard();
    }

    pub(crate) fn take_pointer_events(&mut self) -> Vec<ScreenPointerEvent> {
        std::mem::take(&mut self.pointer_events)
    }

    pub(crate) fn has_pointer_cancellation(&self) -> bool {
        self.pointer_events
            .iter()
            .any(|event| event.phase == PointerPhase::Cancel)
    }

    pub(crate) fn cancel_pointer(&mut self) {
        let Some(position) = self.active_pointer.take() else {
            return;
        };
        self.pointer_events.push(ScreenPointerEvent {
            position,
            phase: PointerPhase::Cancel,
        });
    }

    pub(crate) fn discard_pointer_events(&mut self) {
        self.pointer_events.clear();
        self.active_pointer = None;
    }

    pub(crate) fn push_pointer_event(&mut self, event: ScreenPointerEvent) {
        match event.phase {
            PointerPhase::Press => {
                self.active_pointer = Some(event.position);
                self.pointer_events.push(event);
            }
            PointerPhase::Drag => {
                if self.active_pointer.is_some() {
                    self.active_pointer = Some(event.position);
                    self.pointer_events.push(event);
                }
            }
            PointerPhase::Release => {
                if self.active_pointer.take().is_some() {
                    self.pointer_events.push(event);
                }
            }
            PointerPhase::Cancel => {
                self.cancel_pointer();
            }
        }
    }

    pub(crate) fn press(&mut self, key: GameKey) {
        self.pressed.borrow_mut().insert(key);
    }

    pub(crate) fn release(&mut self, key: GameKey) {
        self.pressed.borrow_mut().remove(&key);
    }
}

#[derive(Debug, Clone, Copy)]
struct PlayerControlMap {
    player: usize,
    wing: GameKey,
    thrust: GameKey,
    brake: GameKey,
    reverse: GameKey,
    turn_left: GameKey,
    turn_right: GameKey,
    laser: GameKey,
    cannon: GameKey,
    zoom_in: GameKey,
    zoom_out: GameKey,
}

const P1_CONTROLS: PlayerControlMap = PlayerControlMap {
    player: 0,
    wing: GameKey::P1Wing,
    thrust: GameKey::P1Thrust,
    brake: GameKey::P1Brake,
    reverse: GameKey::P1Reverse,
    turn_left: GameKey::P1TurnLeft,
    turn_right: GameKey::P1TurnRight,
    laser: GameKey::P1Laser,
    cannon: GameKey::P1Cannon,
    zoom_in: GameKey::P1ZoomIn,
    zoom_out: GameKey::P1ZoomOut,
};

const P2_CONTROLS: PlayerControlMap = PlayerControlMap {
    player: 1,
    wing: GameKey::P2Wing,
    thrust: GameKey::P2Thrust,
    brake: GameKey::P2Brake,
    reverse: GameKey::P2Reverse,
    turn_left: GameKey::P2TurnLeft,
    turn_right: GameKey::P2TurnRight,
    laser: GameKey::P2Laser,
    cannon: GameKey::P2Cannon,
    zoom_in: GameKey::P2ZoomIn,
    zoom_out: GameKey::P2ZoomOut,
};

const FLIGHT_EVENT_INITIAL: u16 = 1 << 0;
const FLIGHT_EVENT_AVOIDANCE_CHANGED: u16 = 1 << 1;
const FLIGHT_EVENT_CONTACT_CHANGED: u16 = 1 << 2;
const FLIGHT_EVENT_ASSIST_CHANGED: u16 = 1 << 3;
const FLIGHT_EVENT_EMERGENCY_CHANGED: u16 = 1 << 4;
const FLIGHT_EVENT_FORM_CHANGED: u16 = 1 << 5;
const FLIGHT_EVENT_DOCK_CHANGED: u16 = 1 << 6;

#[derive(Debug, Clone, Copy, PartialEq)]
struct FlightEpisodeContext {
    seed: u64,
    universe_radius: u32,
    asteroid_probability_per_sec: f32,
    planets: bool,
    fps: u32,
    player_health_percent: [u32; 2],
}

impl FlightEpisodeContext {
    fn from_state(state: &SpacewarsState) -> Self {
        Self {
            seed: state.seed,
            universe_radius: state.config.universe_radius,
            asteroid_probability_per_sec: state.config.asteroid_probability_per_sec,
            planets: state.config.use_planets,
            fps: state.config.fps,
            player_health_percent: [
                state.config.players[0].health_percent,
                state.config.players[1].health_percent,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PredictedBodyApproach {
    body: AvoidanceBody,
    local_position: Vec2,
    local_velocity: Vec2,
    current_clearance: f32,
    time_to_impact: Option<f32>,
    closest_time: f32,
    closest_clearance: f32,
}

#[derive(Debug, Clone, Copy)]
struct BotFlightSample {
    seed: u64,
    tick: u64,
    events: u16,
    telemetry: BrainTelemetry,
    intent: ShipIntent,
    form: ShipForm,
    life_fraction: f32,
    wings_closed: bool,
    docked_planet: Option<PlanetId>,
    position: Vec2,
    velocity: Vec2,
    omega: f32,
    contact: Option<BodyId>,
    contact_incident: Option<BodyId>,
    predicted_approach: Option<PredictedBodyApproach>,
}

#[derive(Debug, Clone, Copy)]
struct FlightContactIncident {
    body: BodyId,
    last_seen_tick: u64,
}

#[derive(Debug, Clone, Copy)]
enum FlightEncounterTrigger {
    BodyContact(BodyId),
    EscapeAssist(Option<AvoidanceBody>),
    EmergencyEscape(Option<AvoidanceBody>),
}

impl std::fmt::Display for FlightEncounterTrigger {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BodyContact(body) => write!(formatter, "body_contact:{body:?}"),
            Self::EscapeAssist(body) => write!(formatter, "escape_assist:{body:?}"),
            Self::EmergencyEscape(body) => write!(formatter, "emergency_escape:{body:?}"),
        }
    }
}

struct FlightEncounterCapture {
    trigger: FlightEncounterTrigger,
    trigger_tick: u64,
    samples: Vec<BotFlightSample>,
    collecting: bool,
}

struct BotFlightHistory {
    context: Option<FlightEpisodeContext>,
    recent: VecDeque<BotFlightSample>,
    encounter: Option<FlightEncounterCapture>,
    last_sample_tick: Option<u64>,
    initialized: bool,
    previous_avoided_body: Option<AvoidanceBody>,
    contact_incident: Option<FlightContactIncident>,
    previous_escape_assist: bool,
    previous_emergency_assist: bool,
    previous_form: ShipForm,
    previous_docked_planet: Option<PlanetId>,
}

impl Default for BotFlightHistory {
    fn default() -> Self {
        Self {
            context: None,
            recent: VecDeque::with_capacity(FLIGHT_RECENT_CAPACITY),
            encounter: None,
            last_sample_tick: None,
            initialized: false,
            previous_avoided_body: None,
            contact_incident: None,
            previous_escape_assist: false,
            previous_emergency_assist: false,
            previous_form: ShipForm::Ship,
            previous_docked_planet: None,
        }
    }
}

impl BotFlightHistory {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn is_empty(&self) -> bool {
        self.recent.is_empty() && self.encounter.is_none()
    }

    fn record(
        &mut self,
        state: &SpacewarsState,
        observation: &ShipObservationV1,
        telemetry: BrainTelemetry,
        intent: ShipIntent,
    ) {
        let context = FlightEpisodeContext::from_state(state);
        if self.context != Some(context) {
            self.clear();
            self.context = Some(context);
        }

        let contact = state
            .body_collisions
            .iter()
            .find(|collision| collision.ship == observation.actor.index())
            .map(|collision| collision.body);
        // Accepted spaceport contact is intentionally represented in each
        // sample, but it is not a crash and should not replace a mechanical
        // encounter capture.
        let mechanical_contact = contact.filter(|body| {
            !matches!(
                (body, observation.own_ship.docked_planet),
                (BodyId::Planet(index), Some(planet)) if *index == planet.index()
            )
        });

        let previous_contact_incident = self.contact_incident.map(|incident| incident.body);
        if self.contact_incident.is_some_and(|incident| {
            state.tick.saturating_sub(incident.last_seen_tick) >= FLIGHT_CONTACT_REARM_TICKS
        }) {
            self.contact_incident = None;
        }
        let mut mechanical_contact_started = false;
        if let Some(body) = mechanical_contact {
            match self.contact_incident.as_mut() {
                Some(incident) if incident.body == body => incident.last_seen_tick = state.tick,
                _ => {
                    self.contact_incident = Some(FlightContactIncident {
                        body,
                        last_seen_tick: state.tick,
                    });
                    mechanical_contact_started = true;
                }
            }
        }
        let contact_incident = self.contact_incident.map(|incident| incident.body);

        let mut events = 0;
        if !self.initialized {
            events |= FLIGHT_EVENT_INITIAL;
        } else {
            if telemetry.avoided_body != self.previous_avoided_body {
                events |= FLIGHT_EVENT_AVOIDANCE_CHANGED;
            }
            if contact_incident != previous_contact_incident {
                events |= FLIGHT_EVENT_CONTACT_CHANGED;
            }
            if telemetry.avoidance_escape_assist != self.previous_escape_assist {
                events |= FLIGHT_EVENT_ASSIST_CHANGED;
            }
            if telemetry.avoidance_emergency_escape_assist != self.previous_emergency_assist {
                events |= FLIGHT_EVENT_EMERGENCY_CHANGED;
            }
            if observation.own_ship.form != self.previous_form {
                events |= FLIGHT_EVENT_FORM_CHANGED;
            }
            if observation.own_ship.docked_planet != self.previous_docked_planet {
                events |= FLIGHT_EVENT_DOCK_CHANGED;
            }
        }

        let escape_assist_started =
            telemetry.avoidance_escape_assist && !self.previous_escape_assist;
        let emergency_assist_started =
            telemetry.avoidance_emergency_escape_assist && !self.previous_emergency_assist;
        let sample_due = events != 0
            || self.last_sample_tick.is_none_or(|last_tick| {
                state.tick.saturating_sub(last_tick)
                    >= flight_sample_interval_ticks(state.config.fps)
            });

        self.initialized = true;
        self.previous_avoided_body = telemetry.avoided_body;
        self.previous_escape_assist = telemetry.avoidance_escape_assist;
        self.previous_emergency_assist = telemetry.avoidance_emergency_escape_assist;
        self.previous_form = observation.own_ship.form;
        self.previous_docked_planet = observation.own_ship.docked_planet;

        if !sample_due {
            return;
        }

        let ship = &state.ships[observation.actor.index()];
        let sample = BotFlightSample {
            seed: state.seed,
            tick: state.tick,
            events,
            telemetry,
            intent,
            form: observation.own_ship.form,
            life_fraction: observation.own_ship.life_fraction,
            wings_closed: observation.own_ship.wings_closed,
            docked_planet: observation.own_ship.docked_planet,
            position: ship.position,
            velocity: ship.velocity,
            omega: ship.omega,
            contact,
            contact_incident,
            predicted_approach: predicted_body_approach(observation),
        };
        self.push_recent(sample);
        self.last_sample_tick = Some(state.tick);

        let trigger = if mechanical_contact_started {
            mechanical_contact.map(FlightEncounterTrigger::BodyContact)
        } else if emergency_assist_started {
            Some(FlightEncounterTrigger::EmergencyEscape(
                telemetry.avoided_body,
            ))
        } else if escape_assist_started {
            Some(FlightEncounterTrigger::EscapeAssist(telemetry.avoided_body))
        } else {
            None
        };

        let can_start_encounter = self
            .encounter
            .as_ref()
            .is_none_or(|encounter| !encounter.collecting);
        if let Some(trigger) = trigger.filter(|_| can_start_encounter) {
            let start = self
                .recent
                .len()
                .saturating_sub(FLIGHT_ENCOUNTER_PRE_SAMPLES);
            let mut samples = Vec::with_capacity(FLIGHT_ENCOUNTER_CAPACITY);
            samples.extend(self.recent.iter().skip(start).copied());
            self.encounter = Some(FlightEncounterCapture {
                trigger,
                trigger_tick: state.tick,
                samples,
                collecting: true,
            });
        } else if let Some(encounter) = self.encounter.as_mut().filter(|encounter| {
            encounter.collecting
                && encounter
                    .samples
                    .last()
                    .is_none_or(|last| last.tick != sample.tick)
        }) {
            encounter.samples.push(sample);
            if encounter.samples.len() >= FLIGHT_ENCOUNTER_CAPACITY {
                encounter.collecting = false;
            }
        }
    }

    fn push_recent(&mut self, sample: BotFlightSample) {
        if self.recent.len() == FLIGHT_RECENT_CAPACITY {
            self.recent.pop_front();
        }
        self.recent.push_back(sample);
    }

    fn write_text(&self, player: usize, output: &mut String) {
        let _ = writeln!(
            output,
            "flight_history player={} sample_rate_hz={} sample_interval_ticks={} recent_capacity={} encounter_pre_samples={} encounter_capacity={} contact_rearm_ticks={}",
            player + 1,
            FLIGHT_SAMPLE_RATE_HZ,
            self.context
                .map_or(1, |context| flight_sample_interval_ticks(context.fps)),
            FLIGHT_RECENT_CAPACITY,
            FLIGHT_ENCOUNTER_PRE_SAMPLES,
            FLIGHT_ENCOUNTER_CAPACITY,
            FLIGHT_CONTACT_REARM_TICKS,
        );
        output.push_str(
            "flight_intent_order=(turn,thrust,brake,wings_closed,laser,cannon) prediction_horizon_seconds=5\n",
        );
        if let Some(context) = self.context {
            let _ = writeln!(
                output,
                "episode seed={} universe_radius={} asteroid_probability_per_sec={:.3} planets={} fps={} player_health_percent={:?}",
                context.seed,
                context.universe_radius,
                context.asteroid_probability_per_sec,
                context.planets,
                context.fps,
                context.player_health_percent,
            );
        }

        let encounter_last_tick = if let Some(encounter) = &self.encounter {
            let _ = writeln!(
                output,
                "encounter trigger={} trigger_tick={} collecting={} samples={}",
                encounter.trigger,
                encounter.trigger_tick,
                encounter.collecting,
                encounter.samples.len(),
            );
            for sample in &encounter.samples {
                write_flight_sample(output, sample);
            }
            encounter.samples.last().map(|sample| sample.tick)
        } else {
            None
        };

        let recent = self
            .recent
            .iter()
            .filter(|sample| encounter_last_tick.is_none_or(|tick| sample.tick > tick))
            .collect::<Vec<_>>();
        if !recent.is_empty() {
            let _ = writeln!(output, "recent samples={}", recent.len());
            for sample in recent {
                write_flight_sample(output, sample);
            }
        }
    }
}

fn flight_sample_interval_ticks(fps: u32) -> u64 {
    u64::from((fps.saturating_add(FLIGHT_SAMPLE_RATE_HZ / 2) / FLIGHT_SAMPLE_RATE_HZ).max(1))
}

fn predicted_body_approach(observation: &ShipObservationV1) -> Option<PredictedBodyApproach> {
    let mut best = observation.sun.map(|sun| {
        body_approach(
            AvoidanceBody::Sun,
            sun.local_position,
            sun.local_velocity,
            sun.radius + observation.own_ship.collision_radius,
        )
    });
    for planet in &observation.planets {
        let candidate = body_approach(
            AvoidanceBody::Planet(planet.id),
            planet.local_position,
            planet.local_velocity,
            planet.radius + observation.own_ship.collision_radius,
        );
        if best.is_none_or(|current| body_approach_precedes(candidate, current)) {
            best = Some(candidate);
        }
    }
    best
}

fn body_approach(
    body: AvoidanceBody,
    local_position: Vec2,
    local_velocity: Vec2,
    collision_radius: f32,
) -> PredictedBodyApproach {
    let speed_squared = local_velocity.length_squared();
    let current_clearance = local_position.length() - collision_radius;
    if speed_squared <= 1.0e-6 {
        return PredictedBodyApproach {
            body,
            local_position,
            local_velocity,
            current_clearance,
            time_to_impact: (current_clearance <= 0.0).then_some(0.0),
            closest_time: 0.0,
            closest_clearance: current_clearance,
        };
    }

    let closest_time = (-local_position.dot(local_velocity) / speed_squared)
        .clamp(0.0, BODY_PREDICTION_HORIZON_SECONDS);
    let closest_clearance =
        (local_position + local_velocity * closest_time).length() - collision_radius;
    let time_to_impact = if current_clearance <= 0.0 {
        Some(0.0)
    } else {
        let b = 2.0 * local_position.dot(local_velocity);
        let c = local_position.length_squared() - collision_radius * collision_radius;
        let discriminant = b * b - 4.0 * speed_squared * c;
        if discriminant < 0.0 {
            None
        } else {
            let root = (-b - discriminant.sqrt()) / (2.0 * speed_squared);
            (0.0..=BODY_PREDICTION_HORIZON_SECONDS)
                .contains(&root)
                .then_some(root)
        }
    };
    PredictedBodyApproach {
        body,
        local_position,
        local_velocity,
        current_clearance,
        time_to_impact,
        closest_time,
        closest_clearance,
    }
}

fn body_approach_precedes(
    candidate: PredictedBodyApproach,
    current: PredictedBodyApproach,
) -> bool {
    match (candidate.time_to_impact, current.time_to_impact) {
        (Some(candidate_time), Some(current_time)) => candidate_time < current_time,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => candidate.closest_clearance < current.closest_clearance,
    }
}

fn write_flight_sample(output: &mut String, sample: &BotFlightSample) {
    let prediction = sample.predicted_approach;
    let _ = writeln!(
        output,
        "flight tick={} seed={} events={} form={:?} life={:.3} pos=({:.2},{:.2}) vel=({:.2},{:.2}) speed={:.2} omega={:.3} wings={} dock={:?} contact={:?} contact_incident={:?} strategy={:?} strategy_planet={:?} strategy_score={} strategy_age={} strategy_reason={:?} brain={:?} brain_planet={:?} port={:?} hazard={:?} avoid={:?} clearance={} outward={} predictive={} avoid_closest_time={} avoid_predicted_clearance={} avoid_age={} stalled={} assist={} emergency={} target_distance={:.3} heading={:.3} desired_speed={:.3} relative_speed={:.3} predict={:?} body_pos=({},{}) body_vel=({},{}) predict_clearance={} impact_time={} closest_time={} closest_clearance={} intent=({:.2},{:.2},{:.2},{},{},{})",
        sample.tick,
        sample.seed,
        flight_event_text(sample.events),
        sample.form,
        sample.life_fraction,
        sample.position.x,
        sample.position.y,
        sample.velocity.x,
        sample.velocity.y,
        sample.velocity.length(),
        sample.omega,
        sample.wings_closed,
        sample.docked_planet,
        sample.contact,
        sample.contact_incident,
        sample.telemetry.strategy.goal,
        sample.telemetry.strategy.target_planet,
        optional_f32(sample.telemetry.strategy.selected_score),
        sample.telemetry.strategy.age_ticks,
        sample.telemetry.strategy.selection_reason,
        sample.telemetry.goal,
        sample.telemetry.target_planet,
        sample.telemetry.port_phase,
        sample.telemetry.hazard,
        sample.telemetry.avoided_body,
        optional_f32(sample.telemetry.avoidance_surface_clearance),
        optional_f32(sample.telemetry.avoidance_outward_speed),
        sample.telemetry.avoidance_predictive,
        optional_f32(sample.telemetry.avoidance_seconds_until_closest),
        optional_f32(sample.telemetry.avoidance_predicted_surface_clearance),
        sample.telemetry.avoidance_age_ticks,
        sample.telemetry.avoidance_stalled_ticks,
        sample.telemetry.avoidance_escape_assist,
        sample.telemetry.avoidance_emergency_escape_assist,
        sample.telemetry.target_distance,
        sample.telemetry.heading_error,
        sample.telemetry.desired_speed,
        sample.telemetry.relative_speed,
        prediction.map(|approach| approach.body),
        optional_f32(prediction.map(|approach| approach.local_position.x)),
        optional_f32(prediction.map(|approach| approach.local_position.y)),
        optional_f32(prediction.map(|approach| approach.local_velocity.x)),
        optional_f32(prediction.map(|approach| approach.local_velocity.y)),
        optional_f32(prediction.map(|approach| approach.current_clearance)),
        optional_f32(prediction.and_then(|approach| approach.time_to_impact)),
        optional_f32(prediction.map(|approach| approach.closest_time)),
        optional_f32(prediction.map(|approach| approach.closest_clearance)),
        sample.intent.turn,
        sample.intent.thrust,
        sample.intent.brake,
        sample.intent.wings_closed,
        sample.intent.laser,
        sample.intent.cannon,
    );
}

fn flight_event_text(events: u16) -> String {
    let names = [
        (FLIGHT_EVENT_INITIAL, "initial"),
        (FLIGHT_EVENT_AVOIDANCE_CHANGED, "avoidance"),
        (FLIGHT_EVENT_CONTACT_CHANGED, "contact"),
        (FLIGHT_EVENT_ASSIST_CHANGED, "assist"),
        (FLIGHT_EVENT_EMERGENCY_CHANGED, "emergency"),
        (FLIGHT_EVENT_FORM_CHANGED, "form"),
        (FLIGHT_EVENT_DOCK_CHANGED, "dock"),
    ];
    let mut output = String::new();
    for (flag, name) in names {
        if events & flag == 0 {
            continue;
        }
        if !output.is_empty() {
            output.push('|');
        }
        output.push_str(name);
    }
    if output.is_empty() {
        output.push('-');
    }
    output
}

fn optional_f32(value: Option<f32>) -> String {
    value.map_or_else(|| "-".into(), |value| format!("{value:.3}"))
}

struct SpacewarsControls {
    seats: [ControlSeat; 2],
    rule_brains: [Box<dyn ShipBrain>; 2],
    brain_contexts: [Option<BrainReset>; 2],
    active_sources: [SpacewarsControlMode; 2],
    encoder: ShipIntentEncoder,
    bot_diagnostics: [Option<String>; 2],
    flight_histories: [BotFlightHistory; 2],
    diagnostics_revision: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SpacewarsControlMode {
    #[default]
    Human,
    RuleBot,
    Benchmark,
}

impl SpacewarsControls {
    fn new(pressed: SharedKeyboardState, gamepads: SharedGamepadInput) -> Self {
        let mut p1 =
            ControlSeat::with_source(KeyboardSource::new(Rc::clone(&pressed), P1_CONTROLS));
        p1.add_source(GamepadSource::new(Rc::clone(&gamepads), 0));
        let mut p2 = ControlSeat::with_source(KeyboardSource::new(pressed, P2_CONTROLS));
        p2.add_source(GamepadSource::new(gamepads, 1));
        Self {
            seats: [p1, p2],
            rule_brains: std::array::from_fn(|_| DEFAULT_BUILT_IN_POLICY.create()),
            brain_contexts: [None, None],
            active_sources: [SpacewarsControlMode::Human; 2],
            encoder: ShipIntentEncoder::default(),
            bot_diagnostics: std::array::from_fn(|_| None),
            flight_histories: std::array::from_fn(|_| BotFlightHistory::default()),
            diagnostics_revision: 0,
        }
    }

    fn actions(
        &mut self,
        state: &SpacewarsState,
        benchmark_active: bool,
        bot_players: [bool; 2],
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        for (player, bot_player) in bot_players.into_iter().enumerate() {
            let mode = if benchmark_active {
                SpacewarsControlMode::Benchmark
            } else if bot_player {
                SpacewarsControlMode::RuleBot
            } else {
                SpacewarsControlMode::Human
            };

            if mode != SpacewarsControlMode::RuleBot
                && self.bot_diagnostics[player].take().is_some()
            {
                self.flight_histories[player].clear();
                self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
            }

            // Always send a neutral frame before a different source can take
            // over. Besides making live handoff predictable, this releases any
            // held laser/thrust state left by the previous source.
            let intent = if self.active_sources[player] != mode {
                self.active_sources[player] = mode;
                self.brain_contexts[player] = None;
                ShipIntent::default()
            } else {
                match mode {
                    SpacewarsControlMode::Human => self.seats[player].intent(),
                    SpacewarsControlMode::RuleBot => self.rule_bot_intent(state, player),
                    SpacewarsControlMode::Benchmark => {
                        SpacewarsScenario::benchmark_intent(state, player)
                    }
                }
            };
            actions.extend(self.encoder.encode(player, intent));
        }
        actions
    }

    fn rule_bot_intent(&mut self, state: &SpacewarsState, player: usize) -> ShipIntent {
        let Some(actor) = PlayerId::from_index(player) else {
            return ShipIntent::default();
        };
        let context = BrainReset {
            actor,
            episode_seed: state.seed,
        };
        if self.brain_contexts[player] != Some(context) {
            self.rule_brains[player].reset(context);
            self.brain_contexts[player] = Some(context);
        }
        let Some(observation) =
            SpacewarsScenario::observe_ship(state, actor, ShipSensorProfile::FullMapRadar)
        else {
            return ShipIntent::default();
        };
        let intent = self.rule_brains[player].intent(&observation);
        let telemetry = self.rule_brains[player].telemetry();
        self.flight_histories[player].record(state, &observation, telemetry, intent);
        if state.tick.is_multiple_of(60) {
            let target_planet = telemetry.target_planet.and_then(|target| {
                observation
                    .planets
                    .iter()
                    .find(|planet| planet.id == target)
            });
            tracing::debug!(
                player = player + 1,
                seed = state.seed,
                tick = state.tick,
                strategy_goal = ?telemetry.strategy.goal,
                strategy_target = ?telemetry.strategy.target,
                strategy_target_planet = ?telemetry.strategy.target_planet,
                strategy_score = ?telemetry.strategy.selected_score,
                strategy_age_ticks = telemetry.strategy.age_ticks,
                strategy_reason = ?telemetry.strategy.selection_reason,
                strategy_scores = ?telemetry.strategy.scores,
                goal = ?telemetry.goal,
                target = ?telemetry.target,
                target_planet = ?telemetry.target_planet,
                port_phase = ?telemetry.port_phase,
                hazard = ?telemetry.hazard,
                avoided_body = ?telemetry.avoided_body,
                avoidance_surface_clearance = ?telemetry.avoidance_surface_clearance,
                avoidance_outward_speed = ?telemetry.avoidance_outward_speed,
                avoidance_predictive = telemetry.avoidance_predictive,
                avoidance_seconds_until_closest = ?telemetry.avoidance_seconds_until_closest,
                avoidance_predicted_surface_clearance = ?telemetry
                    .avoidance_predicted_surface_clearance,
                avoidance_age_ticks = telemetry.avoidance_age_ticks,
                avoidance_stalled_ticks = telemetry.avoidance_stalled_ticks,
                avoidance_escape_assist = telemetry.avoidance_escape_assist,
                avoidance_emergency_escape_assist = telemetry
                    .avoidance_emergency_escape_assist,
                life_fraction = observation.own_ship.life_fraction,
                distance = telemetry.target_distance,
                heading_error = telemetry.heading_error,
                desired_speed = telemetry.desired_speed,
                relative_speed = telemetry.relative_speed,
                docked_planet = ?observation.own_ship.docked_planet,
                planet_owner = ?target_planet.and_then(|planet| planet.owner),
                capturing_player = ?target_planet.and_then(|planet| planet.capturing_player),
                capture_progress = target_planet.map_or(0.0, |planet| planet.capture_progress),
                ?intent,
                "Spacewars rule-bot telemetry."
            );
            let avoidance_surface_clearance = telemetry
                .avoidance_surface_clearance
                .map_or_else(|| "none".into(), |clearance| format!("{clearance:.3}"));
            let avoidance_outward_speed = telemetry
                .avoidance_outward_speed
                .map_or_else(|| "none".into(), |speed| format!("{speed:.3}"));
            let avoidance_seconds_until_closest = telemetry
                .avoidance_seconds_until_closest
                .map_or_else(|| "none".into(), |seconds| format!("{seconds:.3}"));
            let avoidance_predicted_surface_clearance = telemetry
                .avoidance_predicted_surface_clearance
                .map_or_else(|| "none".into(), |clearance| format!("{clearance:.3}"));
            let target = target_planet.map_or_else(
                || "none".into(),
                |planet| {
                    format!(
                        "id={:?} owner={:?} capturing={:?} progress={:.3} radius={:.3} center_local={:?} port_local={:?} port_velocity_local={:?} surface_clearance={:.3}",
                        planet.id,
                        planet.owner,
                        planet.capturing_player,
                        planet.capture_progress,
                        planet.radius,
                        planet.local_position,
                        planet.local_spaceport_position,
                        planet.local_spaceport_velocity,
                        planet.local_position.length()
                            - planet.radius
                            - observation.own_ship.collision_radius,
                    )
                },
            );
            let docked = observation.own_ship.docked_planet.map_or_else(
                || "none".into(),
                |docked| {
                    observation
                        .planets
                        .iter()
                        .find(|planet| planet.id == docked)
                        .map_or_else(
                            || format!("id={docked:?} observation=missing"),
                            |planet| {
                                format!(
                                    "id={:?} owner={:?} surface_clearance={:.3} port_distance={:.3}",
                                    planet.id,
                                    planet.owner,
                                    planet.local_position.length()
                                        - planet.radius
                                        - observation.own_ship.collision_radius,
                                    planet.local_spaceport_position.length(),
                                )
                            },
                        )
                },
            );
            let ship = &state.ships[player];
            self.bot_diagnostics[player] = Some(format!(
                "player={} seed={} tick={} planets={} winner={:?}\nstrategy goal={:?} target={:?} target_planet={:?} score={:?} age_ticks={} reason={:?} scores={:?}\nbrain goal={:?} target={:?} target_planet={:?} phase={:?} hazard={:?} avoided_body={:?} avoidance_surface_clearance={} avoidance_outward_speed={} avoidance_predictive={} avoidance_seconds_until_closest={} avoidance_predicted_surface_clearance={} avoidance_age_ticks={} avoidance_stalled_ticks={} avoidance_escape_assist={} avoidance_emergency_escape_assist={} distance={:.3} heading_error={:.3} desired_speed={:.3} relative_speed={:.3}\nintent turn={:.3} thrust={:.3} brake={:.3} wings_closed={} laser={} cannon={}\nship form={:?} life_fraction={:.3} position={:?} velocity={:?} omega={:.3} observed_wings_closed={} docked_planet={:?}\ntarget {target}\ndocked {docked}",
                player + 1,
                state.seed,
                state.tick,
                state.players[player].planet_count,
                state.winner,
                telemetry.strategy.goal,
                telemetry.strategy.target,
                telemetry.strategy.target_planet,
                telemetry.strategy.selected_score,
                telemetry.strategy.age_ticks,
                telemetry.strategy.selection_reason,
                telemetry.strategy.scores,
                telemetry.goal,
                telemetry.target,
                telemetry.target_planet,
                telemetry.port_phase,
                telemetry.hazard,
                telemetry.avoided_body,
                avoidance_surface_clearance,
                avoidance_outward_speed,
                telemetry.avoidance_predictive,
                avoidance_seconds_until_closest,
                avoidance_predicted_surface_clearance,
                telemetry.avoidance_age_ticks,
                telemetry.avoidance_stalled_ticks,
                telemetry.avoidance_escape_assist,
                telemetry.avoidance_emergency_escape_assist,
                telemetry.target_distance,
                telemetry.heading_error,
                telemetry.desired_speed,
                telemetry.relative_speed,
                intent.turn,
                intent.thrust,
                intent.brake,
                intent.wings_closed,
                intent.laser,
                intent.cannon,
                observation.own_ship.form,
                observation.own_ship.life_fraction,
                ship.position,
                ship.velocity,
                ship.omega,
                observation.own_ship.wings_closed,
                observation.own_ship.docked_planet,
            ));
            self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
        }
        intent
    }

    fn flight_history_text(&self) -> String {
        let mut output = String::new();
        let mut found = false;
        for (player, history) in self.flight_histories.iter().enumerate() {
            if history.is_empty() {
                continue;
            }
            if found {
                output.push('\n');
            }
            history.write_text(player, &mut output);
            found = true;
        }
        if !found {
            output.push_str("No rule-bot flight history captured.");
        }
        output
    }

    fn reset(&mut self) {
        self.encoder.reset();
        self.rule_brains = std::array::from_fn(|_| DEFAULT_BUILT_IN_POLICY.create());
        self.brain_contexts = [None, None];
        self.active_sources = [SpacewarsControlMode::Human; 2];
        self.bot_diagnostics = std::array::from_fn(|_| None);
        self.flight_histories = std::array::from_fn(|_| BotFlightHistory::default());
        self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
    }
}

trait ControlSource {
    fn intent(&mut self) -> ShipIntent;
}

struct ControlSeat {
    sources: Vec<Box<dyn ControlSource>>,
}

impl ControlSeat {
    fn with_source(source: impl ControlSource + 'static) -> Self {
        Self {
            sources: vec![Box::new(source)],
        }
    }

    fn add_source(&mut self, source: impl ControlSource + 'static) {
        self.sources.push(Box::new(source));
    }

    fn intent(&mut self) -> ShipIntent {
        self.sources
            .iter_mut()
            .fold(ShipIntent::default(), |intent, source| {
                intent.merged_with(source.intent())
            })
    }
}

struct KeyboardSource {
    pressed: SharedKeyboardState,
    controls: PlayerControlMap,
}

impl KeyboardSource {
    fn new(pressed: SharedKeyboardState, controls: PlayerControlMap) -> Self {
        Self { pressed, controls }
    }
}

impl ControlSource for KeyboardSource {
    fn intent(&mut self) -> ShipIntent {
        let pressed = self.pressed.borrow();
        let thrust = if pressed.contains(&self.controls.thrust) {
            1.0
        } else if pressed.contains(&self.controls.reverse) {
            -1.0
        } else {
            0.0
        };
        let turn = if pressed.contains(&self.controls.turn_left) {
            -1.0
        } else if pressed.contains(&self.controls.turn_right) {
            1.0
        } else {
            0.0
        };
        ShipIntent {
            turn,
            thrust,
            brake: if pressed.contains(&self.controls.brake) {
                1.0
            } else {
                0.0
            },
            wings_closed: pressed.contains(&self.controls.wing),
            laser: pressed.contains(&self.controls.laser),
            cannon: pressed.contains(&self.controls.cannon),
        }
    }
}

struct GamepadSource {
    gamepads: SharedGamepadInput,
    seat: usize,
}

impl GamepadSource {
    fn new(gamepads: SharedGamepadInput, seat: usize) -> Self {
        Self { gamepads, seat }
    }
}

impl ControlSource for GamepadSource {
    fn intent(&mut self) -> ShipIntent {
        let gamepads = self.gamepads.borrow();
        let Some(gamepad) = gamepads.seat(self.seat) else {
            return ShipIntent::default();
        };
        if !gamepad.connected {
            return ShipIntent::default();
        }

        let turn = if gamepad.dpad_left {
            -1.0
        } else if gamepad.dpad_right {
            1.0
        } else {
            shape_stick(gamepad.left_stick_x)
        };
        let forward_thrust = if gamepad.dpad_up {
            1.0
        } else {
            shape_trigger(gamepad.right_trigger)
        };
        let brake = if gamepad.dpad_down {
            1.0
        } else {
            shape_trigger(gamepad.left_trigger)
        };
        ShipIntent {
            turn,
            thrust: if gamepad.east { -1.0 } else { forward_thrust },
            brake,
            wings_closed: gamepad.right_bumper,
            laser: gamepad.south,
            cannon: gamepad.west,
        }
    }
}

const STICK_DEADZONE: f32 = 0.15;
const TRIGGER_DEADZONE: f32 = 0.05;

pub(crate) fn shape_stick(value: f32) -> f32 {
    shape_axis(value, STICK_DEADZONE, true)
}

pub(crate) fn shape_trigger(value: f32) -> f32 {
    shape_axis(value.clamp(0.0, 1.0), TRIGGER_DEADZONE, false)
}

fn shape_axis(value: f32, deadzone: f32, signed: bool) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let value = if signed {
        value.clamp(-1.0, 1.0)
    } else {
        value.clamp(0.0, 1.0)
    };
    let magnitude = value.abs();
    if magnitude <= deadzone {
        return 0.0;
    }

    let normalized = (magnitude - deadzone) / (1.0 - deadzone);
    normalized * normalized * value.signum()
}

fn mapped_key_event(event: &WindowEvent) -> Option<(GameKey, ElementState)> {
    let WindowEvent::KeyboardInput { event, .. } = event else {
        return None;
    };
    if event.repeat {
        return None;
    }

    let PhysicalKey::Code(code) = event.physical_key else {
        return None;
    };
    game_key_from_key_code(code).map(|key| (key, event.state))
}

fn game_key_from_key_code(code: KeyCode) -> Option<GameKey> {
    match code {
        KeyCode::KeyR => Some(GameKey::Reset),
        KeyCode::KeyP => Some(GameKey::Pause),
        KeyCode::KeyB => Some(GameKey::Benchmark),
        KeyCode::Escape => Some(GameKey::Back),
        KeyCode::KeyC | KeyCode::F1 => Some(GameKey::Controls),
        KeyCode::KeyQ => Some(GameKey::ReturnLauncher),
        KeyCode::ArrowUp => Some(GameKey::NesUp),
        KeyCode::ArrowDown => Some(GameKey::NesDown),
        KeyCode::ArrowLeft => Some(GameKey::NesLeft),
        KeyCode::ArrowRight => Some(GameKey::NesRight),
        KeyCode::KeyZ => Some(GameKey::NesA),
        KeyCode::Tab => Some(GameKey::NesSelect),
        KeyCode::Enter => Some(GameKey::NesStart),
        KeyCode::KeyJ => Some(GameKey::P1Wing),
        KeyCode::KeyW => Some(GameKey::P1Thrust),
        KeyCode::KeyS => Some(GameKey::P1Brake),
        KeyCode::KeyX => Some(GameKey::P1Reverse),
        KeyCode::KeyA => Some(GameKey::P1TurnLeft),
        KeyCode::KeyD => Some(GameKey::P1TurnRight),
        KeyCode::Space => Some(GameKey::P1Laser),
        KeyCode::KeyK => Some(GameKey::P1Cannon),
        KeyCode::KeyU => Some(GameKey::P1ZoomIn),
        KeyCode::KeyI => Some(GameKey::P1ZoomOut),
        KeyCode::PageDown => Some(GameKey::P2Wing),
        KeyCode::Numpad8 => Some(GameKey::P2Thrust),
        KeyCode::Numpad5 => Some(GameKey::P2Brake),
        KeyCode::Numpad2 => Some(GameKey::P2Reverse),
        KeyCode::Numpad4 => Some(GameKey::P2TurnLeft),
        KeyCode::Numpad6 => Some(GameKey::P2TurnRight),
        KeyCode::Delete => Some(GameKey::P2Laser),
        KeyCode::End => Some(GameKey::P2Cannon),
        KeyCode::Insert => Some(GameKey::P2ZoomIn),
        KeyCode::Home => Some(GameKey::P2ZoomOut),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_core::SpacewarsConfig;
    use scenario_spacewars::{SpacewarsAction as ScenarioAction, SpacewarsActionKind};

    fn decoded(actions: &[Action]) -> Vec<ScenarioAction> {
        actions.iter().filter_map(ScenarioAction::decode).collect()
    }

    fn has_action(actions: &[ScenarioAction], player: usize, kind: SpacewarsActionKind) -> bool {
        actions
            .iter()
            .any(|action| action.player() == player && action.kind() == kind)
    }

    fn take_spacewars_actions(input: &mut ClientInput) -> Vec<Action> {
        let state = SpacewarsScenario::init(SpacewarsConfig::deathmatch(), 0);
        input.actions_for_spacewars(&state, false, [false; 2])
    }

    #[test]
    fn pointer_cancel_uses_the_last_active_position_once() {
        let mut input = ClientInput::default();
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(10.0, 20.0),
            phase: PointerPhase::Press,
        });
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(30.0, 40.0),
            phase: PointerPhase::Drag,
        });
        input.take_pointer_events();

        input.cancel_pointer();
        input.cancel_pointer();

        assert_eq!(
            input.take_pointer_events(),
            vec![ScreenPointerEvent {
                position: RenderPoint::new(30.0, 40.0),
                phase: PointerPhase::Cancel,
            }]
        );
    }

    #[test]
    fn clearing_keyboard_state_preserves_pointer_cancellation() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Thrust);
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(10.0, 20.0),
            phase: PointerPhase::Press,
        });
        input.take_pointer_events();
        input.cancel_pointer();

        input.clear_keyboard();

        assert!(!input.pressed.borrow().contains(&GameKey::P1Thrust));
        assert!(input.has_pointer_cancellation());
    }

    #[test]
    fn focus_loss_releases_controls_without_requesting_a_host_pause() {
        let mut input = ClientInput::default();
        input.press(GameKey::NesA);
        input.push_pointer_event(ScreenPointerEvent {
            position: RenderPoint::new(10.0, 20.0),
            phase: PointerPhase::Press,
        });
        input.take_pointer_events();

        input.handle_focus_loss();

        assert_eq!(input.nes_controller_buttons(0), ControllerButtons::NONE);
        assert!(input.has_pointer_cancellation());
        assert!(!input.take_force_pause_requested());
    }

    #[test]
    fn keyboard_source_emits_changed_full_scale_setpoints() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Wing);
        input.press(GameKey::P1Thrust);

        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.contains(&ScenarioAction::SetWings {
            player: 0,
            closed: true,
        }));
        assert!(actions.contains(&ScenarioAction::SetThrust {
            player: 0,
            amount: 1.0,
        }));

        input.release(GameKey::P1Wing);
        let actions = decoded(&take_spacewars_actions(&mut input));
        assert_eq!(
            actions,
            vec![ScenarioAction::SetWings {
                player: 0,
                closed: false,
            }]
        );

        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.is_empty());
    }

    #[test]
    fn release_emits_one_changed_zero_setpoint() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1TurnLeft);
        input.press(GameKey::P1Laser);
        take_spacewars_actions(&mut input);

        input.release(GameKey::P1TurnLeft);
        input.release(GameKey::P1Laser);
        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.contains(&ScenarioAction::SetTurn {
            player: 0,
            rate: 0.0,
        }));
        assert!(actions.contains(&ScenarioAction::SetLaser {
            player: 0,
            on: false,
        }));

        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.is_empty());
    }

    #[test]
    fn brake_and_reverse_keys_use_distinct_keyboard_rows() {
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyS),
            Some(GameKey::P1Brake)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyX),
            Some(GameKey::P1Reverse)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad5),
            Some(GameKey::P2Brake)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad2),
            Some(GameKey::P2Reverse)
        );
    }

    #[test]
    fn nes_input_combines_keyboard_and_the_assigned_gamepad() {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_right: true,
                south: true,
                start: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(gamepads);
        input.press(GameKey::NesLeft);
        input.press(GameKey::P1Reverse);

        assert_eq!(
            input.nes_controller_buttons(0),
            ControllerButtons::LEFT
                | ControllerButtons::RIGHT
                | ControllerButtons::A
                | ControllerButtons::B
                | ControllerButtons::START
        );
        assert_eq!(input.nes_controller_buttons(1), ControllerButtons::NONE);
    }

    #[test]
    fn keyboard_source_reports_brake_and_propulsion_independently() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Wing);
        input.press(GameKey::P1Thrust);
        input.press(GameKey::P1Reverse);
        input.press(GameKey::P1Brake);

        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(actions.contains(&ScenarioAction::SetWings {
            player: 0,
            closed: true,
        }));
        assert!(actions.contains(&ScenarioAction::SetBrake {
            player: 0,
            amount: 1.0,
        }));
        assert!(actions.contains(&ScenarioAction::SetThrust {
            player: 0,
            amount: 1.0,
        }));
    }

    #[test]
    fn brake_release_emits_one_zero_setpoint() {
        let mut input = ClientInput::default();
        input.press(GameKey::P1Brake);
        take_spacewars_actions(&mut input);

        input.release(GameKey::P1Brake);
        let actions = decoded(&take_spacewars_actions(&mut input));
        assert_eq!(
            actions,
            vec![ScenarioAction::SetBrake {
                player: 0,
                amount: 0.0,
            }]
        );

        let actions = decoded(&take_spacewars_actions(&mut input));
        assert!(actions.is_empty());
    }

    #[test]
    fn p2_controls_use_numpad_and_navigation_keys() {
        assert_eq!(
            game_key_from_key_code(KeyCode::PageDown),
            Some(GameKey::P2Wing)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad8),
            Some(GameKey::P2Thrust)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad5),
            Some(GameKey::P2Brake)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad2),
            Some(GameKey::P2Reverse)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad4),
            Some(GameKey::P2TurnLeft)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Numpad6),
            Some(GameKey::P2TurnRight)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Delete),
            Some(GameKey::P2Laser)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::End),
            Some(GameKey::P2Cannon)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Insert),
            Some(GameKey::P2ZoomIn)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Home),
            Some(GameKey::P2ZoomOut)
        );
    }

    #[test]
    fn zoom_keys_emit_original_per_player_zoom_actions() {
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyU),
            Some(GameKey::P1ZoomIn)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyI),
            Some(GameKey::P1ZoomOut)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Insert),
            Some(GameKey::P2ZoomIn)
        );
        assert_eq!(
            game_key_from_key_code(KeyCode::Home),
            Some(GameKey::P2ZoomOut)
        );

        let mut input = ClientInput::default();
        input.press(GameKey::P1ZoomIn);
        input.press(GameKey::P2ZoomOut);
        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(has_action(&actions, 0, SpacewarsActionKind::ZoomIn));
        assert!(has_action(&actions, 1, SpacewarsActionKind::ZoomOut));
    }

    #[test]
    fn reset_key_is_one_shot_host_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::Reset);

        assert!(input.take_reset_requested());
        assert!(!input.take_reset_requested());
    }

    #[test]
    fn pause_key_is_one_shot_host_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::Pause);

        assert!(input.take_pause_requested());
        assert!(!input.take_pause_requested());
    }

    #[test]
    fn benchmark_key_is_one_shot_host_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::Benchmark);

        assert!(input.take_benchmark_requested());
        assert!(!input.take_benchmark_requested());
    }

    #[test]
    fn escape_key_is_one_shot_back_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::Back);

        assert!(input.take_back_requested());
        assert!(!input.take_back_requested());
        assert_eq!(game_key_from_key_code(KeyCode::Escape), Some(GameKey::Back));
    }

    #[test]
    fn controls_keys_are_one_shot_menu_controls() {
        let mut input = ClientInput::default();

        input.press(GameKey::Controls);

        assert!(input.take_controls_requested());
        assert!(!input.take_controls_requested());
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyC),
            Some(GameKey::Controls)
        );
        assert_eq!(game_key_from_key_code(KeyCode::F1), Some(GameKey::Controls));
    }

    #[test]
    fn q_key_is_one_shot_return_launcher_control() {
        let mut input = ClientInput::default();

        input.press(GameKey::ReturnLauncher);

        assert!(input.take_return_launcher_requested());
        assert!(!input.take_return_launcher_requested());
        assert_eq!(
            game_key_from_key_code(KeyCode::KeyQ),
            Some(GameKey::ReturnLauncher)
        );
    }

    #[test]
    fn gamepad_curves_apply_deadzones_and_reach_full_scale() {
        assert_eq!(shape_stick(0.14), 0.0);
        assert_eq!(shape_stick(-0.15), 0.0);
        assert!((shape_stick(0.575) - 0.25).abs() < 0.001);
        assert_eq!(shape_stick(-1.0), -1.0);
        assert_eq!(shape_trigger(0.04), 0.0);
        assert_eq!(shape_trigger(1.0), 1.0);
        assert_eq!(shape_stick(f32::NAN), 0.0);
    }

    #[test]
    fn gamepad_source_maps_ship_controls_and_reverse_wins() {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                left_stick_x: 1.0,
                left_trigger: 0.5,
                right_trigger: 0.75,
                dpad_up: true,
                right_bumper: true,
                south: true,
                east: true,
                west: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(gamepads);
        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(actions.contains(&ScenarioAction::SetTurn {
            player: 0,
            rate: 1.0,
        }));
        assert!(actions.contains(&ScenarioAction::SetThrust {
            player: 0,
            amount: -1.0,
        }));
        assert!(actions.iter().any(|action| matches!(
            action,
            ScenarioAction::SetBrake {
                player: 0,
                amount,
            } if *amount > 0.0 && *amount < 0.5
        )));
        assert!(actions.contains(&ScenarioAction::SetWings {
            player: 0,
            closed: true,
        }));
        assert!(actions.contains(&ScenarioAction::SetLaser {
            player: 0,
            on: true,
        }));
        assert!(actions.contains(&ScenarioAction::SetCannon {
            player: 0,
            on: true,
        }));
    }

    #[test]
    fn dpad_controls_digital_flight_without_changing_zoom() {
        let gamepads = Rc::new(RefCell::new(GamepadInput::default()));
        gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_right: true,
                dpad_up: true,
                ..GamepadSeatInput::default()
            },
        );
        let mut input = ClientInput::new(gamepads);
        input.press(GameKey::P1TurnLeft);
        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(actions.contains(&ScenarioAction::SetTurn {
            player: 0,
            rate: 1.0,
        }));
        assert!(actions.contains(&ScenarioAction::SetThrust {
            player: 0,
            amount: 1.0,
        }));
        assert!(!has_action(&actions, 0, SpacewarsActionKind::ZoomIn));

        input.gamepads.borrow_mut().set_seat(
            0,
            GamepadSeatInput {
                connected: true,
                dpad_down: true,
                ..GamepadSeatInput::default()
            },
        );
        let actions = decoded(&take_spacewars_actions(&mut input));

        assert!(actions.contains(&ScenarioAction::SetBrake {
            player: 0,
            amount: 1.0,
        }));
        assert!(!has_action(&actions, 0, SpacewarsActionKind::ZoomOut));
    }

    #[test]
    fn rule_bot_waits_one_neutral_frame_before_taking_control() {
        let state = SpacewarsScenario::init(SpacewarsConfig::deathmatch(), 7);
        let mut input = ClientInput::default();

        let first = decoded(&input.actions_for_spacewars(&state, false, [false, true]));
        assert!(!first.iter().any(|action| action.player() == 1));

        let second = decoded(&input.actions_for_spacewars(&state, false, [false, true]));
        assert!(second.iter().any(|action| matches!(
            action,
            ScenarioAction::SetTurn { player: 1, rate } if rate.abs() > 0.0
        )));
        let diagnostics = input.runtime_diagnostics_text();
        assert!(diagnostics.contains("player=2 seed=7 tick=0"));
        assert!(diagnostics.contains("brain goal=Attack"));
        assert!(diagnostics.contains("avoided_body=None avoidance_surface_clearance=none"));
        assert!(diagnostics.contains(
            "avoidance_outward_speed=none avoidance_predictive=false avoidance_seconds_until_closest=none avoidance_predicted_surface_clearance=none avoidance_age_ticks=0 avoidance_stalled_ticks=0 avoidance_escape_assist=false avoidance_emergency_escape_assist=false"
        ));
        assert!(diagnostics.contains("ship form=Ship life_fraction=1.000"));
        assert!(diagnostics.contains("docked none"));

        input.reset_spacewars_controls();
        let after_reset = decoded(&input.actions_for_spacewars(&state, false, [false, true]));
        assert!(!after_reset.iter().any(|action| action.player() == 1));
    }

    #[test]
    fn changing_p2_from_human_to_bot_releases_held_controls_first() {
        let state = SpacewarsScenario::init(SpacewarsConfig::deathmatch(), 7);
        let mut input = ClientInput::default();
        input.press(GameKey::P2Laser);

        let human = decoded(&input.actions_for_spacewars(&state, false, [false; 2]));
        assert!(human.contains(&ScenarioAction::SetLaser {
            player: 1,
            on: true,
        }));

        let handoff = decoded(&input.actions_for_spacewars(&state, false, [false, true]));
        assert_eq!(
            handoff,
            vec![ScenarioAction::SetLaser {
                player: 1,
                on: false,
            }]
        );

        let controlled = decoded(&input.actions_for_spacewars(&state, false, [false, true]));
        assert!(controlled.iter().any(|action| matches!(
            action,
            ScenarioAction::SetTurn { player: 1, rate } if rate.abs() > 0.0
        )));
    }

    #[test]
    fn body_prediction_reports_time_to_planet_impact() {
        let state = SpacewarsScenario::init(SpacewarsConfig::default(), 17);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        observation.sun = None;
        observation.planets.truncate(1);
        let planet = &mut observation.planets[0];
        planet.local_position = Vec2::new(0.0, 100.0);
        planet.local_velocity = Vec2::new(0.0, -20.0);
        planet.radius = 20.0;
        let planet_id = planet.id;
        let planet_radius = planet.radius;

        let approach = predicted_body_approach(&observation).unwrap();
        let expected_time = (100.0 - planet_radius - observation.own_ship.collision_radius) / 20.0;

        assert_eq!(approach.body, AvoidanceBody::Planet(planet_id));
        assert!(
            (approach.current_clearance
                - (100.0 - planet_radius - observation.own_ship.collision_radius))
                .abs()
                < 0.001
        );
        assert!((approach.time_to_impact.unwrap() - expected_time).abs() < 0.001);
        assert_eq!(approach.closest_time, BODY_PREDICTION_HORIZON_SECONDS);
        assert!(approach.closest_clearance < 0.0);
    }

    #[test]
    fn flight_history_preserves_pre_contact_samples_after_recent_window_rolls() {
        let mut state = SpacewarsScenario::init(SpacewarsConfig::default(), 23);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        let planet = observation.planets[0].id;
        let mut telemetry = BrainTelemetry::default();
        let mut history = BotFlightHistory::default();

        for tick in 0..19 {
            state.tick = tick;
            observation.tick = tick;
            history.record(&state, &observation, telemetry, ShipIntent::default());
        }

        state.tick = 19;
        observation.tick = 19;
        state
            .body_collisions
            .push(scenario_spacewars::BodyCollision {
                ship: PlayerId::PLAYER_2.index(),
                body: BodyId::Planet(planet.index()),
            });
        telemetry.avoided_body = Some(AvoidanceBody::Planet(planet));
        telemetry.avoidance_escape_assist = true;
        history.record(&state, &observation, telemetry, ShipIntent::default());

        // Contact manifolds flicker every other tick during one continuous
        // scrape. That must remain the original incident instead of replacing
        // the completed capture and discarding its lead-in.
        for tick in 20..1_500 {
            state.tick = tick;
            observation.tick = tick;
            state.body_collisions.clear();
            if tick % 2 == 1 {
                state
                    .body_collisions
                    .push(scenario_spacewars::BodyCollision {
                        ship: PlayerId::PLAYER_2.index(),
                        body: BodyId::Planet(planet.index()),
                    });
            }
            history.record(&state, &observation, telemetry, ShipIntent::default());
        }

        let encounter = history.encounter.as_ref().unwrap();
        assert_eq!(encounter.trigger_tick, 19);
        assert_eq!(encounter.samples.first().unwrap().tick, 0);
        assert!(
            encounter.samples.iter().any(
                |sample| sample.tick == 19 && sample.events & FLIGHT_EVENT_CONTACT_CHANGED != 0
            )
        );
        assert!(!encounter.collecting);
        assert_eq!(encounter.samples.len(), FLIGHT_ENCOUNTER_CAPACITY);
        assert_eq!(history.recent.len(), FLIGHT_RECENT_CAPACITY);
        assert!(history.recent.front().unwrap().tick > 19);

        let mut input = ClientInput::default();
        input.spacewars_controls.flight_histories[1] = history;
        assert!(!input.runtime_diagnostics_text().contains("flight_history"));
        let paused = input.paused_runtime_diagnostics_text();
        assert!(paused.contains("flight_history player=2"));
        assert!(paused.contains("contact_rearm_ticks=30"));
        assert!(paused.contains("episode seed=23"));
        assert!(paused.contains("trigger=body_contact:Planet(0) trigger_tick=19"));
        assert!(paused.contains("events=avoidance|contact|assist"));
        assert!(paused.contains("contact_incident=Some(Planet(0))"));
        assert!(paused.contains("impact_time="));
    }

    #[test]
    fn accepted_spaceport_contact_does_not_start_a_crash_capture() {
        let mut state = SpacewarsScenario::init(SpacewarsConfig::default(), 29);
        let mut observation = SpacewarsScenario::observe_ship(
            &state,
            PlayerId::PLAYER_2,
            ShipSensorProfile::FullMapRadar,
        )
        .unwrap();
        let planet = observation.planets[0].id;
        observation.own_ship.docked_planet = Some(planet);
        state
            .body_collisions
            .push(scenario_spacewars::BodyCollision {
                ship: PlayerId::PLAYER_2.index(),
                body: BodyId::Planet(planet.index()),
            });

        let mut history = BotFlightHistory::default();
        history.record(
            &state,
            &observation,
            BrainTelemetry::default(),
            ShipIntent::default(),
        );

        assert!(history.encounter.is_none());
        assert_eq!(
            history.recent.back().unwrap().contact,
            Some(BodyId::Planet(planet.index()))
        );
    }
}
