//! Unit tests drive a visit without a `ClockState`, so they keep its arena
//! beside it. The `world` field and the forwarding methods below let those
//! tests read as though the visit still owned its world, which is how most of
//! them were written; `DuckEvent` itself no longer does.
use std::ops::{Deref, DerefMut};

use super::*;

pub(crate) struct TestDuck {
    pub duck: DuckEvent,
    pub world: Option<PhysicsWorld>,
}

impl From<DuckEvent> for TestDuck {
    fn from(duck: DuckEvent) -> Self {
        Self { duck, world: None }
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
        state.arena = self.world;
    }

    pub fn view(&self) -> DuckView<'_> {
        DuckView::new(&self.duck, self.world.as_ref())
    }

    pub fn step(&mut self) -> bool {
        self.step_with_water(None)
    }

    pub fn step_with_water(&mut self, water: Option<&WaterWorld>) -> bool {
        self.step_with_environment(water, water.is_some())
    }

    pub fn step_with_environment(
        &mut self,
        water: Option<&WaterWorld>,
        panels_advanced: bool,
    ) -> bool {
        self.step_with_soil(water, panels_advanced, None)
    }

    pub fn step_with_soil(
        &mut self,
        water: Option<&WaterWorld>,
        panels_advanced: bool,
        soil: Option<&mut crate::grass::SoilHabitat>,
    ) -> bool {
        self.duck
            .step_with_soil(&mut self.world, water, panels_advanced, soil)
    }

    pub fn spawn(&mut self) {
        self.duck.spawn(&mut self.world);
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
        self.world
            .as_ref()
            .and_then(|world| self.duck.supported_surface(world))
    }

    pub fn physics_counts(&self) -> (usize, usize) {
        self.world
            .as_ref()
            .map_or((0, 0), |world| (world.body_count(), world.collider_count()))
    }

    pub fn take_control(&mut self, session_id: u64, seat: u8) -> bool {
        self.duck
            .take_control(self.world.as_mut(), session_id, seat)
    }

    pub fn dismiss_player(&mut self) {
        self.duck.dismiss_player(self.world.as_mut());
    }

    pub fn synchronize_soil(&mut self, soil: Option<&mut crate::grass::SoilHabitat>) {
        self.duck
            .terrain
            .synchronize_soil(self.world.as_mut(), soil);
    }

    /// Leave the arena as an event's lease keeps it, checking that the
    /// character took its body and flaps with it.
    pub fn leave(self) -> (VisitArena, Option<PhysicsWorld>) {
        let TestDuck {
            mut duck,
            mut world,
        } = self;
        let mut lease = VisitArena::claim(&mut duck, &mut world);
        lease.retain(Box::new(duck), world.as_mut().expect("leased arena"));
        let left = world.as_ref().unwrap();
        assert!(left.motion(DUCK_BODY).is_none(), "the body leaves");
        assert!(
            (0..2).all(|side| !left.contains_body(flaps::body(side))),
            "the flaps leave"
        );
        (lease, world)
    }

    /// Leave, then come back as a new player character on the same terrain.
    pub fn rejoin(self, session_id: u64, seat: u8) -> Self {
        let (mut lease, mut world) = self.leave();
        let duck = *lease.rejoin(&mut world, 1.0, session_id, seat).unwrap();
        Self { duck, world }
    }

    pub fn run(&mut self, water: Option<&WaterWorld>) {
        self.duck
            .run(self.world.as_mut().expect("visit arena"), water);
    }

    pub fn decide_automatic_ledge(&mut self) {
        self.duck
            .decide_automatic_ledge(self.world.as_ref().expect("visit arena"));
    }

    pub fn drive_ledge(&mut self) -> bool {
        self.duck
            .drive_ledge(self.world.as_mut().expect("visit arena"))
    }

    pub fn attach_to_flap(&mut self, handle: flaps::FlapHandle, local_point: Vec2) -> bool {
        self.duck.attach_to_flap(
            self.world.as_ref().expect("visit arena"),
            handle,
            local_point,
        )
    }

    pub fn advance_flaps(&mut self) {
        self.duck
            .advance_flaps(self.world.as_mut().expect("visit arena"));
    }

    pub fn remove_flaps(&mut self) {
        self.duck
            .remove_flaps(self.world.as_mut().expect("visit arena"));
    }

    pub fn flap_diagnostics(&self) -> [engine_common::ClockDuckFlapState; 2] {
        self.duck.flap_diagnostics(self.world.as_ref())
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
