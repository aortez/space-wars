//! Supported groups return to vacant cells at the caller's edit boundary.
use super::*;
use engine_terrain::{Cell, CellCoord, CellDeposit};
use std::collections::BTreeSet;

mod packing;
mod support;

const QUIET_SECONDS: f32 = 0.5;
const MAX_DEPOSITS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    Unsupported,
    Moving,
    Waiting,
    NoRoom,
    Obstructed,
    Budget,
}

/// Current reasons that surviving grains have not returned to terrain. These
/// counts partition the loose pool. Rejections remain visible during retry wait.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct SettlingDiagnostics {
    pub unsupported: usize,
    pub moving: usize,
    pub waiting: usize,
    pub no_room: usize,
    pub obstructed: usize,
    pub budget: usize,
}
impl SettlingDiagnostics {
    fn count(&mut self, reason: Reason) {
        *match reason {
            Reason::Unsupported => &mut self.unsupported,
            Reason::Moving => &mut self.moving,
            Reason::Waiting => &mut self.waiting,
            Reason::NoRoom => &mut self.no_room,
            Reason::Obstructed => &mut self.obstructed,
            Reason::Budget => &mut self.budget,
        } += 1;
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SettlingState {
    destination: BodyId,
    anchor: Vec2,
    seconds: f32,
    retry_seconds: f32,
    reason: Reason,
}

#[derive(Debug, Clone)]
pub struct DepositCommit {
    pub body: BodyId,
    pub retired_grains: Vec<PhysicsId>,
    pub dirty_chunks: Vec<ChunkId>,
    pub rebuilt_chunks: usize,
}

impl LooseTerrain {
    pub fn deposited_cells(&self) -> u64 {
        self.deposited_cells
    }
    pub fn settling_diagnostics(&self) -> SettlingDiagnostics {
        self.settling_diagnostics
    }

    /// Continuation state for scenario observations. Local anchors, quiet time
    /// and retry state affect future transfers even when current motion matches.
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
            write(state.reason as u64);
            for v in [
                state.anchor.x,
                state.anchor.y,
                state.seconds,
                state.retry_seconds,
            ] {
                write(u64::from(v.to_bits()));
            }
        }
        self.hash_slumping(&mut write);
        hash
    }

    /// Call once per simulation tick after a completed solve. Quiet grains can
    /// be supported through other grains, but each admitted group must have its
    /// own terrain contact. Motion is measured in that terrain body's frame.
    /// Up to 64 candidates are examined per call, oldest first; failed groups
    /// retry after another quiet interval. Prepared local placements preserve
    /// quantity/damage and validate actual added surface against all other bodies.
    pub fn settle<'a>(
        &mut self,
        world: &mut PhysicsWorld,
        destinations: impl IntoIterator<Item = TerrainBodyMut<'a>>,
        dt: f32,
    ) -> Result<Vec<DepositCommit>, TerrainError> {
        if !dt.is_finite() || dt <= 0.0 || dt > 0.25 {
            return Err(TerrainError("invalid settling timestep"));
        }
        self.settling_diagnostics = SettlingDiagnostics::default();
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
        if self.grains.iter().any(|g| !world.contains_body(g.body())) {
            return Err(TerrainError("missing loose body"));
        }
        let graph = support::ContactGraph::new(world, &self.grains, &fields);
        let mut quiet = BTreeMap::new();
        let mut ready = BTreeSet::new();
        let mut reasons = vec![Reason::Unsupported; self.len()];
        for (i, grain) in self.grains.iter().enumerate() {
            let Some(destination) = graph.destination[i] else {
                continue;
            };
            let field = &fields[&destination];
            if field.terrain.cell_size() != grain.cell_size()
                || field
                    .terrain
                    .material(grain.cell().material)
                    .is_none_or(|m| m.hardness < grain.cell().durability)
            {
                reasons[i] = Reason::NoRoom;
                continue;
            }
            let motion = world.motion(grain.body()).unwrap();
            let support = world.motion(destination).unwrap();
            let velocity = world
                .velocity_at_point(destination, motion.position)
                .unwrap();
            if motion.linear_velocity.distance_to(velocity) > grain.cell_size() * 0.2
                || (motion.angular_velocity - support.angular_velocity).abs() * grain.radius()
                    > grain.cell_size() * 0.2
            {
                reasons[i] = Reason::Moving;
                continue;
            }
            let local = (motion.position - support.position).rotate_radians(-support.angle);
            let previous = self.settling.get(&grain.id()).filter(|s| {
                s.destination == destination
                    && s.anchor.distance_to(local) <= grain.cell_size() * 0.15
            });
            let mut state = previous.copied().unwrap_or(SettlingState {
                destination,
                anchor: local,
                seconds: 0.0,
                retry_seconds: 0.0,
                reason: Reason::Waiting,
            });
            state.seconds = (state.seconds + dt).min(60.0);
            state.retry_seconds = (state.retry_seconds + dt).min(60.0);
            if state.seconds >= QUIET_SECONDS {
                ready.insert(i);
            }
            reasons[i] = state.reason;
            quiet.insert(grain.id(), state);
        }
        self.settling = quiet;
        let ready = graph.grounded(&ready);
        let mut seeds: Vec<_> = ready
            .iter()
            .copied()
            .filter(|&i| {
                graph.roots[i].contains(&graph.destination[i].unwrap())
                    && self.settling[&self.grains[i].id()].retry_seconds >= QUIET_SECONDS
            })
            .collect();
        seeds.sort_by(|&a, &b| {
            self.settling[&self.grains[b].id()]
                .retry_seconds
                .total_cmp(&self.settling[&self.grains[a].id()].retry_seconds)
                .then(self.grains[a].id().cmp(&self.grains[b].id()))
        });
        let mut attempted = BTreeSet::new();
        let mut retired = BTreeSet::new();
        let mut commits = Vec::new();
        let mut budget = MAX_DEPOSITS;
        for seed in seeds {
            if attempted.contains(&seed) {
                continue;
            }
            if budget == 0 {
                reasons[seed] = Reason::Budget;
                continue;
            }
            let group = graph.group(seed, &ready, &attempted, budget);
            budget -= group.len();
            attempted.extend(group.iter().copied());
            let body = graph.destination[seed].unwrap();
            let field = fields.get_mut(&body).unwrap();
            let grains: Vec<_> = group.iter().map(|&i| &self.grains[i]).collect();
            let plan = packing::prepare(world, field, &grains);
            let reason = match &plan {
                Ok(plan) => plan.remaining_reason,
                Err(reason) => *reason,
            };
            for &i in &group {
                let state = self.settling.get_mut(&self.grains[i].id()).unwrap();
                state.retry_seconds = 0.0;
                state.reason = reason;
                reasons[i] = reason;
            }
            let Ok(plan) = plan else { continue };
            let accepted: Vec<_> = plan.accepted.iter().map(|&i| group[i]).collect();
            let grains: Vec<_> = accepted.iter().map(|&i| &self.grains[i]).collect();
            let mut momentum = Momentum::default();
            momentum.add(world, body);
            for grain in &grains {
                momentum.add(world, grain.body());
            }
            *field.terrain = plan.terrain;
            *field.geometry = plan.geometry;
            let rebuilt_chunks = field
                .assembly
                .synchronize(world, field.terrain, field.geometry)
                .expect("validated deposit geometry");
            momentum.restore(world, body);
            let retired_grains: Vec<_> = grains.iter().map(|g| g.id()).collect();
            for id in &retired_grains {
                world.remove_entity(*id);
                self.settling.remove(id);
            }
            self.deposited_cells += retired_grains.len() as u64;
            self.disturb(body, field.terrain, plan.changed);
            retired.extend(accepted);
            commits.push(DepositCommit {
                body,
                retired_grains,
                dirty_chunks: plan.dirty_chunks,
                rebuilt_chunks,
            });
        }
        if budget == 0 {
            for i in ready.difference(&attempted) {
                reasons[*i] = Reason::Budget;
            }
        }
        for (i, reason) in reasons.into_iter().enumerate() {
            if !retired.contains(&i) {
                self.settling_diagnostics.count(reason);
            }
        }
        let mut i = 0;
        self.grains.retain(|_| {
            let keep = !retired.contains(&i);
            i += 1;
            keep
        });
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
