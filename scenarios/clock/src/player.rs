//! Player presence is separate from the automatic event scheduler. For now it
//! owns a Duck course, a joined drain or shared panels while compatible events continue.
//! Falling and Meltdown lease that world without owning the player's life.
use super::*;
use events::duck::DuckEvent;

/// A complete setpoint, guarded against another controller or an old visit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockDuckInput {
    pub session_id: u64,
    pub player: u8,
    /// Screen-relative horizontal intent in -1000..=1000.
    pub move_milli: i16,
    pub jump: bool,
}

impl ClockDuckInput {
    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let input = Self {
            player: payload[0],
            session_id: u64::from_le_bytes(payload[1..9].try_into().ok()?),
            move_milli: i16::from_le_bytes(payload[9..11].try_into().ok()?),
            jump: payload[11] == 1,
        };
        ((1..=2).contains(&input.player)
            && (-1000..=1000).contains(&input.move_milli)
            && payload[11] <= 1)
            .then_some(input)
    }
}

impl ClockAction {
    pub fn toggle_player_duck(player: u8) -> Action {
        let mut payload = CLOCK_ACTION_VERSION.to_le_bytes().to_vec();
        payload.push(player);
        Action::scenario(CLOCK_ACTION_TOGGLE_PLAYER_DUCK, payload)
    }

    pub fn player_duck_input(input: ClockDuckInput) -> Action {
        let mut payload = CLOCK_ACTION_VERSION.to_le_bytes().to_vec();
        payload.push(input.player);
        payload.extend_from_slice(&input.session_id.to_le_bytes());
        payload.extend_from_slice(&input.move_milli.to_le_bytes());
        payload.push(u8::from(input.jump));
        Action::scenario(CLOCK_ACTION_PLAYER_DUCK_INPUT, payload)
    }
}

impl ClockState {
    pub fn player_duck_session(&self) -> Option<(u64, u8)> {
        self.player_duck.as_ref()?.player_session()
    }

    pub fn player_duck_state(&self) -> Option<engine_common::ClockPlayerDuckState> {
        self.player_duck.as_ref()?.player_diagnostics()
    }

    pub fn automatic_events_suspended(&self) -> bool {
        self.player_duck.is_some()
            && !ClockEventKind::ALL
                .into_iter()
                .any(|kind| self.config.events.enabled(kind) && !self.event_blocked_by_player(kind))
    }

    pub fn event_blocked_by_player(&self, kind: ClockEventKind) -> bool {
        self.player_duck.is_some()
            && (kind == ClockEventKind::Duck
                || (kind == ClockEventKind::Meltdown
                    && self.config.water_lab != ClockWaterLab::Off))
    }

    pub(super) fn sync_event_schedule(&mut self) {
        // Filter the runtime schedule, never the user's saved preferences.
        let enabled =
            |kind| self.config.events.enabled(kind) && !self.event_blocked_by_player(kind);
        let effective = ClockEvents {
            falling: enabled(ClockEventKind::Falling),
            color_cycle: enabled(ClockEventKind::ColorCycle),
            meltdown: enabled(ClockEventKind::Meltdown),
            duck: enabled(ClockEventKind::Duck),
            marquee: enabled(ClockEventKind::Marquee),
            digit_slide: enabled(ClockEventKind::DigitSlide),
            rain: enabled(ClockEventKind::Rain),
        };
        self.schedule
            .configure(self.config.event_profile, effective);
    }

    pub(crate) fn duck_scene(&self) -> Option<&DuckEvent> {
        self.player_duck
            .as_deref()
            .or_else(|| match self.active_event.as_ref()? {
                ActiveEvent::Duck(duck) => Some(duck.as_ref()),
                _ => None,
            })
    }

    pub(super) fn toggle_player_duck(&mut self, player: u8) {
        if self.reading.is_none() {
            return;
        }
        if let Some(duck) = &mut self.player_duck {
            if duck
                .player_session()
                .is_some_and(|(_, owner)| owner == player)
            {
                duck.dismiss_player();
            }
            return;
        }
        let layout = Layout::new(self.aspect_ratio());
        let session = self.player_duck_sequence + 1;
        let seed = self.player_seed.wrapping_add(session);
        if let Some(ActiveEvent::Duck(duck)) = &mut self.active_event
            && duck.take_control(session, player)
        {
            let Some(ActiveEvent::Duck(duck)) = self.active_event.take() else {
                unreachable!("the automatic visit was just taken over");
            };
            // Move the visit, not a snapshot. Retire only the automatic event's
            // scheduler/floor claim; its actor and course now belong to the player.
            self.schedule.finish(ClockEventKind::Duck);
            self.floor.release(ClockEventKind::Duck);
            self.floor.acquire_player();
            self.player_duck_sequence = session;
            self.player_duck = Some(duck);
            self.sync_event_schedule();
            self.announce_player(player);
            return;
        }
        if let Some(event) = &mut self.active_event
            && let Some(duck) = event.join_arena(layout, seed, session, player)
        {
            self.player_duck_sequence += 1;
            self.floor.acquire_player();
            self.player_duck = Some(duck);
            self.sync_event_schedule();
            return;
        }
        let is_rain = matches!(self.active_event, Some(ActiveEvent::Rain(_)));
        if !is_rain
            && self
                .event_kind()
                .is_some_and(|kind| EVENT_CATALOG[kind as usize].uses_floor())
        {
            self.finish_event();
        }
        self.player_duck_sequence += 1;
        self.floor.acquire_player();
        let duck = if let Some(ActiveEvent::Rain(rain)) = &mut self.active_event
            && let Some(floor) = rain.responsive_floor().cloned()
        {
            let motion = rain.join_player();
            DuckEvent::new_responsive_player(
                layout,
                seed,
                self.player_duck_sequence,
                player,
                rain.facing,
                floor,
                motion,
            )
        } else {
            let mut duck = DuckEvent::new_player(
                layout,
                seed,
                self.config.duck_course_pattern,
                self.player_duck_sequence,
                player,
            );
            if let Some(ActiveEvent::Rain(rain)) = &self.active_event
                && let Some(geometry) = rain.course()
            {
                duck.adopt_course(geometry);
            }
            duck
        };
        self.player_duck = Some(Box::new(duck));
        self.sync_event_schedule();
        self.announce_player(player);
    }

    fn announce_player(&mut self, player: u8) {
        self.event_notice = Some((
            if player == 1 {
                "Player 1 duck"
            } else {
                "Player 2 duck"
            },
            self.schedule.tick + 2 * u64::from(FIXED_HZ),
        ));
    }

    pub(super) fn apply_player_duck_input(&mut self, input: ClockDuckInput) {
        if let Some(duck) = &mut self.player_duck
            && duck.player_session() == Some((input.session_id, input.player))
        {
            duck.set_player_input(input.move_milli, input.jump);
        }
    }

    pub(super) fn finish_player_duck(&mut self) {
        if let Some(duck) = self.player_duck.take() {
            if let Some(event) = &mut self.active_event
                && event.shares_player_arena()
            {
                event.retain_arena(duck);
            }
            self.floor.release_player();
            self.sync_event_schedule();
        }
    }
}
