//! Interactive editable terrain fixture, independent of Spacewars services.

use std::time::Duration;

use engine_common::{
    Action, Observation, PointerPhase, RenderFrame, Scenario, StepResult, TickModel,
};
use engine_core::Vec2;
use engine_gravity::GravitySolver;
use engine_rapier::{
    spaceling::{SpacelingAssembly, SpacelingControl, SpacelingSnapshot, SpacelingSpec},
    terrain::{TerrainAssembly, TerrainSpec},
    world::{
        BodyKind, BodyMotion, BodySpec, PhysicsId, PhysicsWorld, PhysicsWorldConfig,
        RayCastOptions, RayHit,
    },
};
use engine_terrain::{
    Brush, CellCoord, ChunkId, EditMode, Material, MaterialId, Terrain, TerrainEdit,
    TerrainGeometry,
};

mod fragments;
mod impacts;
mod mining;
mod render;
mod tools;
mod view;
pub use fragments::TerrainFragment;
pub use impacts::{TerrainImpactConfig, TerrainImpactStats};
pub use mining::{
    DRILL_DAMAGE, DRILL_INTERVAL_TICKS, DRILL_RADIUS, DRILL_RANGE, MiningControls, MiningInventory,
    MiningSnapshot, MiningTarget,
};
pub use tools::{MiningTool, MiningToolProfile, ToolControls};
pub use view::TerrainView;
pub const FIXED_HZ: u32 = 60;
pub const ROCK: MaterialId = MaterialId(1);
pub const ORE: MaterialId = MaterialId(2);
const PLANET_ID: PhysicsId = PhysicsId::new(1);
const SPACELING_ID: PhysicsId = PhysicsId::new(2);

pub struct TerrainLabScenario;

#[derive(Debug, Clone, Copy)]
pub struct TerrainLabConfig {
    pub radius: f32,
    pub cell_size: f32,
    pub angular_velocity: f32,
    pub orbit_radius: f32,
    pub gravity_acceleration: f32,
    /// Profiles indexed by `MiningTool`, independent of terrain resolution.
    pub mining_tools: [MiningToolProfile; 3],
    pub impacts: TerrainImpactConfig,
}

impl Default for TerrainLabConfig {
    fn default() -> Self {
        Self {
            radius: 20.0,
            cell_size: 0.5,
            angular_velocity: 0.04,
            orbit_radius: 4.0,
            gravity_acceleration: 18.0,
            mining_tools: MiningToolProfile::DEFAULTS,
            impacts: TerrainImpactConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LabControls {
    pub walk: f32,
    pub jump: bool,
    pub crater: bool,
    pub tunnel: bool,
    pub debug: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TerrainLabAction {
    Controls(LabControls),
    Edit(TerrainEdit),
    Mining(MiningControls),
    Tools(ToolControls),
}

impl TerrainLabAction {
    pub fn controls(walk: f32, jump: bool, crater: bool, tunnel: bool, debug: bool) -> Action {
        Self::Controls(LabControls {
            walk,
            jump,
            crater,
            tunnel,
            debug,
        })
        .encode()
    }

    pub fn encode(self) -> Action {
        match self {
            Self::Controls(control) => {
                let mut payload = control.walk.to_le_bytes().to_vec();
                payload.extend(
                    [control.jump, control.crater, control.tunnel, control.debug].map(u8::from),
                );
                Action::scenario(1, payload)
            }
            Self::Edit(edit) => {
                let (tag, start, end, radius) = match edit.brush {
                    Brush::Circle { center, radius } => (0, center, center, radius),
                    Brush::Capsule { start, end, radius } => (1, start, end, radius),
                };
                let (mode, work) = match edit.mode {
                    EditMode::Remove => (0, 0),
                    EditMode::Damage(work) => (1, work),
                };
                let mut payload = vec![tag, mode, work];
                for value in [start.x, start.y, end.x, end.y] {
                    payload.extend(value.to_le_bytes());
                }
                payload.extend(radius.to_le_bytes());
                Action::scenario(2, payload)
            }
            Self::Mining(control) => {
                let mut payload = vec![1, u8::from(control.held)];
                for value in [control.turn, control.aim.x, control.aim.y] {
                    payload.extend(value.to_le_bytes());
                }
                Action::scenario(3, payload)
            }
            Self::Tools(control) => Action::scenario(
                4,
                vec![
                    1,
                    u8::from(control.cycle_tool),
                    u8::from(control.cycle_view),
                ],
            ),
        }
    }

    pub fn decode(action: &Action) -> Option<Self> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        match *kind {
            1 if payload.len() == 8 && payload[4..].iter().all(|value| *value <= 1) => {
                let walk = f32::from_le_bytes(payload[..4].try_into().ok()?);
                walk.is_finite().then_some(Self::Controls(LabControls {
                    walk: walk.clamp(-1.0, 1.0),
                    jump: payload[4] != 0,
                    crater: payload[5] != 0,
                    tunnel: payload[6] != 0,
                    debug: payload[7] != 0,
                }))
            }
            2 if payload.len() == 23 => {
                let coordinate =
                    |index| i32::from_le_bytes(payload[index..index + 4].try_into().unwrap());
                let start = CellCoord::new(coordinate(3), coordinate(7));
                let end = CellCoord::new(coordinate(11), coordinate(15));
                let radius = u32::from_le_bytes(payload[19..23].try_into().ok()?);
                let brush = match payload[0] {
                    0 if start == end => Brush::Circle {
                        center: start,
                        radius,
                    },
                    1 => Brush::Capsule { start, end, radius },
                    _ => return None,
                };
                let mode = match payload[1] {
                    0 if payload[2] == 0 => EditMode::Remove,
                    1 => EditMode::Damage(payload[2]),
                    _ => return None,
                };
                Some(Self::Edit(TerrainEdit { brush, mode }))
            }
            3 if payload.len() == 14 && payload[0] == 1 && payload[1] <= 1 => {
                let value =
                    |index| f32::from_le_bytes(payload[index..index + 4].try_into().unwrap());
                let (turn, x, y) = (value(2), value(6), value(10));
                [turn, x, y]
                    .into_iter()
                    .all(f32::is_finite)
                    .then_some(Self::Mining(MiningControls {
                        held: payload[1] != 0,
                        turn: turn.clamp(-1.0, 1.0),
                        aim: Vec2::new(x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0)),
                    }))
            }
            4 if payload.len() == 3 && payload[0] == 1 && payload[1..].iter().all(|v| *v <= 1) => {
                Some(Self::Tools(ToolControls {
                    cycle_tool: payload[1] != 0,
                    cycle_view: payload[2] != 0,
                }))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TerrainLabMetrics {
    pub changed_cells: u32,
    pub removed_cells: u32,
    pub rebuilt_chunks: usize,
    pub detached_cells: u32,
    pub spawned_fragments: usize,
    pub edit_time: Duration,
    pub connectivity_time: Duration,
    pub rebuild_time: Duration,
}

#[derive(Clone)]
pub struct TerrainLabState {
    pub config: TerrainLabConfig,
    pub tick: u64,
    pub last_edit: TerrainLabMetrics,
    pub removed_cells: u64,
    pub recovered: MiningInventory,
    pub rejected_edits: u64,
    pub overlay: bool,
    pub view: TerrainView,
    terrain: Terrain,
    geometry: TerrainGeometry,
    terrain_hash: u64,
    edited_chunks: Vec<ChunkId>,
    terrain_assembly: TerrainAssembly,
    fragments: Vec<TerrainFragment>,
    next_fragment_id: u64,
    physics: PhysicsWorld,
    spaceling: SpacelingAssembly,
    gravity_solver: GravitySolver,
    controls: LabControls,
    previous_controls: LabControls,
    tool_controls: ToolControls,
    previous_tool_controls: ToolControls,
    mining: mining::MiningState,
    pending_edits: Vec<PendingEdit>,
    impact: impacts::ImpactState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum EditCause {
    Debug,
    Mining,
    Impact,
}

#[derive(Clone, Copy)]
struct PendingEdit {
    body: PhysicsId,
    edit: TerrainEdit,
    cause: EditCause,
}

impl From<TerrainEdit> for PendingEdit {
    fn from(edit: TerrainEdit) -> Self {
        Self {
            body: PLANET_ID,
            edit,
            cause: EditCause::Debug,
        }
    }
}

impl TerrainLabState {
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }
    pub fn terrain_hash(&self) -> u64 {
        // Preserve the original content hash when no fragments exist; include
        // stable ownership and each cached field hash after separation.
        self.fragments
            .iter()
            .fold(self.terrain_hash, |hash, fragment| {
                [fragment.id.value(), fragment.hash]
                    .into_iter()
                    .fold(hash, |hash, value| {
                        value.to_le_bytes().into_iter().fold(hash, |hash, byte| {
                            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
                        })
                    })
            })
    }
    pub fn rectangle_count(&self) -> usize {
        self.geometry.rectangle_count()
            + self
                .fragments
                .iter()
                .map(|fragment| fragment.geometry.rectangle_count())
                .sum::<usize>()
    }
    pub fn collider_count(&self) -> usize {
        self.physics.collider_count()
    }
    pub fn spaceling_snapshot(&self) -> SpacelingSnapshot {
        self.spaceling
            .snapshot(&self.physics)
            .expect("lab retains character")
    }
    pub fn planet_motion(&self) -> BodyMotion {
        self.physics
            .motion(self.terrain_assembly.body())
            .expect("lab retains terrain body")
    }
    pub fn local_to_world(&self, local: Vec2) -> Vec2 {
        let motion = self.planet_motion();
        motion.position + local.rotate_radians(motion.angle)
    }
    pub fn world_to_cell(&self, world: Vec2) -> Option<CellCoord> {
        let motion = self.planet_motion();
        self.terrain
            .local_to_cell((world - motion.position).rotate_radians(-motion.angle))
    }

    /// Query the canonical world along the field's local equator.
    pub fn probe(&self) -> (Vec2, Vec2, Option<RayHit>) {
        let reach = self.config.radius + 3.0;
        let start = self.local_to_world(Vec2::new(-reach, 0.0));
        let end = self.local_to_world(Vec2::new(reach, 0.0));
        let hit = self.physics.cast_ray(
            start,
            (end - start).normalized(),
            RayCastOptions {
                max_distance: reach * 2.0,
                exclude_entity: Some(SPACELING_ID),
                ..RayCastOptions::default()
            },
        );
        (start, end, hit)
    }
}

/// Seeded coarse ore regions. Geology is scenario policy; the terrain crate
/// accepts arbitrary rectangular fields and a material generator.
pub fn generate_planet(
    config: TerrainLabConfig,
    seed: u64,
) -> Result<Terrain, engine_terrain::TerrainError> {
    if !(5.0..=150.0).contains(&config.radius) || !(0.25..=2.0).contains(&config.cell_size) {
        return Err(engine_terrain::TerrainError(
            "invalid planet radius or cell size",
        ));
    }
    let radius = (config.radius / config.cell_size).ceil() as i32;
    let radius_squared = f64::from(config.radius / config.cell_size).powi(2);
    let side = (radius * 2 + 1) as u32;
    Terrain::generate(
        side,
        side,
        config.cell_size,
        vec![
            Material {
                id: ROCK,
                hardness: 100,
            },
            Material {
                id: ORE,
                hardness: 180,
            },
        ],
        |coordinate| {
            let x = coordinate.x - radius;
            let y = coordinate.y - radius;
            if f64::from(x * x + y * y) > radius_squared {
                return MaterialId::VOID;
            }
            let mut value = seed
                ^ ((coordinate.x / 8) as u64).wrapping_mul(0x9e3779b97f4a7c15)
                ^ ((coordinate.y / 8) as u64).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            if (value ^ (value >> 31)).is_multiple_of(5) {
                ORE
            } else {
                ROCK
            }
        },
    )
}

impl Scenario for TerrainLabScenario {
    type State = TerrainLabState;
    type Config = TerrainLabConfig;

    fn init(mut config: Self::Config, seed: u64) -> Self::State {
        let defaults = TerrainLabConfig::default();
        let normalize = |value: f32, default, min, max| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                default
            }
        };
        config.radius = normalize(config.radius, defaults.radius, 5.0, 150.0);
        config.cell_size = normalize(config.cell_size, defaults.cell_size, 0.25, 2.0);
        config.angular_velocity = normalize(
            config.angular_velocity,
            defaults.angular_velocity,
            -0.5,
            0.5,
        );
        config.orbit_radius = normalize(config.orbit_radius, defaults.orbit_radius, 0.0, 10.0);
        config.gravity_acceleration = normalize(
            config.gravity_acceleration,
            defaults.gravity_acceleration,
            0.0,
            40.0,
        );
        for (profile, default) in config.mining_tools.iter_mut().zip(defaults.mining_tools) {
            *profile = profile.normalized(default);
        }
        config.impacts = config.impacts.normalized();
        let terrain = generate_planet(config, seed).expect("normalized lab field");
        let geometry = TerrainGeometry::new(&terrain);
        let terrain_hash = terrain.hash();
        let mut physics = PhysicsWorld::new(PhysicsWorldConfig {
            gravity: Vec2::ZERO,
            solver_iterations: 8,
            internal_stabilization_iterations: 2,
            max_ccd_substeps: 4,
            ..PhysicsWorldConfig::default()
        });
        let terrain_assembly = TerrainAssembly::insert(
            &mut physics,
            PLANET_ID,
            BodySpec {
                kind: BodyKind::KinematicPosition,
                can_sleep: false,
                ..BodySpec::default()
            },
            &terrain,
            &geometry,
            TerrainSpec::default(),
        )
        .expect("valid lab terrain");
        // Prime terrain ray queries before publishing the first frame. No
        // dynamic bodies exist yet, so this does not advance character motion.
        physics.step(1.0 / FIXED_HZ as f32);
        let spec = SpacelingSpec::default();
        let spaceling = SpacelingAssembly::insert(
            &mut physics,
            SPACELING_ID,
            Vec2::new(0.0, config.radius + spec.half_height() + config.cell_size),
            0.0,
            spec,
        )
        .expect("valid lab character");
        TerrainLabState {
            config,
            tick: 0,
            last_edit: TerrainLabMetrics::default(),
            removed_cells: 0,
            recovered: MiningInventory::default(),
            rejected_edits: 0,
            overlay: false,
            view: TerrainView::default(),
            terrain,
            geometry,
            terrain_hash,
            edited_chunks: Vec::new(),
            terrain_assembly,
            fragments: Vec::new(),
            next_fragment_id: 3,
            physics,
            spaceling,
            gravity_solver: GravitySolver::new(),
            controls: LabControls::default(),
            previous_controls: LabControls::default(),
            tool_controls: ToolControls::default(),
            previous_tool_controls: ToolControls::default(),
            mining: mining::MiningState::default(),
            pending_edits: Vec::new(),
            impact: impacts::ImpactState::default(),
        }
    }

    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        let dt = dt.as_secs_f32();
        if dt <= 0.0 || !dt.is_finite() {
            // The host delivers pointer cancellation while paused with zero dt.
            // Release held input without advancing physics, damage, or cooldown.
            if actions.iter().any(|action| matches!(action,
                Action::Pointer(pointer) if matches!(pointer.phase, PointerPhase::Cancel | PointerPhase::Release)
            )) {
                state.mining.pointer = None;
            }
            return StepResult::default();
        }
        for action in actions {
            match TerrainLabAction::decode(action) {
                Some(TerrainLabAction::Controls(controls)) => state.controls = controls,
                Some(TerrainLabAction::Edit(edit)) => state.pending_edits.push(edit.into()),
                Some(TerrainLabAction::Mining(controls)) => state.mining.controls = controls,
                Some(TerrainLabAction::Tools(controls)) => state.tool_controls = controls,
                None => {}
            }
            if let Action::Pointer(pointer) = action {
                if matches!(pointer.phase, PointerPhase::Cancel | PointerPhase::Release) {
                    state.mining.pointer = None;
                } else if pointer.position.x.is_finite()
                    && pointer.position.y.is_finite()
                    && (pointer.phase == PointerPhase::Press || state.mining.pointer.is_some())
                {
                    state.mining.pointer = Some(Vec2::new(pointer.position.x, pointer.position.y));
                }
            }
        }
        if state.tool_controls.cycle_tool && !state.previous_tool_controls.cycle_tool {
            state.select_tool(state.selected_tool().next());
        }
        if state.tool_controls.cycle_view && !state.previous_tool_controls.cycle_view {
            state.set_view(state.view.next());
        }
        state.previous_tool_controls = state.tool_controls;
        state.overlay = state.controls.debug;
        if state.controls.debug && state.controls.crater && !state.previous_controls.crater {
            let character = state.spaceling_snapshot();
            let inward = (state.planet_motion().position - character.motion.position).normalized();
            if let Some(center) = state.world_to_cell(character.motion.position + inward * 2.0) {
                state.pending_edits.push(
                    TerrainEdit {
                        brush: Brush::Circle {
                            center,
                            radius: (2.5 / state.config.cell_size).ceil() as u32,
                        },
                        mode: EditMode::Remove,
                    }
                    .into(),
                );
            }
        }
        if state.controls.debug && state.controls.tunnel && !state.previous_controls.tunnel {
            let y = state.terrain.height() as i32 / 2;
            state.pending_edits.push(
                TerrainEdit {
                    brush: Brush::Capsule {
                        start: CellCoord::new(-4, y),
                        end: CellCoord::new(state.terrain.width() as i32 + 4, y),
                        radius: (2.0 / state.config.cell_size).ceil() as u32,
                    },
                    mode: EditMode::Remove,
                }
                .into(),
            );
        }
        state.previous_controls = state.controls;
        state.queue_mining(dt);
        state.commit_edits();

        let elapsed = (state.tick + 1) as f32 * dt;
        let phase = elapsed * 0.15;
        let center = Vec2::new(phase.cos() - 1.0, phase.sin()) * state.config.orbit_radius;
        state.physics.set_next_kinematic_pose(
            state.terrain_assembly.body(),
            center,
            elapsed * state.config.angular_velocity,
        );
        state.physics.clear_forces();
        let mut impact_motions = state.capture_impact_motions();
        if let Some(planet) = impact_motions.get_mut(&PLANET_ID) {
            planet.set_kinematic_target(
                center,
                elapsed * state.config.angular_velocity,
                state.config.angular_velocity,
                dt,
            );
        }
        let delta = state.apply_fragment_gravity(dt);
        state.spaceling.apply_control(
            &mut state.physics,
            SpacelingControl {
                walk: state.controls.walk,
                jump_held: state.controls.jump,
            },
            delta / dt,
            dt,
        );
        state
            .physics
            .apply_velocity_delta(state.spaceling.body(), delta, true);
        state.physics.step(dt);
        state.queue_impact_damage(&impact_motions);
        state.tick += 1;
        StepResult::default()
    }

    fn observe(state: &Self::State) -> Observation {
        let mut payload = vec![6, state.selected_tool() as u8, state.view as u8];
        for value in [
            state.tick,
            state.terrain.revision(),
            state.terrain_hash(),
            state.removed_cells,
            state.rejected_edits,
            state.recovered.rock_cells,
            state.recovered.ore_cells,
        ] {
            payload.extend(value.to_le_bytes());
        }
        let character = state.spaceling_snapshot();
        for motion in [state.planet_motion(), character.motion] {
            for value in [
                motion.position.x,
                motion.position.y,
                motion.angle,
                motion.linear_velocity.x,
                motion.linear_velocity.y,
                motion.angular_velocity,
            ] {
                payload.extend(value.to_le_bytes());
            }
        }
        payload.push(u8::from(character.grounded()));
        payload.push(match character.balance {
            engine_rapier::spaceling::SpacelingBalance::Balanced => 0,
            engine_rapier::spaceling::SpacelingBalance::KnockedDown => 1,
            engine_rapier::spaceling::SpacelingBalance::Recovering => 2,
        });
        payload.push(character.get_up_result as u8);
        for value in [
            character.jumps,
            character.knockdowns,
            character.recoveries,
            character.get_up_attempts,
        ] {
            payload.extend(value.to_le_bytes());
        }
        payload.extend(character.settled_seconds.to_le_bytes());
        payload.extend(character.recovery_progress.to_le_bytes());
        payload.extend(state.next_fragment_id.to_le_bytes());
        payload.extend((state.fragments.len() as u32).to_le_bytes());
        for fragment in &state.fragments {
            for value in [
                fragment.id.value(),
                fragment.terrain.revision(),
                fragment.hash,
            ] {
                payload.extend(value.to_le_bytes());
            }
            let motion = state
                .physics
                .motion(fragment.assembly.body())
                .expect("retained fragment");
            for value in [
                motion.position.x,
                motion.position.y,
                motion.angle,
                motion.linear_velocity.x,
                motion.linear_velocity.y,
                motion.angular_velocity,
            ] {
                payload.extend(value.to_le_bytes());
            }
        }
        let impacts = state.impact_stats();
        for value in [
            impacts.hits,
            impacts.damaged_cells,
            impacts.destroyed_cells,
            impacts.budget_dropped,
            impacts.last_hits as u64,
        ] {
            payload.extend(value.to_le_bytes());
        }
        payload.extend(impacts.last_speed.to_le_bytes());
        payload.extend(impacts.last_energy.to_le_bytes());
        payload.push(impacts.last_damage);
        payload.extend((state.impact.contacts.len() as u32).to_le_bytes());
        for (&(a, b), &tick) in &state.impact.contacts {
            for value in [a.value(), b.value(), tick] {
                payload.extend(value.to_le_bytes());
            }
        }
        payload.extend((state.pending_edits.len() as u32).to_le_bytes());
        for pending in &state.pending_edits {
            payload.extend(pending.body.value().to_le_bytes());
            payload.push(pending.cause as u8);
            if let Action::Scenario { payload: edit, .. } =
                TerrainLabAction::Edit(pending.edit).encode()
            {
                payload.extend(edit);
            }
        }
        Observation { payload }
    }

    fn render_frame(state: &Self::State) -> RenderFrame {
        render::frame(state)
    }
    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: FIXED_HZ }
    }
}

#[cfg(test)]
mod tests;
