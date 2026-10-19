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

    #[cfg(test)]
    pub fn step<'a>(
        &'a mut self,
        player: Option<&'a mut DuckEvent>,
        water: Option<&WaterWorld>,
        panels_advanced: bool,
    ) -> &'a mut DuckEvent {
        self.step_with_soil(player, water, panels_advanced, None)
    }

    pub fn step_with_soil<'a>(
        &'a mut self,
        player: Option<&'a mut DuckEvent>,
        water: Option<&WaterWorld>,
        panels_advanced: bool,
        soil: Option<&mut crate::grass::SoilHabitat>,
    ) -> &'a mut DuckEvent {
        let occupied = player.is_some();
        let duck = self.get_mut(player);
        if occupied {
            duck.step_with_soil(water, panels_advanced, soil);
        } else {
            duck.step_unoccupied_arena(panels_advanced, soil);
        }
        duck
    }
}

impl DuckEvent {
    pub fn existing_world(&self) -> Option<&PhysicsWorld> {
        self.world.as_ref()
    }
    pub fn existing_world_mut(&mut self) -> Option<&mut PhysicsWorld> {
        self.world.as_mut()
    }
    pub(super) fn synchronize_soil(&mut self, soil: Option<&mut crate::grass::SoilHabitat>) {
        self.soil_supports.clear();
        self.soil_course = None;
        self.soil_navigation_owners.clear();
        if let Some(soil) = soil {
            let state = if self.responsive_floor().is_some() {
                crate::grass::SoilSupportState::Responsive
            } else if self.drain_floor.is_some() {
                crate::grass::SoilSupportState::Drain
            } else {
                crate::grass::SoilSupportState::Course
            };
            soil.set_surfaces(crate::grass::surfaces::duck(self), state);
            if let Some(world) = &mut self.world {
                soil.bind_current(world);
            }
            // Only retained material on the actual fixed course inherits course
            // identity. Drain/panel contacts and foreign bodies keep their own rules.
            if state == crate::grass::SoilSupportState::Course {
                if let Some(course) = &self.course {
                    // Split only at actual material edges. Keeping one bank
                    // at a single raised height invents support beyond a patch.
                    // Stable left-to-right order retains deterministic planning.
                    const MAX_SOIL_NAV_SURFACES: usize = 64;
                    let patches: Vec<_> = soil
                        .quads()
                        .into_iter()
                        .filter_map(|q| {
                            let min = q[0];
                            let max = q[2];
                            let a = min.x * self.direction + self.width * 0.5;
                            let b = max.x * self.direction + self.width * 0.5;
                            course
                                .surfaces
                                .iter()
                                .position(|s| {
                                    s.start <= a.min(b) + 0.01
                                        && a.max(b) <= s.end + 0.01
                                        && min.y >= self.layout.floor_y + s.height - 0.01
                                        && max.y
                                            <= self.layout.floor_y
                                                + s.height
                                                + crate::grass::SOIL_CELL_SIZE
                                                    * crate::grass::SOIL_DEPTH_CELLS as f32
                                                + 0.01
                                })
                                .map(|owner| (owner, min, max, a.min(b), a.max(b)))
                        })
                        .collect();
                    let mut navigation = course.clone();
                    navigation.surfaces.clear();
                    let mut owners = Vec::new();
                    for (owner, surface) in course.surfaces.iter().enumerate() {
                        let mut edges = vec![surface.start, surface.end];
                        for (index, _, _, a, b) in &patches {
                            if *index == owner {
                                edges.extend([*a, *b]);
                            }
                        }
                        edges.sort_by(f32::total_cmp);
                        edges.dedup_by(|a, b| (*a - *b).abs() < 0.01);
                        for pair in edges.windows(2) {
                            let x = (pair[0] + pair[1]) * 0.5;
                            let height = patches
                                .iter()
                                .filter(|(i, _, _, a, b)| *i == owner && *a <= x && x <= *b)
                                .map(|(_, _, max, _, _)| max.y - self.layout.floor_y)
                                .fold(surface.height, f32::max);
                            if let Some(previous) = navigation.surfaces.last_mut()
                                && owners.last() == Some(&owner)
                                && (previous.height - height).abs() < 0.01
                            {
                                previous.end = pair[1];
                            } else {
                                navigation.surfaces.push(planner::Surface {
                                    start: pair[0],
                                    end: pair[1],
                                    height,
                                });
                                owners.push(owner);
                            }
                        }
                    }
                    // A real sliver remains collision geometry, but is not a
                    // safe landing interval. Plan across it to a measured-size
                    // foothold instead of repeatedly selecting an impossible hop.
                    let viable: Vec<_> = navigation
                        .surfaces
                        .into_iter()
                        .zip(owners)
                        .filter(|(s, _)| s.end - s.start > self.radius * 3.0)
                        .collect();
                    let (surfaces, owners): (Vec<_>, Vec<_>) = viable.into_iter().unzip();
                    navigation.surfaces = surfaces;
                    if !navigation.surfaces.is_empty()
                        && navigation.surfaces.len() <= MAX_SOIL_NAV_SURFACES
                    {
                        for (index, s) in navigation.surfaces.iter().enumerate() {
                            for (owner, min, max, a, b) in &patches {
                                if owners[index] == *owner
                                    && s.start < *b
                                    && *a < s.end
                                    && (s.height - (max.y - self.layout.floor_y)).abs() < 0.01
                                {
                                    self.soil_supports.push((index, *min, *max));
                                }
                            }
                        }
                        self.soil_navigation_owners = owners;
                        self.soil_course = Some(navigation);
                    }
                }
            }
        }
    }
    pub fn actor_body(&self) -> Option<BodyId> {
        self.buoyant.as_ref().map(BuoyantBody::body)
    }

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
        if self.phase == EventPhase::Resetting && self.door_panels().iter().all(Option::is_none) {
            self.world = None;
        }
    }

    pub fn retire_from_arena(&mut self) {
        assert!(self.arena_claimed);
        self.reset(ClockDuckOutcome::Dismissed);
        self.remove_flaps();
        self.player = None;
    }

    pub fn rejoin_arena(&mut self, session_id: u64, seat: u8) {
        assert!(self.arena_claimed && self.player.is_none());
        self.remove_flaps();
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

    fn step_unoccupied_arena(
        &mut self,
        panels_advanced: bool,
        soil: Option<&mut crate::grass::SoilHabitat>,
    ) {
        assert!(self.arena_claimed && self.player.is_none());
        self.advance_panels(panels_advanced);
        self.synchronize_soil(soil);
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

#[cfg(test)]
mod soil_tests {
    use super::*;

    #[test]
    fn clock_grass_contract_partial_soil_maps_only_retained_supported_material() {
        for direction in [-1.0, 1.0] {
            let layout = Layout::new(800.0 / 480.0);
            let mut duck = DuckEvent::new_platforms(layout, 42);
            duck.direction = direction;
            duck.spawn();
            let mut soil = crate::grass::SoilHabitat::new(layout);
            let plan = soil.admit(&[(-duck.width * 0.5, duck.width * 0.5)], |site| {
                // An actual short bank, leaving most course surfaces bare.
                site.min.x < -duck.width * 0.5 + 20.0 || site.max.x > -duck.width * 0.5 + 80.0
            });
            assert!(!plan.sites.is_empty());
            let accounting = soil.accounting();
            duck.synchronize_soil(Some(&mut soil));
            assert!(!duck.soil_supports.is_empty());
            let course = duck.course.as_ref().unwrap();
            let navigation = duck.soil_course.as_ref().unwrap();
            for (index, surface) in navigation.surfaces.iter().enumerate() {
                let original = course.surfaces[duck.soil_navigation_owners[index]];
                let retained = duck.soil_supports.iter().any(|(i, _, _)| *i == index);
                assert_eq!(surface.height > original.height, retained);
                assert!(surface.start >= original.start && surface.end <= original.end);
            }
            assert!(
                navigation.surfaces.iter().enumerate().any(
                    |(i, s)| s.height == course.surfaces[duck.soil_navigation_owners[i]].height
                ),
                "bare ground retains its own real height"
            );
            // Settle onto actual dirt through the canonical solver.
            let (index, min, max) = duck.soil_supports[0];
            let world = duck.world.as_mut().unwrap();
            world.set_pose(
                DUCK_BODY,
                Vec2::new((min.x + max.x) * 0.5, max.y + duck.radius),
                0.0,
                true,
            );
            for _ in 0..30 {
                world.step(DT);
            }
            assert_eq!(duck.supported_surface(), Some(index));
            assert_eq!(soil.accounting(), accounting);

            // Actual support loss must clear mapping even while dirt is retained
            // and falling. Restoring a floor cannot reattach detached columns.
            soil.revalidate(Some(&[]), true);
            soil.advance_loss(std::time::Duration::from_millis(250));
            duck.synchronize_soil(Some(&mut soil));
            assert!(duck.soil_supports.is_empty());
            assert_eq!(duck.supported_surface(), None);
            duck.synchronize_soil(None);
            assert!(duck.soil_course.is_none());
        }
    }
}
