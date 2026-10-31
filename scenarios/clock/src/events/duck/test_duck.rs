//! Unit tests drive a visit without a `ClockState`, so they keep its arena
//! beside it. The `arena` field and the forwarding methods below let those
//! tests read as though the visit still owned its world, which is how most of
//! them were written; `DuckEvent` itself no longer does.
use std::ops::{Deref, DerefMut};

use engine_common::ClockEventKind;

use super::*;

/// The event whose lease keeps the arena while a test's character is away.
const LEASE: ClockEventKind = ClockEventKind::Falling;

pub(crate) struct TestDuck {
    pub duck: DuckEvent,
    pub arena: ClockArena,
}

impl From<DuckEvent> for TestDuck {
    fn from(duck: DuckEvent) -> Self {
        Self {
            duck,
            arena: ClockArena::default(),
        }
    }
}

impl TestDuck {
    pub fn new(layout: Layout, seed: u64) -> Self {
        DuckEvent::new(layout, seed).into()
    }

    pub fn new_platforms(layout: Layout, seed: u64) -> Self {
        DuckEvent::new_platforms(layout, seed).into()
    }

    pub fn new_course(
        layout: Layout,
        seed: u64,
        pattern: Option<engine_common::ClockDuckCoursePattern>,
    ) -> Self {
        DuckEvent::new_course(layout, seed, pattern).into()
    }

    pub fn new_player(
        layout: Layout,
        seed: u64,
        pattern: Option<engine_common::ClockDuckCoursePattern>,
        session_id: u64,
        seat: u8,
    ) -> Self {
        DuckEvent::new_player(layout, seed, pattern, session_id, seat).into()
    }

    pub fn new_responsive_player(
        layout: Layout,
        seed: u64,
        session_id: u64,
        seat: u8,
        facing: f32,
        floor: ResponsiveFloor,
    ) -> Self {
        DuckEvent::new_responsive_player(layout, seed, session_id, seat, facing, floor).into()
    }

    /// Hand the visit and its arena to `state`, as though it had been admitted
    /// there and stepped this far.
    pub fn admit(self, state: &mut crate::ClockState) {
        state.duck_visit = Some(Box::new(self.duck));
        state.arena = self.arena;
    }

    pub fn view(&self) -> DuckView<'_> {
        DuckView::new(&self.duck, self.arena.world())
    }

    pub fn step(&mut self) -> bool {
        self.step_with_water(None)
    }

    /// One tick as `ClockState` runs it. An event with water moves the panels
    /// itself, as Rain does.
    pub fn step_with_water(&mut self, water: Option<&WaterWorld>) -> bool {
        let panels = if water.is_some() {
            PanelAuthority::Event(ClockEventKind::Rain)
        } else {
            PanelAuthority::Visit
        };
        let step = self.duck.pre_step(&mut self.arena, water, panels, None);
        self.arena.step(DT);
        self.duck.post_step(&mut self.arena, water, step)
    }

    pub fn spawn(&mut self) {
        self.duck.spawn(&mut self.arena);
    }

    pub fn position(&self) -> Option<Vec2> {
        self.view().position()
    }

    pub fn grounded(&self) -> bool {
        self.view().grounded()
    }

    pub fn feet_moving(&self) -> bool {
        self.view().feet_moving()
    }

    pub fn ledge_anchor(&self) -> Option<Vec2> {
        self.view().ledge_anchor()
    }

    pub fn diagnostics(&self) -> ClockDuckState {
        self.view().diagnostics()
    }

    pub fn player_diagnostics(&self) -> Option<engine_common::ClockPlayerDuckState> {
        self.view().player_diagnostics()
    }

    pub fn supported_surface(&self) -> Option<usize> {
        self.arena
            .world()
            .and_then(|world| self.duck.supported_surface(world))
    }

    pub fn physics_counts(&self) -> (usize, usize) {
        self.arena
            .world()
            .map_or((0, 0), |world| (world.body_count(), world.collider_count()))
    }

    pub fn take_control(&mut self, session_id: u64, seat: u8) -> bool {
        self.duck
            .take_control(self.arena.world_mut(), session_id, seat)
    }

    pub fn dismiss_player(&mut self) {
        self.duck.dismiss_player(self.arena.world_mut());
    }

    /// The soil rests on the terrain, and its body takes that shape in the
    /// arena, as before `ClockState` steps it.
    pub fn synchronize_soil(&mut self, mut soil: Option<&mut crate::grass::SoilHabitat>) {
        self.duck.terrain.synchronize_soil(soil.as_deref_mut());
        if let (Some(soil), Some(world)) = (soil, self.arena.world_mut()) {
            soil.bind_current(world);
        }
    }

    /// Leave the arena while an event's lease keeps it, checking that the
    /// character took its body and flaps with it.
    pub fn leave(self) -> ClockArena {
        let TestDuck {
            mut duck,
            mut arena,
        } = self;
        duck.claim_arena(&mut arena, LEASE);
        duck.leave_arena(&mut arena);
        let left = arena.world().unwrap();
        assert!(left.motion(DUCK_BODY).is_none(), "the body leaves");
        assert!(
            (0..2).all(|side| !left.contains_body(flaps::body(side))),
            "the flaps leave"
        );
        arena
    }

    /// Leave, then come back as a new player character on the same terrain.
    pub fn rejoin(self, session_id: u64, seat: u8) -> Self {
        let mut arena = self.leave();
        let duck = DuckEvent::rejoin_arena(&mut arena, 1.0, session_id, seat).unwrap();
        Self { duck, arena }
    }

    /// The character's part of a tick and the arena's step, without the
    /// visit's panels, flaps or phase.
    pub fn run(&mut self, water: Option<&WaterWorld>) {
        let ledge_driven = self
            .duck
            .run(self.arena.world_mut().expect("visit arena"), water);
        self.arena.step(DT);
        self.duck
            .finish_run(self.arena.world_mut().expect("visit arena"), ledge_driven);
    }

    pub fn decide_automatic_ledge(&mut self) {
        self.duck
            .decide_automatic_ledge(self.arena.world().expect("visit arena"));
    }

    pub fn drive_ledge(&mut self) -> bool {
        self.duck
            .drive_ledge(self.arena.world_mut().expect("visit arena"))
    }

    pub fn attach_to_flap(&mut self, handle: flaps::FlapHandle, local_point: Vec2) -> bool {
        self.duck.attach_to_flap(
            self.arena.world().expect("visit arena"),
            handle,
            local_point,
        )
    }

    pub fn advance_flaps(&mut self) {
        self.duck
            .advance_flaps(self.arena.world_mut().expect("visit arena"));
    }

    pub fn remove_flaps(&mut self) {
        self.duck
            .remove_flaps(self.arena.world_mut().expect("visit arena"));
    }

    pub fn flap_diagnostics(&self) -> [engine_common::ClockDuckFlapState; 2] {
        self.duck.flap_diagnostics(self.arena.world())
    }
}

impl Deref for TestDuck {
    type Target = DuckEvent;

    fn deref(&self) -> &DuckEvent {
        &self.duck
    }
}

impl DerefMut for TestDuck {
    fn deref_mut(&mut self) -> &mut DuckEvent {
        &mut self.duck
    }
}
