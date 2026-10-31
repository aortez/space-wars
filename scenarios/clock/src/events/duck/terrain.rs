//! The ground a visit stands on, apart from the character standing on it: the
//! frame its course is laid out in, that course or the hurdle layout, Rain's
//! panels or Falling's drain banks, and the navigation retained soil adds to
//! them. Nothing here needs a character, which is what will let an arena
//! outlive the visit that built it.
use super::*;
use crate::floor::DrainGeometry;

pub(crate) struct ArenaTerrain {
    /// The visit seed this terrain was laid out from. A character's own seeded
    /// choices draw separate streams from it.
    pub seed: u64,
    pub layout: Layout,
    pub width: f32,
    pub radius: f32,
    /// Fixed entrance-side course mirroring, not the runner's current facing.
    pub direction: f32,
    pub obstacles: [Obstacle; 3],
    pub course: Option<planner::Course>,
    // Derived from current retained soil; never changes authored arena geometry.
    pub(super) soil_course: Option<planner::Course>,
    pub(super) soil_supports: Vec<(usize, Vec2, Vec2)>,
    pub(super) soil_navigation_owners: Vec<usize>,
    // Rain/Meltdown advance the authoritative actuator while active. This small
    // snapshot drives contacts/rendering; after the wet event, it settles dry.
    pub(super) responsive_floor: Option<ResponsiveFloor>,
    // A joined Falling keeps its original two banks and side walls.
    pub(crate) drain_floor: Option<DrainGeometry>,
}

impl ArenaTerrain {
    /// The frame and hurdle layout `seed` chooses. A course, Rain's panels or
    /// Falling's banks take the hurdles' place when the visit has one.
    pub fn new(layout: Layout, seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let width = layout.bounds_max.x - layout.bounds_min.x;
        let radius = (width / 100.0).min(8.0);
        let mut hurdle = |fraction: f32| {
            let start = width * (fraction + rng.random_range(-0.015..0.015));
            Obstacle {
                start,
                end: start + width * 0.018,
                height: radius * rng.random_range(0.85..1.25),
            }
        };
        let first = hurdle(0.27);
        let last = hurdle(0.73);
        let pit_start = width * rng.random_range(0.465..0.48);
        // Every seed's layout depends on this draw order, so the direction
        // stays between the pit's start and its end.
        let direction = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
        let pit = Obstacle {
            start: pit_start,
            end: pit_start + width * rng.random_range(0.045..0.05),
            height: 0.0,
        };
        Self {
            seed,
            layout,
            width,
            radius,
            direction,
            obstacles: [first, pit, last],
            course: None,
            soil_course: None,
            soil_supports: Vec::new(),
            soil_navigation_owners: Vec::new(),
            responsive_floor: None,
            drain_floor: None,
        }
    }

    pub fn render_position(&self, position: Vec2) -> Vec2 {
        Vec2::new((position.x - self.width * 0.5) * self.direction, position.y)
    }

    pub fn responsive_floor(&self) -> Option<&ResponsiveFloor> {
        self.responsive_floor.as_ref()
    }

    pub fn sync_responsive_floor(&mut self, floor: &ResponsiveFloor) {
        assert!(self.responsive_floor.is_some());
        self.responsive_floor = Some(floor.clone());
    }

    pub fn support_state(&self) -> crate::grass::SoilSupportState {
        if self.responsive_floor.is_some() {
            crate::grass::SoilSupportState::Responsive
        } else if self.drain_floor.is_some() {
            crate::grass::SoilSupportState::Drain
        } else {
            crate::grass::SoilSupportState::Course
        }
    }

    /// What the planner sees: the authored course, split by retained soil
    /// when there is any.
    pub(super) fn navigation(&self) -> Option<&planner::Course> {
        self.soil_course.as_ref().or(self.course.as_ref())
    }

    /// The fixed bodies a character stands on: Rain's panels, the course's
    /// slabs or the hurdle layout. A drain-joined visit never needs this,
    /// because Falling's banks are already in the world it adopted.
    pub(super) fn insert_bodies(&self, world: &mut PhysicsWorld) {
        let pit = self.obstacles[1];
        let floor = self.layout.floor_y;
        let geometry = if let Some(responsive) = &self.responsive_floor {
            responsive.insert_panels(world);
            vec![]
        } else if let Some(course) = &self.course {
            course
                .surfaces
                .iter()
                .map(|surface| {
                    (
                        surface.start,
                        surface.end,
                        self.layout.bounds_min.y,
                        floor + surface.height,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            vec![
                (
                    -self.radius * 8.0,
                    pit.start,
                    self.layout.bounds_min.y,
                    floor,
                ),
                (
                    pit.end,
                    self.width + self.radius * 8.0,
                    self.layout.bounds_min.y,
                    floor,
                ),
                (
                    self.obstacles[0].start,
                    self.obstacles[0].end,
                    floor,
                    floor + self.obstacles[0].height,
                ),
                (
                    self.obstacles[2].start,
                    self.obstacles[2].end,
                    floor,
                    floor + self.obstacles[2].height,
                ),
            ]
        };
        for (index, (start, end, bottom, top)) in geometry.into_iter().enumerate() {
            let entity = if self.course.is_some() {
                canonical::course_surface(index)
            } else {
                PhysicsId::new(canonical::HURDLES.start + index as u64)
            };
            let mut collider = ColliderSpec::cuboid(
                ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                (end - start) * 0.5,
                (top - bottom) * 0.5,
            );
            collider.friction = 0.0;
            collider.restitution = 0.0;
            assert!(world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: BodyKind::Fixed,
                    position:
                        self.render_position(Vec2::new((start + end) * 0.5, (bottom + top) * 0.5)),
                    ..BodySpec::default()
                },
                &[collider],
            ));
        }
    }

    pub(super) fn panel_spawn_y(&self, x: f32) -> f32 {
        let floor = self.responsive_floor.as_ref().unwrap();
        let slope = (floor.opening * floor.shape.max_drop)
            / (floor.shape.half_width - floor.opening * floor.shape.max_gap);
        floor.shape.surface_y(f64::from(x), floor.opening) as f32
            + self.radius * (1.0 + slope * slope).sqrt() as f32
            + self.radius * 0.05
    }

    pub fn door_floor(&self, local_x: f32) -> f32 {
        self.responsive_floor
            .as_ref()
            .map_or(self.layout.floor_y, |floor| {
                floor.shape.surface_y(
                    f64::from(self.render_position(Vec2::new(local_x, 0.0)).x),
                    floor.opening,
                ) as f32
            })
    }

    pub(super) fn advance_panels(
        &mut self,
        world: Option<&mut PhysicsWorld>,
        panels: PanelAuthority,
        hull: Option<(Vec2, f64)>,
    ) {
        if let Some(floor) = &mut self.responsive_floor {
            if panels == PanelAuthority::Visit {
                floor.step_dry(f64::from(DT), hull);
            }
            if let Some(world) = world {
                floor.move_panels(world);
            }
        }
    }

    pub(super) fn synchronize_soil(&mut self, soil: Option<&mut crate::grass::SoilHabitat>) {
        self.soil_supports.clear();
        self.soil_course = None;
        self.soil_navigation_owners.clear();
        if let Some(soil) = soil {
            let state = self.support_state();
            soil.set_surfaces(crate::grass::surfaces::terrain(self), state);
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
                                    // The layer is inset, so retained material
                                    // fills the surface's own top rows: its top
                                    // is the surface line, never above it.
                                    s.start <= a.min(b) + 0.01
                                        && a.max(b) <= s.end + 0.01
                                        && min.y
                                            >= self.layout.floor_y + s.height
                                                - crate::grass::SOIL_CELL_SIZE
                                                    * crate::grass::SOIL_DEPTH_CELLS as f32
                                                - 0.01
                                        && max.y <= self.layout.floor_y + s.height + 0.01
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
                    // foothold instead of repeatedly selecting an impossible hop:
                    // `planner::plan` refuses a surface narrower than three radii
                    // as a takeoff as well as a landing, so a runner left standing
                    // on one could never plan its way off it.
                    //
                    // Dropping it does leave the planner blind to solid material,
                    // which `supported_surface` then reads as debris. An inset
                    // layer never raises a surface, so nothing reaches this; see
                    // `clock_grass_contract_navigation_never_hides_material_from_the_planner`,
                    // which fails if some later habitat mounds material again.
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
}
