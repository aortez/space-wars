//! Player-mode Meltdown uses real contacts in the one shared mechanics world.
//! Standalone Meltdown keeps its cheaper ballistic cells. Both use the same
//! release schedule, material budget, water solver and reformation lifecycle.
use super::*;
use crate::events::duck::PlayerArena;
use engine_rapier::world::{
    BodyId, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec, PhysicsId, PhysicsWorld,
};

pub(super) struct SharedMeltdown {
    pub arena: PlayerArena,
    bodies: CellBodies,
}

#[derive(Clone, Copy)]
struct CellBody {
    id: BodyId,
    released: bool,
}

struct CellBodies {
    // Parallel to event.cells; compacted together, with stable IDs after removal.
    cells: Vec<CellBody>,
    supports: std::ops::Range<u64>,
}

impl SharedMeltdown {
    pub fn new(player: &mut DuckEvent, count: usize) -> Self {
        assert!(count <= MAX_MELTDOWN_CELLS);
        let supports = if player.responsive_floor().is_some() {
            1001..1003
        } else {
            1000..1000
                + player
                    .course
                    .as_ref()
                    .expect("player course")
                    .surfaces
                    .len() as u64
        };
        let arena = PlayerArena::claim(player);
        player.arena_world_mut().reserve(count, count, 0);
        Self {
            arena,
            bodies: CellBodies {
                cells: (0..count)
                    .map(|i| CellBody {
                        id: BodyId::new(PhysicsId::new(4001 + i as u64), BodyRole::PRIMARY),
                        released: false,
                    })
                    .collect(),
                supports,
            },
        }
    }
}

impl MeltdownEvent {
    pub fn join_player(
        &mut self,
        layout: Layout,
        seed: u64,
        session: u64,
        seat: u8,
    ) -> Option<Box<DuckEvent>> {
        if self.lab || self.shared.is_some() {
            return None;
        }
        let floor = self
            .floor
            .as_ref()
            .expect("standalone Meltdown panels")
            .clone();
        let mut duck = Box::new(DuckEvent::new_responsive_player(
            layout,
            seed,
            session,
            seat,
            if seed.is_multiple_of(2) { 1.0 } else { -1.0 },
            floor,
            None,
        ));
        let mut shared = SharedMeltdown::new(&mut duck, self.cells.len());
        duck.preserve_arena_opacity(self.floor_opacity());
        // Promote only the surviving, already-released cells at their exact
        // current pose and velocity. Neither water nor either clock advances.
        // Unreleased cells retain their seeded release times; liquid is untouched.
        shared
            .bodies
            .release_due(duck.arena_world_mut(), &self.cells, layout, self.tick);
        self.shared = Some(shared);
        Some(duck)
    }

    pub fn shares_player_arena(&self) -> bool {
        self.shared.is_some()
    }

    pub fn vacant_arena(&self) -> Option<&DuckEvent> {
        self.shared.as_ref()?.arena.vacant()
    }

    pub fn arena_opacity(&self) -> f32 {
        if self.vacant_arena().is_some() {
            self.floor_opacity()
        } else {
            1.0
        }
    }

    pub fn retain_arena(&mut self, duck: Box<DuckEvent>) {
        self.shared
            .as_mut()
            .expect("shared Meltdown")
            .arena
            .retain(duck);
    }

    pub fn rejoin(&mut self, session: u64, seat: u8) -> Option<Box<DuckEvent>> {
        self.shared.as_mut()?.arena.rejoin(session, seat)
    }

    pub fn release(&mut self, player: Option<&mut DuckEvent>) {
        if let Some(shared) = &mut self.shared {
            let duck = shared.arena.get_mut(player);
            shared.bodies.remove(duck.arena_world_mut());
            duck.release_arena();
        }
    }

    pub(super) fn step_shared(
        &mut self,
        context: EventContext<'_>,
        mut player: Option<&mut DuckEvent>,
    ) -> bool {
        // Move the small lease/batch handles temporarily, not the world or its
        // allocations. This lets both material paths share reformation code.
        let mut shared = self.shared.take().expect("shared Meltdown");
        self.tick += 1;
        let hull = shared.arena.get(player.as_deref()).clearance_hull();
        if let Some(floor) = &mut self.floor {
            floor.step(&mut self.water, f64::from(DT), hull);
            shared
                .arena
                .get_mut(player.as_deref_mut())
                .sync_responsive_floor(floor);
        }
        let material = self.tick < MELTING_TICKS + DRAINING_TICKS;
        let layout = context.layout;
        if material {
            self.water.step(1.0 / 60.0).expect("fixed water step");
            shared.bodies.release_due(
                shared
                    .arena
                    .get_mut(player.as_deref_mut())
                    .arena_world_mut(),
                &self.cells,
                layout,
                self.tick,
            );
        } else {
            if self.tick == MELTING_TICKS + DRAINING_TICKS {
                shared.bodies.remove(
                    shared
                        .arena
                        .get_mut(player.as_deref_mut())
                        .arena_world_mut(),
                );
            }
            self.reform(context);
        }
        let duck = shared.arena.step(player, Some(&self.water), true);
        if material {
            self.exited_solid_area += shared.bodies.synchronize(
                duck.arena_world_mut(),
                &mut self.cells,
                &mut self.water,
                layout,
                self.cell_area,
            );
        }
        self.shared = Some(shared);
        self.tick >= MELTING_TICKS + DRAINING_TICKS + REFORMING_TICKS
    }
}

impl CellBodies {
    fn release_due(
        &mut self,
        world: &mut PhysicsWorld,
        cells: &[MeltCell],
        layout: Layout,
        tick: u64,
    ) {
        assert_eq!(cells.len(), self.cells.len());
        let gravity_scale = 400.0 / -world.gravity().y;
        for (body, cell) in self.cells.iter_mut().zip(cells) {
            if body.released || tick <= cell.release_tick {
                continue;
            }
            let half = layout.pitch * 0.4 * (cell.area_scale() as f32).sqrt();
            let mut collider = ColliderSpec::cuboid(
                ColliderId::new(body.id.entity, ColliderRole::PRIMARY, 0),
                half,
                half,
            );
            collider.friction = 0.45;
            collider.restitution = 0.1;
            assert!(world.insert_body(
                body.id,
                BodySpec {
                    position: cell.position,
                    angle: cell.angle,
                    linear_velocity: cell.velocity,
                    angular_velocity: cell.spin,
                    gravity_scale,
                    ccd_enabled: true,
                    ..BodySpec::default()
                },
                &[collider]
            ));
            body.released = true;
        }
    }

    fn synchronize(
        &mut self,
        world: &mut PhysicsWorld,
        cells: &mut Vec<MeltCell>,
        water: &mut WaterWorld,
        layout: Layout,
        cell_area: f64,
    ) -> f64 {
        let mut kept = 0;
        let mut exited = 0.0;
        for read in 0..cells.len() {
            let body = self.cells[read];
            let mut cell = cells[read];
            let mut remove = false;
            if body.released {
                let motion = world.motion(body.id).expect("owned Meltdown cell");
                cell.position = motion.position;
                cell.angle = motion.angle;
                cell.velocity = motion.linear_velocity;
                cell.spin = motion.angular_velocity;
                let extent = cell.extent(layout.pitch);
                let area = cell_area * cell.area_scale();
                let escaped = cell.position.y + extent.y < layout.bounds_min.y
                    || cell.position.x + extent.x < layout.bounds_min.x
                    || cell.position.x - extent.x > layout.bounds_max.x;
                if escaped {
                    exited += area;
                    remove = true;
                } else {
                    // No melting on the duck, other blocks, side walls, water,
                    // or a guessed floor plane across a real course/drain gap.
                    let floor_contact = world
                        .surface_contacts(ColliderId::new(body.id.entity, ColliderRole::PRIMARY, 0))
                        .any(|c| {
                            self.supports.contains(&c.collider.entity.value())
                                && c.normal.y > 0.7
                                && c.separation <= layout.pitch * 0.005
                        });
                    remove = floor_contact && material::liquefy(&cell, water, area, layout);
                }
                if remove {
                    world.remove_entity(body.id.entity);
                }
            }
            if !remove {
                cells[kept] = cell;
                self.cells[kept] = body;
                kept += 1;
            }
        }
        cells.truncate(kept);
        self.cells.truncate(kept);
        exited
    }

    fn remove(&mut self, world: &mut PhysicsWorld) {
        for body in self.cells.drain(..).filter(|b| b.released) {
            world.remove_entity(body.id.entity);
        }
    }
}
