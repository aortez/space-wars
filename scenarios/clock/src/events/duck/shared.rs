//! A visit's physical arena can outlive its character while a timed event uses
//! it. When the character leaves, the run's `ClockArena` keeps the terrain it
//! stood on, still under the visit's claim, and a player who rejoins gets a new
//! character on that terrain. There is one world and one step in either case.
use super::*;
use engine_common::ClockEventKind;

/// The leased arena's terrain, under its character or left vacant.
pub(crate) fn leased_terrain<'a>(
    player: Option<&'a mut DuckEvent>,
    vacant: &'a mut Option<ArenaTerrain>,
) -> &'a mut ArenaTerrain {
    if let Some(player) = player {
        assert!(vacant.is_none());
        &mut player.terrain
    } else {
        vacant.as_mut().expect("one shared arena owner")
    }
}

/// Step a leased arena with its character, or without one once it has left.
/// Returns the character, if there is one.
pub(crate) fn step_leased<'a>(
    player: Option<&'a mut DuckEvent>,
    arena: &mut ClockArena,
    water: Option<&WaterWorld>,
    panels_advanced: bool,
    soil: Option<&mut crate::grass::SoilHabitat>,
) -> Option<&'a mut DuckEvent> {
    if let Some(duck) = player {
        assert!(arena.vacant.is_none());
        duck.step_with_soil(arena, water, panels_advanced, soil);
        return Some(duck);
    }
    let (terrain, world) = arena.vacant_with_world().expect("one shared arena owner");
    terrain.advance_panels(Some(world), panels_advanced, None);
    terrain.synchronize_soil(Some(world), soil);
    world.clear_forces();
    world.step(DT);
    None
}

/// `kind` ends its lease. A character still in the arena keeps the world until
/// its own reset has finished with it; a vacant arena's terrain goes now.
pub(crate) fn end_lease(
    player: Option<&mut DuckEvent>,
    arena: &mut ClockArena,
    kind: ClockEventKind,
) {
    arena.release(Owner::Event(kind));
    if let Some(player) = player {
        assert!(arena.vacant.is_none());
        player.release_arena(arena);
    } else {
        assert!(arena.vacant.take().is_some(), "one shared arena owner");
        arena.release(Owner::Visit);
    }
}

impl DuckEvent {
    pub fn actor_body(&self) -> Option<BodyId> {
        self.buoyant.as_ref().map(BuoyantBody::body)
    }

    /// `kind` leases the visit's arena, which the visit builds now if it has
    /// not stepped yet.
    pub fn claim_arena(&mut self, arena: &mut ClockArena, kind: ClockEventKind) {
        assert!(!self.arena_claimed);
        self.ensure_world(arena);
        arena.claim(Owner::Event(kind));
        self.arena_claimed = true;
    }

    /// Lets the arena go as well when the visit has already finished with it.
    pub fn release_arena(&mut self, arena: &mut ClockArena) {
        assert!(self.arena_claimed);
        self.arena_claimed = false;
        if self.phase == EventPhase::Resetting && self.door_panels().iter().all(Option::is_none) {
            arena.release(Owner::Visit);
        }
    }

    /// The character leaves an arena an event still holds. Its body and flaps
    /// go with it, and the terrain stays behind with the visit's claim.
    pub fn leave_arena(mut self, arena: &mut ClockArena) {
        assert!(self.arena_claimed && arena.vacant.is_none());
        let world = arena.world_mut().expect("leased arena");
        world.remove_entity(DUCK_ENTITY);
        self.remove_flaps(world);
        arena.vacant = Some(self.terrain);
    }

    /// A player's new character on the vacant terrain, if there is one. It
    /// takes the visit's claim back, and the arena fades in from `opacity`,
    /// the opacity the event shows it at.
    pub fn rejoin_arena(
        arena: &mut ClockArena,
        opacity: f32,
        session_id: u64,
        seat: u8,
    ) -> Option<Self> {
        let terrain = arena.vacant.take()?;
        let world = arena.world_mut().expect("leased arena");
        let mut duck = Self::with_player(Self::on_terrain(terrain), session_id, seat);
        duck.arena_claimed = true;
        duck.preserve_arena_opacity(opacity);
        if duck.terrain.course.is_some() {
            // An automatic visitor did not need this wall, but a new player
            // needs the same entrance boundary as an ordinary player spawn.
            duck.insert_entrance_wall(world);
        }
        Some(duck)
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
}

#[cfg(test)]
mod soil_tests {
    use super::*;

    #[test]
    fn clock_grass_contract_partial_soil_maps_only_retained_supported_material() {
        for direction in [-1.0, 1.0] {
            let layout = Layout::new(800.0 / 480.0);
            let mut duck = TestDuck::new_platforms(layout, 42);
            duck.face(direction);
            duck.spawn();
            let mut soil = crate::grass::SoilHabitat::new(layout);
            let plan = soil.admit(&[(-duck.width * 0.5, duck.width * 0.5)], |site| {
                // An actual short bank, leaving most course surfaces bare.
                site.min.x < -duck.width * 0.5 + 20.0 || site.max.x > -duck.width * 0.5 + 80.0
            });
            assert!(!plan.sites.is_empty());
            let accounting = soil.accounting();
            duck.synchronize_soil(Some(&mut soil));
            assert!(!duck.terrain.soil_supports.is_empty());
            let course = duck.terrain.course.as_ref().unwrap();
            let navigation = duck.terrain.soil_course.as_ref().unwrap();
            // An inset layer fills each surface's own top rows, so retained
            // material never changes the height the planner navigates, and the
            // runner meets no step where the material begins or ends.
            for (index, surface) in navigation.surfaces.iter().enumerate() {
                let original = course.surfaces[duck.terrain.soil_navigation_owners[index]];
                assert_eq!(surface.height, original.height);
                assert!(surface.start >= original.start && surface.end <= original.end);
            }
            assert!(
                duck.terrain.soil_supports.iter().all(|(index, _, max)| {
                    (max.y - (layout.floor_y + navigation.surfaces[*index].height)).abs() < 0.01
                }),
                "mapped material tops out on its own surface line"
            );
            // Settle onto actual dirt through the canonical solver.
            let (index, min, max) = duck.terrain.soil_supports[0];
            let rest = Vec2::new((min.x + max.x) * 0.5, max.y + duck.radius);
            let world = duck.arena.world_mut().unwrap();
            world.set_pose(DUCK_BODY, rest, 0.0, true);
            for _ in 0..30 {
                world.step(DT);
            }
            assert_eq!(duck.supported_surface(), Some(index));
            assert_eq!(soil.accounting(), accounting);

            // Actual support loss must clear mapping even while dirt is retained
            // and falling. Restoring a floor cannot reattach detached columns.
            // The runner keeps the real floor support it was already standing
            // on, which never depended on the inset material.
            soil.revalidate(Some(&[]), true);
            soil.advance_loss(std::time::Duration::from_millis(250));
            duck.synchronize_soil(Some(&mut soil));
            assert!(duck.terrain.soil_supports.is_empty());
            assert_eq!(duck.supported_surface(), Some(index));
            duck.synchronize_soil(None);
            assert!(duck.terrain.soil_course.is_none());
        }
    }
}

#[cfg(test)]
mod navigation_contract {
    use crate::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};
    use engine_common::{ClockEventKind, ClockEventProfile, Scenario};
    use std::time::Duration;

    fn ticks(state: &mut ClockState, count: u64) {
        for _ in 0..count {
            ClockScenario::step(state, &[], Duration::from_nanos(16_666_667));
        }
    }

    /// `synchronize_soil` drops a navigation surface narrower than three radii,
    /// because `planner::plan` refuses such a surface as a landing *and* as a
    /// takeoff, so a runner standing on one could never plan its way off.
    /// Dropping it leaves the planner blind to geometry that is still solid,
    /// and `supported_surface` then reads the material as debris.
    ///
    /// An inset layer never raises a surface, so no narrow raised segment is
    /// produced and nothing is ever dropped. This pins that: while material is
    /// retained, the navigation course covers exactly what the course covers,
    /// including after an event has opened the floor and exported columns. If
    /// some later habitat mounds material above a surface again, this fails
    /// rather than stranding a runner on a sliver the planner cannot see.
    #[test]
    fn clock_grass_contract_navigation_never_hides_material_from_the_planner() {
        let mut checked = 0u32;
        for aspect in [0.6f32, 4.0 / 3.0, 800.0 / 480.0] {
            for (seed, disturb) in [
                (1u64, None),
                (3, Some(ClockEventKind::Falling)),
                (5, Some(ClockEventKind::Explosion)),
            ] {
                let mut s = ClockScenario::init(
                    ClockConfig {
                        aspect_ratio: aspect,
                        event_profile: ClockEventProfile::Off,
                        ..Default::default()
                    },
                    seed,
                );
                ClockScenario::step(
                    &mut s,
                    &[
                        ClockAction::set_reading(ClockReading::new(10, 42, 0).unwrap()),
                        ClockAction::trigger_event(ClockEventKind::Grass),
                    ],
                    Duration::ZERO,
                );
                ticks(&mut s, 7 * 60);
                if let Some(kind) = disturb {
                    let mut waited = 0;
                    while !s.can_trigger_event() && waited < 2000 {
                        ticks(&mut s, 1);
                        waited += 1;
                    }
                    ClockScenario::step(
                        &mut s,
                        &[ClockAction::trigger_event(kind)],
                        Duration::ZERO,
                    );
                    ticks(&mut s, 300);
                }
                let mut waited = 0;
                while !s.can_trigger_event() && waited < 4000 {
                    ticks(&mut s, 1);
                    waited += 1;
                }
                ClockScenario::step(
                    &mut s,
                    &[ClockAction::trigger_event(ClockEventKind::Duck)],
                    Duration::ZERO,
                );
                for _ in 0..crate::events::duck::DUCK_TICKS {
                    ticks(&mut s, 1);
                    let Some(duck) = s.duck_scene() else { break };
                    let (Some(course), Some(navigation)) = (
                        duck.terrain.course.as_ref(),
                        duck.terrain.soil_course.as_ref(),
                    ) else {
                        continue;
                    };
                    let navigable: f32 = navigation.surfaces.iter().map(|s| s.end - s.start).sum();
                    let real: f32 = course.surfaces.iter().map(|s| s.end - s.start).sum();
                    assert!(
                        navigable >= real - 0.01,
                        "aspect={aspect} seed={seed} disturb={disturb:?}: the planner lost \
                         {:.2} units of real support",
                        real - navigable
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "the navigation course was never built");
    }
}
