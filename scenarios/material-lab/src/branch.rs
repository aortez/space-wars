//! The branch case on ordinary steps: a wood branch welded to the side of a
//! fixed trunk holds a weight near its tip. A safe load never charges the
//! weld; an overload drains its capacity until it severs, and the branch falls
//! with the weight.
use crate::{BRANCH_CHECKPOINT_STEP, Case, Load, MaterialLabState, PHYSICS_DT, Phase, scene};
use engine_core::Vec2;
use engine_material::{Beam, Connection, WOOD};
use engine_rapier::world::{
    BodyKind, BodySpec, ColliderMassProperties, ColliderSpec, ConnectionLoadSample, ConnectionSpec,
    JointId, JointRole, PhysicsWorld, PhysicsWorldConfig,
};
use serde::Serialize;

/// Heavy load is the weight's own weight under this gravity.
pub const GRAVITY: f32 = 400.0;
pub const BRANCH_MASS: f32 = 20.0;
/// The load controls set the weight's gravity scale, never its mass.
pub const WEIGHT_MASS: f32 = 20.0;
/// The run ends this many steps after the weld severs, or at the case's
/// observation window.
pub const FALL_STEPS: u32 = 384;
/// Body roles: the trunk, the branch and the weight.
pub(crate) const TRUNK: u64 = 0;
pub(crate) const BRANCH: u64 = 1;
pub(crate) const WEIGHT: u64 = 2;
/// The branch reaches four along +x from its root at the origin, on the
/// trunk's face, and is half a unit high.
pub const BRANCH_HALF: Vec2 = Vec2::new(2.0, 0.25);
/// The trunk reaches 1.5 above the root and 2.5 below, past where the branch
/// falls to.
pub const TRUNK_HALF: Vec2 = Vec2::new(0.5, 2.0);
pub const TRUNK_CENTER: Vec2 = Vec2::new(-0.5, -0.5);
/// The weight rests on the branch's top face with its center 3.5 out.
pub const WEIGHT_HALF: Vec2 = Vec2::new(0.5, 0.5);
pub const WEIGHT_REACH: f32 = 3.5;

/// The weld's identity, which the branch holds.
pub fn weld_id() -> JointId {
    JointId::new(scene::body(BRANCH).entity, JointRole::new(1))
}

/// The weld's beam: the branch's height and length, with a represented depth
/// that sets wood's safe moment at 32000, twice the moment of the branch's own
/// weight.
pub fn beam() -> Beam {
    Beam::new(256.0, 2.0 * BRANCH_HALF.y, 2.0 * BRANCH_HALF.x)
}

/// The moment at which wood starts to yield in this beam.
pub fn safe_moment() -> f32 {
    WOOD.strength * beam().section_modulus()
}

/// The undamaged weld.
pub fn weld() -> Connection {
    Connection::new(WOOD, beam()).unwrap()
}

/// The gravity scale that gives the weight `load`.
fn scale(load: Load) -> f32 {
    load.weight() / (WEIGHT_MASS * GRAVITY)
}

/// The trunk, the branch welded to it and the weight on the branch, under the
/// light load.
pub(crate) fn build() -> PhysicsWorld {
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -GRAVITY),
        ..Default::default()
    });
    insert(
        &mut world,
        TRUNK,
        BodyKind::Fixed,
        TRUNK_CENTER,
        TRUNK_HALF,
        1.0,
        0.0,
    );
    insert(
        &mut world,
        BRANCH,
        BodyKind::Dynamic,
        Vec2::X * BRANCH_HALF.x,
        BRANCH_HALF,
        BRANCH_MASS,
        0.0,
    );
    // Friction keeps the weight in place on the branch.
    insert(
        &mut world,
        WEIGHT,
        BodyKind::Dynamic,
        Vec2::new(WEIGHT_REACH, BRANCH_HALF.y + WEIGHT_HALF.y),
        WEIGHT_HALF,
        WEIGHT_MASS,
        1.0,
    );
    world.set_gravity_scale(scene::body(WEIGHT), scale(Load::Light), false);
    world
        .insert_connection(
            weld_id(),
            ConnectionSpec {
                root_body: scene::body(TRUNK),
                body: scene::body(BRANCH),
                root: Vec2::ZERO,
                axis: Vec2::X,
                connection: weld(),
                bends: false,
            },
        )
        .unwrap();
    world
}

/// A box body free to turn, with its own mass and a box's inertia.
fn insert(
    world: &mut PhysicsWorld,
    role: u64,
    kind: BodyKind,
    position: Vec2,
    half: Vec2,
    mass: f32,
    friction: f32,
) {
    let mut shape = ColliderSpec::cuboid(scene::collider(role), half.x, half.y);
    shape.friction = friction;
    shape.mass_properties = Some(ColliderMassProperties {
        center: Vec2::ZERO,
        mass,
        inertia: mass * (half.x * half.x + half.y * half.y) / 3.0,
    });
    assert!(world.insert_body(
        scene::body(role),
        BodySpec {
            kind,
            position,
            can_sleep: false,
            ..Default::default()
        },
        &[shape],
    ));
}

/// Where a body is now.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub angle: f32,
}

/// The weld and the two bodies it concerns, read from the physical state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Branch {
    /// The weight's load on the branch.
    pub tip_load: f32,
    /// The load the weld carried in the last step, or in the step that
    /// severed it.
    pub load: ConnectionLoadSample,
    pub safe_moment: f32,
    pub capacity: f64,
    pub remaining: f64,
    /// The capacity the weld has spent.
    pub spent: f64,
    /// Time until the last step's charge exhausts the weld, if it continues.
    pub time_to_failure: Option<f64>,
    /// The step that severed the weld.
    pub severed_at: Option<u32>,
    /// Work the severing step asked beyond the capacity left.
    pub excess: Option<f32>,
    /// How far the branch has fallen below where it was welded.
    pub drop: f32,
    pub branch: Placement,
    pub weight: Placement,
}

impl MaterialLabState {
    /// One ordinary step. The weld charges itself, and a severing pauses the
    /// lab before the branch falls.
    pub(crate) fn advance_branch(&mut self) {
        self.world.step(PHYSICS_DT);
        self.steps += 1;
        if self.world.severed_connections().contains(&weld_id()) {
            self.severed_at = Some(self.steps);
            self.record(self.elapsed(), "Severed");
            self.paused = true;
        }
        if self.steps == BRANCH_CHECKPOINT_STEP {
            self.paused = true;
        }
        let window = Case::BranchFailure.parameters().2;
        let end = self
            .severed_at
            .map_or(window, |at| (at + FALL_STEPS).min(window));
        if self.steps == end {
            self.stopped = Some("Run complete — restart or choose another case".into());
            self.paused = true;
        }
    }
    /// Change the weight's gravity scale. A severed weld takes no more load.
    pub(crate) fn set_branch_load(&mut self, load: Load) -> Result<(), String> {
        if self.severed_at.is_some() {
            return Err("The weld has severed. Restart the case to load the branch again.".into());
        }
        if self.weight() == load.weight() {
            return Ok(());
        }
        if self.load_changes.len() == crate::MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES {
            return Err("Load-change limit reached. Restart the case for a new run.".into());
        }
        self.world
            .set_gravity_scale(scene::body(WEIGHT), scale(load), true);
        self.load_changes.push(crate::LoadChange {
            step: self.steps,
            load,
        });
        Ok(())
    }
    pub(crate) fn branch_weight(&self) -> f32 {
        WEIGHT_MASS * GRAVITY * self.world.gravity_scale(scene::body(WEIGHT)).unwrap()
    }
    pub(crate) fn branch_phase(&self) -> Phase {
        let load = self.world.connection(weld_id()).unwrap().load;
        if self.severed_at.is_some() {
            Phase::Falling
        } else if load.utilization > 1.0 {
            Phase::Overloaded
        } else {
            Phase::Holding
        }
    }
    pub(crate) fn branch_guidance(&self) -> &'static str {
        if self.severed_at.is_some() {
            "The weld has severed. Run to watch the branch fall."
        } else if self.weight() == Load::Heavy.weight() {
            "Run until the weld gives way; the lab will pause."
        } else if self.steps < BRANCH_CHECKPOINT_STEP {
            "Run to watch the weld hold its load; the lab will pause."
        } else {
            "Apply heavy load, then Run until the weld gives way."
        }
    }
    /// The weld's state and both moving bodies, for the branch case only.
    pub fn branch(&self) -> Option<Branch> {
        if self.case != Case::BranchFailure {
            return None;
        }
        let state = self.world.connection(weld_id()).unwrap();
        let region = state.connection.region();
        let remaining = region.remaining_precise();
        let placement = |role| {
            let motion = self.world.motion(scene::body(role)).unwrap();
            Placement {
                x: motion.position.x,
                y: motion.position.y,
                angle: motion.angle,
            }
        };
        let branch = placement(BRANCH);
        Some(Branch {
            tip_load: self.weight(),
            load: state.load,
            safe_moment: safe_moment(),
            capacity: f64::from(region.capacity()),
            remaining,
            spent: f64::from(region.capacity()) - remaining,
            time_to_failure: (state.failure.is_none() && state.load.charged > 0.0)
                .then(|| remaining / f64::from(state.load.charged) * f64::from(PHYSICS_DT)),
            severed_at: self.severed_at,
            excess: state.failure.map(|f| f.excess),
            drop: -branch.y,
            branch,
            weight: placement(WEIGHT),
        })
    }
}
