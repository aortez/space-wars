//! A visit's physical arena can outlive its character while a timed event uses
//! it. Ownership moves (never clones) to that event when the duck leaves, and
//! can move back on rejoin. There is one world and one step in either case.
use super::*;

/// A physical event leases a visit's world, retaining it if the character leaves.
/// It never creates a second world or owns the visit's controller/lifetime.
pub(crate) struct VisitArena {
    vacant: Option<Box<DuckEvent>>,
}

impl VisitArena {
    pub fn claim(player: &mut DuckEvent) -> Self {
        player.claim_arena();
        Self { vacant: None }
    }

    pub fn get<'a>(&'a self, player: Option<&'a DuckEvent>) -> &'a DuckEvent {
        player
            .or(self.vacant.as_deref())
            .expect("one shared arena owner")
    }

    pub fn get_mut<'a>(&'a mut self, player: Option<&'a mut DuckEvent>) -> &'a mut DuckEvent {
        if let Some(player) = player {
            assert!(self.vacant.is_none());
            player
        } else {
            self.vacant.as_deref_mut().expect("one shared arena owner")
        }
    }

    pub fn vacant(&self) -> Option<&DuckEvent> {
        self.vacant.as_deref()
    }

    pub fn retain(&mut self, mut duck: Box<DuckEvent>) {
        assert!(self.vacant.is_none());
        duck.retire_from_arena();
        self.vacant = Some(duck);
    }

    pub fn rejoin(&mut self, session: u64, seat: u8) -> Option<Box<DuckEvent>> {
        let mut duck = self.vacant.take()?;
        duck.rejoin_arena(session, seat);
        Some(duck)
    }

    pub fn step<'a>(
        &'a mut self,
        player: Option<&'a mut DuckEvent>,
        water: Option<&WaterWorld>,
        panels_advanced: bool,
    ) -> &'a mut DuckEvent {
        let occupied = player.is_some();
        let duck = self.get_mut(player);
        if occupied {
            duck.step_with_environment(water, panels_advanced);
        } else {
            duck.step_unoccupied_arena(panels_advanced);
        }
        duck
    }
}

impl DuckEvent {
    pub fn claim_arena(&mut self) -> &mut PhysicsWorld {
        assert!(!self.arena_claimed);
        self.ensure_world();
        self.arena_claimed = true;
        self.world.as_mut().unwrap()
    }

    pub fn arena_world(&self) -> &PhysicsWorld {
        self.world.as_ref().expect("leased arena")
    }

    pub fn arena_world_mut(&mut self) -> &mut PhysicsWorld {
        self.world.as_mut().expect("leased arena")
    }

    pub fn release_arena(&mut self) {
        assert!(self.arena_claimed);
        self.arena_claimed = false;
        if self.phase == EventPhase::Resetting {
            self.world = None;
        }
    }

    pub fn retire_from_arena(&mut self) {
        assert!(self.arena_claimed);
        self.reset(ClockDuckOutcome::Dismissed);
        self.player = None;
    }

    pub fn rejoin_arena(&mut self, session_id: u64, seat: u8) {
        assert!(self.arena_claimed && self.player.is_none());
        if self.entry_arena_opacity.is_some() {
            self.entry_arena_opacity = Some(self.arena_opacity());
        }
        self.player = Some(PlayerControl::new(
            session_id,
            seat,
            1.0,
            OPENING_TICKS + PLAYER_EXIT_DELAY_TICKS,
        ));
        if self.course.is_some() {
            // An automatic visitor did not need this wall, but a new player
            // needs the same entrance boundary as an ordinary player spawn.
            self.insert_entrance_wall();
        }
        self.tick = 0;
        self.jumps = 0;
        self.outcome = None;
        self.enter(EventPhase::Opening);
    }

    pub fn player_finished(&self) -> bool {
        self.phase == EventPhase::Resetting && self.phase_tick >= RESET_TICKS
    }

    pub fn visit_finished(&self) -> bool {
        if self.player.is_some() {
            self.player_finished()
        } else {
            self.tick >= DUCK_TICKS
        }
    }

    pub fn arena_opacity(&self) -> f32 {
        if let Some(initial) = self.entry_arena_opacity {
            let opacity =
                initial + (1.0 - initial) * (self.tick as f32 / OPENING_TICKS as f32).min(1.0);
            if self.phase == EventPhase::Resetting && !self.arena_claimed {
                opacity * self.course_opacity()
            } else {
                opacity
            }
        } else if self.arena_claimed {
            1.0
        } else {
            self.course_opacity()
        }
    }

    pub fn preserve_arena_opacity(&mut self, opacity: f32) {
        self.entry_arena_opacity = Some(opacity);
    }

    fn step_unoccupied_arena(&mut self, panels_advanced: bool) {
        assert!(self.arena_claimed && self.player.is_none());
        self.advance_panels(panels_advanced);
        let world = self.arena_world_mut();
        world.clear_forces();
        world.step(DT);
    }

    pub(super) fn advance_panels(&mut self, already_advanced: bool) {
        let hull = self.clearance_hull();
        if let Some(floor) = &mut self.responsive_floor {
            if !already_advanced {
                floor.step_dry(f64::from(DT), hull);
            }
            if let Some(world) = &mut self.world {
                floor.move_panels(world);
            }
        }
    }
}
