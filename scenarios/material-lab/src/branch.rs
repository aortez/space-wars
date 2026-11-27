//! The branch cases on ordinary steps: a wood branch joined to the side of a
//! fixed trunk holds a weight near its tip. A safe load never charges the
//! joint; an overload drains its capacity until it severs, and the branch
//! falls with the weight. In Branch gives way the joint is a rigid weld. In
//! Branch bends it is a hinge with the beam's bending stiffness, so the branch
//! sags under its load and springs back when the load is lifted.
use crate::{BRANCH_CHECKPOINT_STEP, Case, Load, MaterialLabState, Phase, scene};
use engine_core::Vec2;
use engine_material::{Beam, Connection, FailureMode, Material, WOOD};
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
/// The bending branch's step: a displayed update shows a quarter of its
/// time, so it swings slowly enough to follow.
pub const BEND_DT: f32 = 1.0 / 240.0;
/// The bending branch pauses here, after 2 s of its first swings under its
/// own weight.
pub const BEND_CHECKPOINT_STEP: u32 = 480;
/// The bending branch's observation window: a minute of its time.
pub const BEND_WINDOW: u32 = 14400;
/// The bending run ends this many steps after its hinge severs. The branch
/// is already swinging down fast, so it falls past the trunk's foot, into
/// the controls drawn below, on the tenth.
pub const BEND_FALL_STEPS: u32 = 9;
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

/// The weld's or hinge's identity, which the branch holds.
pub fn weld_id() -> JointId {
    JointId::new(scene::body(BRANCH).entity, JointRole::new(1))
}

/// What fastens the weight to the bending branch: far stronger than any load
/// the case puts on it, so it never charges.
const FASTENING: Material = Material {
    name: "fastening",
    density: 1.0,
    stiffness: 1.0e9,
    strength: 1.0e9,
    toughness: 1.0e9,
    damping: 0.0,
    mode: FailureMode::Ductile,
};

/// The fastening's identity, which the weight holds.
fn fastening_id() -> JointId {
    JointId::new(scene::body(WEIGHT).entity, JointRole::new(1))
}

/// The weld's beam: the branch's height and length, with a represented depth
/// that sets wood's safe moment at 32000, twice the moment of the branch's own
/// weight.
pub fn beam() -> Beam {
    Beam::new(256.0, 2.0 * BRANCH_HALF.y, 2.0 * BRANCH_HALF.x)
}

/// The undamaged weld.
pub fn weld() -> Connection {
    Connection::new(WOOD, beam()).unwrap()
}

/// The hinge's beam: the weld's, with a represented depth of 320 that sets
/// wood's safe moment at 40000, two and a half times the moment of the
/// branch's own weight. Released level, a branch swings down to nearly twice
/// the bend it comes to rest at, so the shallower weld's beam would swing to
/// the edge of its safe moment.
fn bending_beam() -> Beam {
    Beam::new(320.0, 2.0 * BRANCH_HALF.y, 2.0 * BRANCH_HALF.x)
}

/// The undamaged hinge.
pub fn hinge() -> Connection {
    Connection::new(WOOD, bending_beam()).unwrap()
}

/// The gravity scale that gives the weight `load`.
fn scale(load: Load) -> f32 {
    load.weight() / (WEIGHT_MASS * GRAVITY)
}

/// The trunk, the branch welded or hinged to it and the weight on the branch.
/// The welded branch starts under the light load. The bending branch starts
/// with its load released, so that only its own weight swings it down from
/// level, and its weight is fastened on, so that it rides each swing rather
/// than being thrown off as the branch springs back.
pub(crate) fn build(bends: bool) -> PhysicsWorld {
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
    let load = if bends { Load::Released } else { Load::Light };
    world.set_gravity_scale(scene::body(WEIGHT), scale(load), false);
    if bends {
        world
            .insert_connection(
                fastening_id(),
                ConnectionSpec {
                    root_body: scene::body(BRANCH),
                    body: scene::body(WEIGHT),
                    root: Vec2::new(WEIGHT_REACH, BRANCH_HALF.y),
                    axis: Vec2::Y,
                    connection: Connection::new(
                        FASTENING,
                        Beam::new(1.0, 2.0 * WEIGHT_HALF.x, WEIGHT_HALF.y),
                    )
                    .unwrap(),
                    bends: false,
                },
            )
            .unwrap();
    }
    world
        .insert_connection(
            weld_id(),
            ConnectionSpec {
                root_body: scene::body(TRUNK),
                body: scene::body(BRANCH),
                root: Vec2::ZERO,
                axis: Vec2::X,
                connection: if bends { hinge() } else { weld() },
                bends,
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

/// The weld or hinge and the two bodies it concerns, read from the physical
/// state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Branch {
    /// The weight's load on the branch.
    pub tip_load: f32,
    /// The load the joint carried in the last step, or in the step that
    /// severed it.
    pub load: ConnectionLoadSample,
    /// How far the branch has turned at its root since it was joined, in
    /// radians, counterclockwise. A weld's stays near zero. None once the
    /// joint has severed.
    pub deflection: Option<f32>,
    pub safe_moment: f32,
    pub capacity: f64,
    pub remaining: f64,
    /// The capacity the joint has spent.
    pub spent: f64,
    /// Time until the last step's charge exhausts the joint, if it continues.
    pub time_to_failure: Option<f64>,
    /// The step that severed the joint.
    pub severed_at: Option<u32>,
    /// Work the severing step asked beyond the capacity left.
    pub excess: Option<f32>,
    /// How far the branch's center has fallen below where it was joined.
    pub drop: f32,
    pub branch: Placement,
    pub weight: Placement,
}

impl MaterialLabState {
    /// One ordinary step. The joint charges itself, and a severing pauses the
    /// lab before the branch falls.
    pub(crate) fn advance_branch(&mut self) {
        let (checkpoint, fall) = if self.case == Case::BranchBends {
            (BEND_CHECKPOINT_STEP, BEND_FALL_STEPS)
        } else {
            (BRANCH_CHECKPOINT_STEP, FALL_STEPS)
        };
        self.world.step(self.case.dt());
        self.steps += 1;
        if self.world.severed_connections().contains(&weld_id()) {
            self.severed_at = Some(self.steps);
            self.record(self.elapsed(), "Severed");
            self.paused = true;
        }
        if self.steps == checkpoint {
            self.paused = true;
        }
        let window = self.case.parameters().2;
        let end = self.severed_at.map_or(window, |at| (at + fall).min(window));
        if self.steps == end {
            self.stopped = Some("Run complete — restart or choose another case".into());
            self.paused = true;
        }
    }
    /// Change the weight's gravity scale. A severed joint takes no more load.
    pub(crate) fn set_branch_load(&mut self, load: Load) -> Result<(), String> {
        if self.severed_at.is_some() {
            return Err(format!(
                "The {} has severed. Restart the case to load the branch again.",
                self.joint_name()
            ));
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
    /// What joins the branch to the trunk.
    fn joint_name(&self) -> &'static str {
        if self.case == Case::BranchBends {
            "hinge"
        } else {
            "weld"
        }
    }
    pub(crate) fn branch_guidance(&self) -> &'static str {
        if self.case == Case::BranchBends {
            return if self.severed_at.is_some() {
                "The hinge has severed. Run to watch the branch fall."
            } else if self.weight() == Load::Heavy.weight() {
                "Run until the hinge gives way; the lab will pause."
            } else if self.steps < BEND_CHECKPOINT_STEP {
                "Run to watch the branch sag and swing; the lab will pause."
            } else if self.weight() == 0.0 {
                "Apply a load, then Run; the heavy load breaks the hinge."
            } else {
                "Release load to let it spring back, or apply heavy load."
            };
        }
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
    /// The joint's state and both moving bodies, for the branch cases only.
    pub fn branch(&self) -> Option<Branch> {
        if !self.case.is_branch() {
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
            deflection: state.deflection,
            safe_moment: region.material().strength * state.connection.beam().section_modulus(),
            capacity: f64::from(region.capacity()),
            remaining,
            spent: f64::from(region.capacity()) - remaining,
            time_to_failure: (state.failure.is_none() && state.load.charged > 0.0)
                .then(|| remaining / f64::from(state.load.charged) * f64::from(self.case.dt())),
            severed_at: self.severed_at,
            excess: state.failure.map(|f| f.excess),
            drop: -branch.y,
            branch,
            weight: placement(WEIGHT),
        })
    }
}
