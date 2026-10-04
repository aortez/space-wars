//! Supported grains return to vacant cells at the caller's edit boundary.
use super::*;
use engine_terrain::{Cell, CellCoord, CellDeposit};
use std::collections::BTreeSet;

const QUIET_SECONDS: f32 = 0.5;
const MAX_DEPOSITS: usize = 32;

#[derive(Debug, Clone, Copy)]
pub(super) struct SettlingState {
    destination: BodyId,
    coordinate: CellCoord,
    anchor: Vec2,
    seconds: f32,
}

#[derive(Debug, Clone)]
pub struct DepositCommit {
    pub body: BodyId,
    pub retired_grains: Vec<PhysicsId>,
    pub dirty_chunks: Vec<ChunkId>,
    pub rebuilt_chunks: usize,
}

impl LooseTerrain {
    /// Cumulative transfers back to terrain, not a removed/material quantity.
    pub fn deposited_cells(&self) -> u64 {
        self.deposited_cells
    }

    /// Continuation state for scenario observations. Timers and local anchors
    /// affect future transfers even when material/motion currently match.
    pub fn settling_hash(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        let mut write = |value: u64| {
            for b in value.to_le_bytes() {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
            }
        };
        write(self.deposited_cells);
        for (id, state) in &self.settling {
            write(id.value());
            write(state.destination.entity.value());
            write(u64::from(state.destination.role.value()));
            write(state.coordinate.x as u64);
            write(state.coordinate.y as u64);
            for v in [state.anchor.x, state.anchor.y, state.seconds] {
                write(u64::from(v.to_bits()));
            }
        }
        hash
    }

    /// Call once per simulation tick, after a completed solve and before the
    /// next solve. Contacts must still belong to the supplied terrain bodies.
    /// Quiet time is measured relative to the supporting body's point velocity
    /// and rotation, never world speed or a fixed gravity direction. Only direct
    /// terrain contacts qualify; upper pile layers can follow as support packs.
    ///
    /// At most 32 cells transfer per call. Blocked placements retry after another
    /// quiet interval; unsupported/out-of-field/incompatible cells stay loose.
    /// A conservative envelope prevents growth through other solid bodies. Each
    /// destination publishes material, geometry and body retirement together.
    pub fn settle<'a>(
        &mut self,
        world: &mut PhysicsWorld,
        destinations: impl IntoIterator<Item = TerrainBodyMut<'a>>,
        dt: f32,
    ) -> Result<Vec<DepositCommit>, TerrainError> {
        if !dt.is_finite() || dt <= 0.0 || dt > 0.25 {
            return Err(TerrainError("invalid settling timestep"));
        }
        if self.is_empty() {
            return Ok(Vec::new());
        }
        let mut fields = BTreeMap::new();
        for field in destinations {
            let id = field.assembly.body();
            if !world.contains_body(id)
                || !field.geometry.is_current(field.terrain)
                || field.geometry.surface() != field.assembly.spec.surface
                || fields.insert(id, field).is_some()
            {
                return Err(TerrainError("invalid settling destination"));
            }
        }
        let mut quiet = BTreeMap::new();
        let mut ready = BTreeMap::<BodyId, Vec<(PhysicsId, CellDeposit)>>::new();
        let mut reserved = BTreeSet::new();
        let mut budget = MAX_DEPOSITS;
        // Oldest quiet candidates go first. With a large pool, repeatedly
        // blocked low IDs must not consume every admission slot forever.
        let mut grains: Vec<_> = self.grains.iter().collect();
        grains.sort_by(|a, b| {
            let age = |id| self.settling.get(&id).map_or(0.0, |s| s.seconds);
            age(b.id())
                .total_cmp(&age(a.id()))
                .then(a.id().cmp(&b.id()))
        });
        for grain in grains {
            let motion = world
                .motion(grain.body())
                .ok_or(TerrainError("missing loose body"))?;
            let size = grain.cell_size();
            let mut best: Option<(f32, BodyId, CellCoord, Vec2)> = None;
            for contact in world.surface_contacts(grain.collider()) {
                let Some(body) = world.collider_body(contact.collider) else {
                    continue;
                };
                let Some(field) = fields.get(&body) else {
                    continue;
                };
                if field.terrain.cell_size() != size
                    || field
                        .terrain
                        .material(grain.cell().material)
                        .is_none_or(|m| m.hardness < grain.cell().durability)
                    || contact.separation > size * 0.05
                {
                    continue;
                }
                let support = world.motion(body).expect("validated destination");
                let velocity = world.velocity_at_point(body, motion.position).unwrap();
                if motion.linear_velocity.distance_to(velocity) > size * 0.2
                    || (motion.angular_velocity - support.angular_velocity).abs() * grain.radius()
                        > size * 0.2
                {
                    continue;
                }
                let local = (motion.position - support.position).rotate_radians(-support.angle);
                let point = contact.local_surface.position;
                if local.distance_to(point) > size * 0.7 {
                    continue; // A moved body cannot keep an old contact alive.
                }
                let Some(owner) =
                    field
                        .geometry
                        .contact_cell(field.terrain, point, contact.local_surface.normal)
                else {
                    continue;
                };
                for (x, y) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                    let coordinate = CellCoord::new(owner.x + x, owner.y + y);
                    let center = field.terrain.cell_center(coordinate);
                    let distance = local.distance_to(center);
                    if field.terrain.cell(coordinate) != Some(Cell::VOID)
                        || distance > size * 0.8
                        || (center - point).dot(contact.local_surface.normal) <= 0.0
                    {
                        continue;
                    }
                    // Contact traversal uses solver handles; ties use stable IDs
                    // and coordinates so cache rebuild order cannot choose a cell.
                    let key = (distance, body, coordinate.y, coordinate.x);
                    if best.is_none_or(|(d, b, c, _)| key < (d, b, c.y, c.x)) {
                        best = Some((distance, body, coordinate, local));
                    }
                }
            }
            let Some((_, destination, coordinate, local)) = best else {
                continue;
            };
            let previous = self.settling.get(&grain.id()).filter(|s| {
                s.destination == destination
                    && s.coordinate == coordinate
                    && s.anchor.distance_to(local) <= size * 0.15
            });
            let mut state = previous.copied().unwrap_or(SettlingState {
                destination,
                coordinate,
                anchor: local,
                seconds: 0.0,
            });
            state.seconds = (state.seconds + dt).min(60.0);
            if state.seconds >= QUIET_SECONDS
                && budget > 0
                && reserved.insert((destination, coordinate.y, coordinate.x))
            {
                ready.entry(destination).or_default().push((
                    grain.id(),
                    CellDeposit {
                        coordinate,
                        cell: grain.cell(),
                        cell_size: size,
                    },
                ));
                state.seconds = 0.0;
                budget -= 1;
            }
            quiet.insert(grain.id(), state);
        }
        self.settling = quiet;
        let mut commits = Vec::new();
        for (body, mut deposits) in ready {
            let field = fields.get_mut(&body).unwrap();
            let motion = world.motion(body).unwrap();
            // Contour/interpolated changes can extend into neighboring cells.
            // Keep that whole band clear; densely packed grains can remain loose.
            let half = field.terrain.cell_size()
                * match field.geometry.surface() {
                    TerrainSurface::Blocks => 0.5,
                    TerrainSurface::Contour | TerrainSurface::Interpolated => 1.5,
                };
            loop {
                let excluded: Vec<_> = std::iter::once(body.entity)
                    .chain(deposits.iter().map(|(id, _)| *id))
                    .collect();
                let before = deposits.len();
                deposits.retain(|(_, deposit)| {
                    let position = motion.position
                        + field
                            .terrain
                            .cell_center(deposit.coordinate)
                            .rotate_radians(motion.angle);
                    world.current_cuboid_is_clear(
                        position,
                        motion.angle,
                        Vec2::new(half, half),
                        field.assembly.spec.collision_groups,
                        &excluded,
                    )
                });
                if deposits.len() == before {
                    break;
                }
                // A rejected grain becomes an obstacle again. Iterate to a
                // fixed point before retiring anything from this batch.
            }
            if deposits.is_empty() {
                continue;
            }
            let mut next = field.terrain.clone();
            next.deposit_cells(&deposits.iter().map(|(_, d)| *d).collect::<Vec<_>>())?;
            let mut momentum = Momentum::default();
            momentum.add(world, body);
            for (id, _) in &deposits {
                momentum.add(world, BodyId::new(*id, BodyRole::PRIMARY));
            }
            *field.terrain = next;
            let dirty_chunks = field.geometry.refresh(field.terrain);
            let rebuilt_chunks = field
                .assembly
                .synchronize(world, field.terrain, field.geometry)
                .expect("validated deposit geometry");
            momentum.restore(world, body);
            let retired_grains: Vec<_> = deposits.iter().map(|(id, _)| *id).collect();
            for id in &retired_grains {
                world.remove_entity(*id);
                self.settling.remove(id);
            }
            self.grains
                .retain(|grain| !retired_grains.contains(&grain.id()));
            self.deposited_cells += retired_grains.len() as u64;
            commits.push(DepositCommit {
                body,
                retired_grains,
                dirty_chunks,
                rebuilt_chunks,
            });
        }
        Ok(commits)
    }
}

/// Inelastic attachment preserves linear/angular momentum for a dynamic field.
/// Prescribed kinematic/fixed terrain retains the caller's motion instead.
#[derive(Default)]
struct Momentum {
    linear: [f64; 2],
    angular: f64,
}
impl Momentum {
    fn add(&mut self, world: &PhysicsWorld, body: BodyId) {
        let motion = world.motion(body).unwrap();
        let center = world.center_of_mass(body).unwrap();
        let mass = f64::from(world.body_mass(body).unwrap());
        let p = [
            mass * f64::from(motion.linear_velocity.x),
            mass * f64::from(motion.linear_velocity.y),
        ];
        self.linear[0] += p[0];
        self.linear[1] += p[1];
        self.angular += f64::from(world.dynamic_body_inertia(body).unwrap_or(0.0))
            * f64::from(motion.angular_velocity)
            + f64::from(center.x) * p[1]
            - f64::from(center.y) * p[0];
    }
    fn restore(&self, world: &mut PhysicsWorld, body: BodyId) {
        let Some(inertia) = world.dynamic_body_inertia(body) else {
            return;
        };
        let mass = f64::from(world.body_mass(body).unwrap());
        let center = world.center_of_mass(body).unwrap();
        let angular = if world.body_rotation_locked(body) == Some(true) || inertia <= 0.0 {
            0.0
        } else {
            ((self.angular - f64::from(center.x) * self.linear[1]
                + f64::from(center.y) * self.linear[0])
                / f64::from(inertia)) as f32
        };
        world.set_velocity(
            body,
            Vec2::new(
                (self.linear[0] / mass) as f32,
                (self.linear[1] / mass) as f32,
            ),
            angular,
            true,
        );
    }
}

#[cfg(test)]
mod tests;
