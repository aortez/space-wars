//! The tree bed's wood as bodies under gravity. Each wood tissue becomes a
//! segment from the tissue it grew from to its own cell, a box as it is drawn,
//! joined there by a hinge with the bending stiffness and strength of a square
//! wood beam as wide as the segment. The trunk's foot is hinged to its tree's
//! roots, which `roots` anchors in the dirt.
//! Needles and cones weigh on the wood they grow on. Snow on a branch bends it
//! further. A load that exhausts a hinge breaks the wood beyond it away: the
//! tree loses those tissues, as in a cut, and they fall to the dirt as dead
//! wood, while the snow on them slides off.
//!
//! The trees grow on while their wood is physical. Their cells stay the shape
//! the wood rests in, where growth and light work as they do at rest, and the
//! bodies and the drawing carry the sag. At each boundary, before the next
//! step, new wood is hinged where its parent now hangs and moves as it moves.
//! Wood that thickened, or whose needles and cones changed, is reshaped where
//! it hangs, and its hinge keeps the rest it was joined at. Wood a tree shed,
//! and a tree that died, stay as dead wood.
//!
//! Living wood is solid: dead wood and loose snow land on it, and it lands
//! on the dirt. Living wood passes living wood, since its segments overlap
//! where they join. Wood that dies lies in the living wood it came away
//! from, so it passes living wood until it is clear of it, as dead wood does
//! that living wood grows into. Light passes living wood's bodies, since its
//! tissues shade at their cells; dead wood shades where it lies.
use super::*;
use engine_material::{Beam, Connection, FailureMode, Material};
use std::f32::consts::FRAC_PI_2;

/// Gravity on the bed's bodies, in cells per second squared, a cell being a
/// meter.
pub const GRAVITY: f32 = 9.81;
/// The mass a needle or cone tissue adds, per size, to the wood it grows on.
/// A segment of the thinnest wood is 0.15.
pub const NEEDLE_MASS: f32 = 0.02;
pub const CONE_MASS: f32 = 0.05;
/// Air drag on the wood, its needles and its snow, per second. On wood's own
/// damping alone, the grown tree's lowest branch would swing for 46 s before
/// it settled; with the drag, it settles in 6 s.
pub const DRAG: f32 = 1.0;
/// Snow's mass. Laid on the tip of the grown tree's lowest branch toward the
/// grass, light snow doubles the branch's droop and heavy snow breaks it.
pub const LIGHT_SNOW: f32 = 1.0;
pub const HEAVY_SNOW: f32 = 5.0;
/// Snow builds up over this long, so that it bends a branch rather than
/// striking it. Landing at once, light snow would swing the lowest branch's
/// tip down to 0.81 cells, where it comes to rest at 0.60.
pub const SNOW_SECONDS: f32 = 1.0;
/// The bodies move in real time, a step for each of the lab's updates, sixty a
/// second, of at most this long.
pub const WOOD_DT: f32 = 1.0 / 60.0;
/// The most segments of dead wood the bed keeps, as many as it has tissues.
/// Were every tissue living wood as well, the world would hold about a
/// quarter of the colliders its light can measure.
pub const MAX_DEAD: usize = 256;
const SNOW_HALF: Vec2 = Vec2::new(0.45, 0.2);
const SNOW: RenderColor = RenderColor::rgb(0.88, 0.94, 1.0);
/// The wood's and snow's physics identities count up from here, past the
/// dirt's and the shade's, and are never reused.
const FIRST_ENTITY: u64 = 16;
const WOOD_GROUP: u32 = 1 << 8;
const LIVING_GROUP: u32 = 1 << 9;
const PARTING_GROUP: u32 = 1 << 10;
/// Living wood stops all but living wood, whose segments overlap where they
/// join, and wood that is parting from it. A hinge's two bodies never touch,
/// so a trunk's foot stands in the dirt it is hinged to.
const LIVING: CollisionGroups = CollisionGroups {
    memberships: LIVING_GROUP,
    filter: !(LIVING_GROUP | PARTING_GROUP),
};
/// Dead wood, and snow that slid off it, lands on the dirt and on living
/// wood, and passes through dead wood.
const DEAD_WOOD: CollisionGroups = CollisionGroups {
    memberships: WOOD_GROUP,
    filter: !(WOOD_GROUP | PARTING_GROUP),
};
/// Dead wood or loose snow that lies in living wood passes through all wood
/// until it is clear, and lands on the dirt.
const PARTING: CollisionGroups = CollisionGroups {
    memberships: PARTING_GROUP,
    filter: !(WOOD_GROUP | LIVING_GROUP | PARTING_GROUP),
};
/// Snow fastened to wood touches nothing: its fastening holds it.
const FASTENED: CollisionGroups = CollisionGroups::NONE;
/// Two boxes that overlap by less than this, in cells, count as clear of
/// each other.
const CLEAR: f32 = 0.01;
/// What fastens snow to wood: far stronger than any load it carries, so it
/// never charges.
const FASTENING: Material = Material {
    name: "fastening",
    density: 1.0,
    stiffness: 1.0e9,
    strength: 1.0e9,
    toughness: 1.0e9,
    damping: 0.0,
    mode: FailureMode::Ductile,
};

/// The bed's wood as bodies.
#[derive(Clone, Debug)]
pub struct Wood {
    /// While the wood is physical, its bodies move and the trees grow on.
    pub physical: bool,
    /// Each living wood tissue's segment.
    pub living: BTreeMap<u64, Segment>,
    /// Dead wood in the order it died: wood that broke, was cut or was shed
    /// away, and trees that died standing.
    pub dead: Vec<Dead>,
    /// At most this many segments of dead wood stay. Past it, the wood that
    /// died first is cleared away, with all that died with it.
    pub max_dead: usize,
    pub snow: Option<Snow>,
    /// Hinges that the load broke.
    pub breaks: u32,
    /// Each living tree's root plate, by its organism.
    pub plates: BTreeMap<u64, Plate>,
    /// Dead trees' root plates, which hold their dead wood.
    pub dead_plates: Vec<DeadPlate>,
    /// Roots whose anchor gave way. They grip nothing again, at rest or
    /// physical, for as long as their tree has them.
    pub pulled: BTreeSet<u64>,
    /// Anchors that the load broke, and trees it uprooted.
    pub pulls: u32,
    pub uprooted: u32,
    pub wind: Wind,
    /// What keeping the bodies in step with growth has done since the wood
    /// became physical.
    pub work: Work,
    entities: u64,
    deaths: u32,
}

impl Default for Wood {
    fn default() -> Self {
        Self {
            physical: false,
            living: BTreeMap::new(),
            dead: Vec::new(),
            max_dead: MAX_DEAD,
            snow: None,
            breaks: 0,
            plates: BTreeMap::new(),
            dead_plates: Vec::new(),
            pulled: BTreeSet::new(),
            pulls: 0,
            uprooted: 0,
            wind: Wind::default(),
            work: Work::default(),
            entities: 0,
            deaths: 0,
        }
    }
}

impl Wood {
    pub(super) fn entity(&mut self) -> PhysicsId {
        self.entities += 1;
        PhysicsId::new(FIRST_ENTITY + self.entities - 1)
    }
}

/// What keeping the bodies in step with growth has done, in segments.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Work {
    /// Built for new wood, hinged where the wood it grew on hung.
    pub grown: u32,
    /// Reshaped where they hung, as their wood thickened or their needles
    /// and cones grew, dropped or were shed.
    pub reshaped: u32,
    /// Living wood that became dead wood: shed, broken or cut away, or dead
    /// with its tree.
    pub died: u32,
    /// Dead wood cleared away to stay within the cap.
    pub cleared: u32,
    /// Hinges the engine would not reshape, which keep the beam they had,
    /// and the last of them: its wood tissue and why.
    pub refused: u32,
    pub refusal: Option<(u64, ConnectionRejection)>,
}

/// A wood tissue's body: a box from the cell it grew from to its own, as wide
/// as the wood is drawn. A capsule's rounded ends would reach past those cells,
/// and a thick trunk's below the soil.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub entity: PhysicsId,
    pub length: f32,
    pub width: f32,
    /// The living wood it was hinged to when it was built, or none for a
    /// trunk's foot, hinged to its tree's roots.
    pub parent: Option<u64>,
    /// Its wood's size, the mass of it and what grows on it in its body's
    /// frame, and the sail its needles and cones add, when it was last
    /// shaped.
    size: u16,
    mass: ColliderMassProperties,
    sail: f32,
    /// Where the body lies at rest, where the tissue's drawing lies: its
    /// center and its angle, with its y axis along the segment.
    rest: (Vec2, f32),
}

impl Segment {
    pub fn body(&self) -> BodyId {
        BodyId::new(self.entity, BodyRole::PRIMARY)
    }

    /// The hinge where it joins the wood it grew from, or the dirt.
    pub fn hinge(&self) -> JointId {
        JointId::new(self.entity, JointRole::new(1))
    }

    fn collider(&self) -> ColliderId {
        ColliderId::new(self.entity, ColliderRole::PRIMARY, 0)
    }

    /// Its box, which touches what `groups` lets it.
    fn shape(&self, groups: CollisionGroups) -> ColliderSpec {
        let mut collider =
            ColliderSpec::cuboid(self.collider(), 0.5 * self.width, 0.5 * self.length);
        collider.mass_properties = Some(self.mass);
        collider.collision_groups = groups;
        collider
    }

    fn half(&self) -> Vec2 {
        Vec2::new(0.5 * self.width, 0.5 * self.length)
    }

    /// What it turns to a wind: its width, its length, and the sail of the
    /// needles and cones on it.
    pub(super) fn sail(&self) -> (f32, f32, f32) {
        (self.width, self.length, self.sail)
    }

    /// The beam of its hinge: wood with a square section as wide as the
    /// segment, as long as it.
    fn beam(&self) -> Beam {
        Beam::new(self.width, self.width, self.length)
    }

    /// Its far end, at the tissue's cell at rest, for a body at `center`
    /// turned to `angle`.
    fn end(&self, center: Vec2, angle: f32) -> Vec2 {
        center + Vec2::new(0.0, 0.5 * self.length).rotate_radians(angle)
    }
}

/// A segment of dead wood, with the drawing it and what grew on it had, in
/// grey.
#[derive(Clone, Debug)]
pub struct Dead {
    pub segment: Segment,
    /// Whether it lies in living wood, and so passes through all wood.
    pub parting: bool,
    drawing: Vec<(i32, RenderPrimitive)>,
    /// Wood that died together shares a death, and is cleared together.
    death: u32,
}

/// Snow fastened on top of a segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snow {
    pub entity: PhysicsId,
    /// Its mass once built up.
    pub mass: f32,
    /// The wood tissue it was laid on.
    pub on: u64,
    /// Whether it still lies on that wood, or slid off as the wood died.
    pub fastened: bool,
    /// Whether, slid off, it lies in living wood, and so passes through all
    /// wood.
    pub parting: bool,
    /// The share of its weight it started from, and how long it has built up.
    from: f32,
    age: f32,
}

impl Snow {
    /// The share of its weight on the wood now.
    pub fn share(&self) -> f32 {
        (self.from + (1.0 - self.from) * self.age / SNOW_SECONDS).min(1.0)
    }

    pub fn heavy(&self) -> bool {
        self.mass == HEAVY_SNOW
    }

    pub fn body(&self) -> BodyId {
        BodyId::new(self.entity, BodyRole::PRIMARY)
    }

    fn collider(&self) -> ColliderId {
        ColliderId::new(self.entity, ColliderRole::PRIMARY, 0)
    }

    fn fastening(&self) -> JointId {
        JointId::new(self.entity, JointRole::new(1))
    }
}

/// A living wood tissue's hinge, as the last step left it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hinge {
    pub tissue: u64,
    pub cell: [i32; 2],
    /// Its last load over its safe limit.
    pub utilization: f32,
    /// How far it has turned from the rest it was joined at, in radians,
    /// counterclockwise.
    pub turn: f32,
    /// How far the wood's end hangs below its cell, in cells.
    pub droop: f32,
}

/// How a body has moved from where it lies at rest, to move a drawing with
/// it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Moved {
    from: Vec2,
    to: Vec2,
    turn: f32,
}

impl Moved {
    /// From lying unturned at `from` to lying at `to`, turned by `turn`.
    pub(super) fn between(from: Vec2, to: Vec2, turn: f32) -> Self {
        Self { from, to, turn }
    }

    pub(super) fn turn(self) -> f32 {
        self.turn
    }

    pub(super) fn apply(self, v: Vec2) -> Vec2 {
        self.to + (v - self.from).rotate_radians(self.turn)
    }

    pub(super) fn point(self, p: RenderPoint) -> RenderPoint {
        let v = self.apply(Vec2::new(p.x, p.y));
        RenderPoint::new(v.x, v.y)
    }

    pub(super) fn primitive(self, primitive: RenderPrimitive) -> RenderPrimitive {
        match primitive {
            RenderPrimitive::Circle(mut c) => {
                c.center = self.point(c.center);
                RenderPrimitive::Circle(c)
            }
            RenderPrimitive::Line(mut l) => {
                l.start = self.point(l.start);
                l.end = self.point(l.end);
                RenderPrimitive::Line(l)
            }
            RenderPrimitive::Polygon(mut p) => {
                for q in &mut p.points {
                    *q = self.point(*q);
                }
                RenderPrimitive::Polygon(p)
            }
            RenderPrimitive::Text(t) => RenderPrimitive::Text(t),
        }
    }
}

/// A box as its body lies: its center, its angle and its half extents.
#[derive(Clone, Copy, Debug)]
struct Lying {
    center: Vec2,
    angle: f32,
    half: Vec2,
}

impl Lying {
    /// Whether the boxes overlap by more than `CLEAR`: no side of either
    /// separates them.
    fn overlaps(&self, other: &Lying) -> bool {
        let between = other.center - self.center;
        [self.angle, other.angle].into_iter().all(|angle| {
            [Vec2::X, Vec2::Y].into_iter().all(|side| {
                let axis = side.rotate_radians(angle);
                between.dot(axis).abs() + CLEAR < self.reach(axis) + other.reach(axis)
            })
        })
    }

    /// How far the box reaches from its center along the unit `axis`.
    fn reach(&self, axis: Vec2) -> f32 {
        Vec2::X.rotate_radians(self.angle).dot(axis).abs() * self.half.x
            + Vec2::Y.rotate_radians(self.angle).dot(axis).abs() * self.half.y
    }
}

/// A wood tissue's segment as its cells and what grows on it shape it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shape {
    length: f32,
    width: f32,
    rest: (Vec2, f32),
    mass: ColliderMassProperties,
    sail: f32,
}

/// A tissue and every tissue that grew from it, nearest first.
pub(super) fn beyond(o: &plants::Observation, id: u64) -> Vec<u64> {
    let mut gone = vec![id];
    let mut i = 0;
    while i < gone.len() {
        let at = gone[i];
        i += 1;
        gone.extend(
            o.connections
                .iter()
                .filter(|&&(p, _)| p == at)
                .map(|&(_, child)| child),
        );
    }
    gone
}

/// What grows on each tissue, by the tissue's ID.
type Growing<'a> = BTreeMap<u64, Vec<&'a Tissue>>;

/// Every tree's wood tissue with the tissue it grew from, in ID order, so
/// that wood comes after the wood it grew on, and what grows on each tissue.
fn wood_of(o: &plants::Observation) -> (Vec<(&Tissue, &Tissue)>, Growing<'_>) {
    let tissues: BTreeMap<u64, &Tissue> = o.tissues.iter().map(|t| (t.id, t)).collect();
    let mut wood: Vec<(&Tissue, &Tissue)> = o
        .connections
        .iter()
        .map(|(p, c)| (tissues[c], tissues[p]))
        .filter(|(t, _)| t.policy == tree::POLICY && t.role == Role::Wood)
        .collect();
    wood.sort_by_key(|(t, _)| t.id);
    let mut on = Growing::new();
    for (p, c) in &o.connections {
        on.entry(*p).or_default().push(tissues[c]);
    }
    (wood, on)
}

impl TreeBed {
    pub(super) fn at(&self, cell: [i32; 2]) -> Vec2 {
        self.terrain.cell_center(CellCoord::new(cell[0], cell[1]))
    }

    /// Make the wood physical, or let it rest.
    pub(super) fn toggle_wood(&mut self) {
        if self.wood.physical {
            self.rest_wood();
        } else {
            self.build_wood();
        }
    }

    /// Every tree's roots as a plate anchored in the dirt, and its wood as
    /// segments where it now grows, each hinged where it grew from: to the
    /// wood it grew on, or for a trunk's foot to its tree's roots.
    pub(super) fn build_wood(&mut self) {
        let o = self.plants.observe();
        self.sync_plates(&o);
        let (wood, on) = wood_of(&o);
        for (t, parent) in wood {
            self.grow_segment(t, parent, on.get(&t.id).map_or(&[], Vec::as_slice));
        }
        self.wood.work = Work::default();
        self.wood.physical = true;
    }

    /// Where a wood tissue's segment lies at rest, between the tissue it grew
    /// from and its own cell, and its mass in its body's frame: a square beam
    /// of its wood, and each needle and cone on it at its own cell.
    fn shape(&self, t: &Tissue, parent: &Tissue, on: &[&Tissue]) -> Shape {
        let (from, to) = (self.at(parent.cell), self.at(t.cell));
        let axis = to - from;
        let length = axis.length();
        let width = 0.5 * f32::from(t.size).sqrt();
        let rest = ((from + to) * 0.5, axis.y.atan2(axis.x) - FRAC_PI_2);
        let own = engine_material::WOOD.density * width * width * length;
        let mut parts = vec![(
            own,
            Vec2::ZERO,
            own * (length * length + width * width) / 12.0,
        )];
        let mut sail = 0.0;
        for c in on {
            let (per_size, sail_per_size) = match c.role {
                Role::Shoot => (NEEDLE_MASS, NEEDLE_SAIL),
                Role::Cone => (CONE_MASS, CONE_SAIL),
                _ => continue,
            };
            let at = (self.at(c.cell) - rest.0).rotate_radians(-rest.1);
            parts.push((per_size * f32::from(c.size), at, 0.0));
            sail += sail_per_size * f32::from(c.size);
        }
        let mass: f32 = parts.iter().map(|p| p.0).sum();
        let center = parts.iter().fold(Vec2::ZERO, |sum, p| sum + p.1 * p.0) / mass;
        let inertia = parts
            .iter()
            .map(|&(m, at, own)| own + m * (at - center).length_squared())
            .sum();
        Shape {
            length,
            width,
            rest,
            mass: ColliderMassProperties {
                center,
                mass,
                inertia,
            },
            sail,
        }
    }

    /// A segment for wood `t` grown from `parent`, with `on` growing on it.
    /// It lies where the body that carries it, the wood it grew on or for a
    /// trunk's foot its tree's roots, has carried the cells it grew between.
    /// It moves as that body moves there, and is hinged to it at the parent's
    /// cell.
    fn grow_segment(&mut self, t: &Tissue, parent: &Tissue, on: &[&Tissue]) {
        let shape = self.shape(t, parent, on);
        let carrier = self.wood.living.get(&parent.id).copied();
        let (holder, moved) = match carrier {
            Some(s) => (s.body(), self.segment_moved(&s)),
            None => {
                let plate = &self.wood.plates[&t.organism];
                (plate.body(), self.plate_moved(plate))
            }
        };
        let place = |v: Vec2| moved.map_or(v, |m| m.apply(v));
        let turn = moved.map_or(0.0, |m| m.turn);
        let position = place(shape.rest.0);
        let (linear_velocity, angular_velocity) = self
            .world
            .motion(holder)
            .and_then(|m| {
                Some((
                    self.world.velocity_at_point(holder, position)?,
                    m.angular_velocity,
                ))
            })
            .unwrap_or_default();
        let segment = Segment {
            entity: self.wood.entity(),
            length: shape.length,
            width: shape.width,
            parent: carrier.map(|_| parent.id),
            size: t.size,
            mass: shape.mass,
            sail: shape.sail,
            rest: shape.rest,
        };
        assert!(self.world.insert_body(
            segment.body(),
            BodySpec {
                kind: BodyKind::Dynamic,
                position,
                angle: shape.rest.1 + turn,
                linear_velocity,
                angular_velocity,
                linear_damping: DRAG,
                angular_damping: DRAG,
                ..Default::default()
            },
            &[segment.shape(LIVING)],
        ));
        if carrier.is_none() {
            // A trunk's foot stands in the dirt its roots hold it in.
            self.world
                .pass_through(segment.entity, self.assembly.body().entity);
        }
        let (from, to) = (self.at(parent.cell), self.at(t.cell));
        self.world
            .insert_connection(
                segment.hinge(),
                ConnectionSpec {
                    root_body: holder,
                    body: segment.body(),
                    root: place(from),
                    axis: (to - from).rotate_radians(turn),
                    connection: Connection::new(engine_material::WOOD, segment.beam())
                        .expect("a wood segment's beam"),
                    bends: true,
                },
            )
            .expect("a fresh hinge between two bodies");
        self.wood.living.insert(t.id, segment);
        self.part(&segment);
    }

    /// Bring the bodies in step with what growth did at a boundary, before
    /// the next step. Wood the trees no longer have becomes dead wood, drawn
    /// as `before`, the bed as the boundary found it, has it. New wood gets a
    /// segment. Wood that thickened, or whose needles and cones changed, is
    /// reshaped where it hangs.
    pub(super) fn sync_wood(&mut self, before: &plants::Observation) {
        let o = self.plants.observe();
        let (wood, on) = wood_of(&o);
        let present: BTreeSet<u64> = wood.iter().map(|(t, _)| t.id).collect();
        // Everything that went, so that dying wood takes its needles and
        // cones with it.
        let remaining: BTreeSet<u64> = o.tissues.iter().map(|t| t.id).collect();
        let mut gone: BTreeSet<u64> = before
            .tissues
            .iter()
            .map(|t| t.id)
            .filter(|id| !remaining.contains(id))
            .collect();
        gone.extend(self.wood.living.keys().filter(|id| !present.contains(id)));
        if gone.iter().any(|id| self.wood.living.contains_key(id)) {
            self.kill(before, &gone.into_iter().collect::<Vec<_>>());
        }
        self.sync_plates(&o);
        for (t, parent) in wood {
            let carried = on.get(&t.id).map_or(&[][..], Vec::as_slice);
            let Some(segment) = self.wood.living.get(&t.id).copied() else {
                self.grow_segment(t, parent, carried);
                self.wood.work.grown += 1;
                continue;
            };
            let shape = self.shape(t, parent, carried);
            if segment.size != t.size || segment.mass != shape.mass {
                self.reshape(t.id, segment, t.size, shape);
            }
        }
    }

    /// A living segment reshaped where it hangs: its box as wide as its wood
    /// now is, carrying what now grows on it, and its hinge's beam as
    /// wide. The hinge keeps the rest it was joined at.
    fn reshape(&mut self, id: u64, segment: Segment, size: u16, shape: Shape) {
        let segment = Segment {
            width: shape.width,
            size,
            mass: shape.mass,
            sail: shape.sail,
            ..segment
        };
        assert!(self.world.replace_colliders(
            segment.body(),
            ColliderRole::PRIMARY,
            &[segment.shape(LIVING)]
        ));
        // Mass first, so that the hinge is damped for the body as it is now.
        self.world.refresh_mass_properties(segment.body());
        if let Err(why) = self
            .world
            .reshape_connection(segment.hinge(), segment.beam())
        {
            self.wood.work.refused += 1;
            self.wood.work.refusal = Some((id, why));
        }
        self.wood.living.insert(id, segment);
        self.wood.work.reshaped += 1;
    }

    /// Move the bodies on by `dt`: snow builds up, the wind blows, and the
    /// wood beyond a hinge the load exhausted falls away. A hinge of dead wood
    /// that breaks lets the wood beyond it fall from the rest. A root whose
    /// anchor the load exhausted grips nothing again, and a tree that no root
    /// holds is uprooted.
    pub(super) fn step_wood(&mut self, dt: f32) {
        if let Some(snow) = &mut self.wood.snow
            && snow.share() < 1.0
        {
            snow.age += dt;
            self.world
                .set_gravity_scale(snow.body(), snow.share(), true);
        }
        self.blow(dt);
        self.world.step(dt);
        let severed = self.world.severed_connections().to_vec();
        for joint in severed {
            let living = self
                .wood
                .living
                .iter()
                .find(|(_, s)| s.hinge() == joint)
                .map(|(&id, _)| id);
            if let Some(id) = living {
                self.wood.breaks += 1;
                self.fall(id);
            } else if self.wood.dead.iter().any(|d| d.segment.hinge() == joint) {
                self.wood.breaks += 1;
                self.world.remove_connection(joint);
            } else {
                self.pull(joint);
            }
        }
        self.uproot();
        self.settle();
        self.sweep();
    }

    /// How a segment's box lies now.
    fn lying(&self, segment: &Segment) -> Option<Lying> {
        let motion = self.world.motion(segment.body())?;
        Some(Lying {
            center: motion.position,
            angle: motion.angle,
            half: segment.half(),
        })
    }

    /// How the snow's box lies now.
    fn snow_lying(&self, snow: &Snow) -> Option<Lying> {
        let motion = self.world.motion(snow.body())?;
        Some(Lying {
            center: motion.position,
            angle: motion.angle,
            half: SNOW_HALF,
        })
    }

    /// Dead wood and loose snow that the living `segment` lies in pass
    /// through wood until they are clear of it: living wood has grown into
    /// them, or was built where they lie.
    fn part(&mut self, segment: &Segment) {
        let Some(grown) = self.lying(segment) else {
            return;
        };
        for i in 0..self.wood.dead.len() {
            let dead = self.wood.dead[i].segment;
            if !self.wood.dead[i].parting && self.lying(&dead).is_some_and(|l| l.overlaps(&grown)) {
                self.world.set_collision_groups(dead.collider(), PARTING);
                self.wood.dead[i].parting = true;
            }
        }
        if let Some(snow) = self.wood.snow
            && !snow.fastened
            && !snow.parting
            && self.snow_lying(&snow).is_some_and(|l| l.overlaps(&grown))
        {
            self.world.set_collision_groups(snow.collider(), PARTING);
            self.wood.snow = Some(Snow {
                parting: true,
                ..snow
            });
        }
    }

    /// Dead wood and loose snow that have come clear of the living wood no
    /// longer pass through wood.
    fn settle(&mut self) {
        let snow = self.wood.snow.filter(|s| s.parting);
        if snow.is_none() && !self.wood.dead.iter().any(|d| d.parting) {
            return;
        }
        let living: Vec<Lying> = self
            .wood
            .living
            .values()
            .filter_map(|s| self.lying(s))
            .collect();
        let clear =
            |own: Option<Lying>| own.is_some_and(|o| !living.iter().any(|l| l.overlaps(&o)));
        for i in 0..self.wood.dead.len() {
            let dead = self.wood.dead[i].segment;
            if self.wood.dead[i].parting && clear(self.lying(&dead)) {
                self.world.set_collision_groups(dead.collider(), DEAD_WOOD);
                self.wood.dead[i].parting = false;
            }
        }
        if let Some(snow) = snow
            && clear(self.snow_lying(&snow))
        {
            self.world.set_collision_groups(snow.collider(), DEAD_WOOD);
            self.wood.snow = Some(Snow {
                parting: false,
                ..snow
            });
        }
    }

    /// The living wood's colliders, which light passes: its tissues shade at
    /// their cells.
    pub(super) fn living_colliders(&self) -> Vec<ColliderId> {
        self.wood.living.values().map(Segment::collider).collect()
    }

    /// The wood beyond a broken or cut hinge leaves its tree, as in a cut, and
    /// falls as dead wood.
    pub(super) fn fall(&mut self, id: u64) {
        let o = self.plants.observe();
        let gone = beyond(&o, id);
        self.kill(&o, &gone);
        self.plants.remove_many(&gone).unwrap();
    }

    /// The living wood of `gone` becomes dead wood where it is, each segment
    /// drawn grey with what grew on it as `o`, where those tissues still grow,
    /// has them. Wood hinged to wood or roots that live on comes away from
    /// them; the rest keeps its hinges, to one another and to its roots,
    /// which die with their tree and hold on, so a tree that died stands
    /// until it breaks or is uprooted. Snow on it slides off. Past the cap,
    /// the wood that died first is cleared away.
    pub(super) fn kill(&mut self, o: &plants::Observation, gone: &[u64]) {
        let mut drawings: BTreeMap<u64, Vec<(i32, RenderPrimitive)>> = BTreeMap::new();
        let mut roots: BTreeMap<u64, Vec<(i32, RenderPrimitive)>> = BTreeMap::new();
        {
            let scene = self.scene(o);
            for t in gone.iter().filter_map(|id| scene.tissues.get(id)) {
                if let Some(carrier) = scene.carrier(t) {
                    drawings
                        .entry(carrier)
                        .or_default()
                        .extend(self.tissue_drawing(&scene, t, DEAD));
                } else if t.policy == tree::POLICY && t.role == Role::Root {
                    roots
                        .entry(t.organism)
                        .or_default()
                        .extend(self.tissue_drawing(&scene, t, DEAD));
                }
            }
        }
        let death = self.wood.deaths;
        self.wood.deaths += 1;
        let standing = self.standing_plates(gone);
        for id in gone {
            let Some(segment) = self.wood.living.remove(id) else {
                continue;
            };
            let parted = match segment.parent {
                Some(p) => !gone.contains(&p),
                // A foot comes away from roots that live on.
                None => self
                    .world
                    .connection(segment.hinge())
                    .is_some_and(|c| standing.contains(&c.root_body)),
            };
            let broken = self
                .world
                .connection(segment.hinge())
                .is_some_and(|c| c.failure.is_some());
            if parted || broken {
                self.world.remove_connection(segment.hinge());
            }
            // It lies in the wood it came away from until a step finds it
            // clear.
            assert!(self.world.replace_colliders(
                segment.body(),
                ColliderRole::PRIMARY,
                &[segment.shape(PARTING)]
            ));
            self.wood.dead.push(Dead {
                segment,
                parting: true,
                drawing: drawings.remove(id).unwrap_or_default(),
                death,
            });
            self.wood.work.died += 1;
        }
        if let Some(snow) = &mut self.wood.snow
            && snow.fastened
            && gone.contains(&snow.on)
        {
            // It slides off, and falls with its full weight.
            self.world.remove_connection(snow.fastening());
            self.world.set_collision_groups(snow.collider(), PARTING);
            self.world.set_gravity_scale(snow.body(), 1.0, true);
            snow.from = 1.0;
            snow.fastened = false;
            snow.parting = true;
        }
        self.kill_plates(gone, roots, death);
        while self.wood.dead.len() > self.wood.max_dead {
            let first = self.wood.dead[0].death;
            let cleared = self
                .wood
                .dead
                .iter()
                .take_while(|d| d.death == first)
                .count();
            for d in self.wood.dead.drain(..cleared) {
                self.world.remove_entity(d.segment.entity);
            }
            // Its roots go with it.
            let world = &mut self.world;
            self.wood.dead_plates.retain(|d| {
                if d.death == first {
                    world.remove_entity(d.plate.entity);
                }
                d.death != first
            });
            self.wood.work.cleared += cleared as u32;
        }
    }

    /// The living wood and roots leave the world with any snow, the wind
    /// drops, and the trees grow on as their cells stand. Dead wood stays
    /// where it is, with its roots.
    pub(super) fn rest_wood(&mut self) {
        self.clear_snow();
        for segment in std::mem::take(&mut self.wood.living).into_values() {
            self.world.remove_entity(segment.entity);
        }
        self.rest_plates();
        self.wood.wind = Wind::default();
        self.wood.physical = false;
    }

    /// Lay snow of `mass` on the selected wood, else on the tip of the eldest
    /// tree's lowest branch toward the grass, making the wood physical first.
    /// The same snow again clears it. Snow of another mass on the same wood
    /// builds up from the weight already there.
    pub(super) fn snow(&mut self, mass: f32) {
        if !self.wood.physical {
            self.build_wood();
        }
        let previous = self.wood.snow;
        self.clear_snow();
        if previous.is_some_and(|s| s.mass == mass) {
            return;
        }
        let Some(on) = self.snow_target() else {
            return;
        };
        let segment = self.wood.living[&on];
        let Some(motion) = self.world.motion(segment.body()) else {
            return;
        };
        let snow = Snow {
            entity: self.wood.entity(),
            mass,
            on,
            fastened: true,
            parting: false,
            from: previous
                .filter(|s| s.on == on)
                .map_or(0.0, |s| (s.share() * s.mass / mass).min(1.0)),
            age: 0.0,
        };
        // On top of the segment's middle, as it now lies.
        let base = motion.position + Vec2::new(0.0, 0.5 * segment.width);
        let mut collider = ColliderSpec::cuboid(snow.collider(), SNOW_HALF.x, SNOW_HALF.y);
        collider.mass_properties = Some(ColliderMassProperties {
            center: Vec2::ZERO,
            mass,
            inertia: mass * SNOW_HALF.length_squared() / 3.0,
        });
        collider.collision_groups = FASTENED;
        assert!(self.world.insert_body(
            snow.body(),
            BodySpec {
                kind: BodyKind::Dynamic,
                position: base + Vec2::new(0.0, SNOW_HALF.y),
                gravity_scale: snow.from,
                linear_damping: DRAG,
                angular_damping: DRAG,
                ..Default::default()
            },
            &[collider],
        ));
        self.world
            .insert_connection(
                snow.fastening(),
                ConnectionSpec {
                    root_body: segment.body(),
                    body: snow.body(),
                    root: base,
                    axis: Vec2::Y,
                    connection: Connection::new(FASTENING, Beam::new(1.0, 1.0, 1.0))
                        .expect("the fastening's beam"),
                    bends: false,
                },
            )
            .expect("a fresh fastening between two bodies");
        self.wood.snow = Some(snow);
    }

    fn clear_snow(&mut self) {
        if let Some(snow) = self.wood.snow.take() {
            self.world.remove_entity(snow.entity);
        }
    }

    /// The selected wood, else the tip of the eldest tree's lowest branch
    /// toward the grass: the wood of it farthest from the trunk.
    fn snow_target(&self) -> Option<u64> {
        if let Some(t) = self
            .selection()
            .filter(|t| self.wood.living.contains_key(&t.id))
        {
            return Some(t.id);
        }
        let o = self.plants.observe();
        let base = self.lowest_branch(&o)?;
        let cell = |id: u64| o.tissues.iter().find(|t| t.id == id).map(|t| t.cell);
        let from = cell(base)?;
        beyond(&o, base)
            .into_iter()
            .filter(|id| self.wood.living.contains_key(id))
            .max_by_key(|&id| {
                cell(id).map_or(0, |c| (c[0] - from[0]).abs() + (c[1] - from[1]).abs())
            })
    }

    /// How the body that carries a tissue has moved from where it lies at
    /// rest: wood rides its own segment, a needle or cone the wood it grows
    /// on, and a tree's root its tree's plate.
    pub(super) fn moved(&self, scene: &Scene, t: &Tissue) -> Option<Moved> {
        if t.policy == tree::POLICY && t.role == Role::Root {
            return self.plate_moved(self.wood.plates.get(&t.organism)?);
        }
        self.segment_moved(self.wood.living.get(&scene.carrier(t)?)?)
    }

    fn segment_moved(&self, segment: &Segment) -> Option<Moved> {
        let motion = self.world.motion(segment.body())?;
        Some(Moved {
            from: segment.rest.0,
            to: motion.position,
            turn: motion.angle - segment.rest.1,
        })
    }

    /// Dead wood where it is with its roots, and the snow.
    pub(super) fn draw_wood(&self, f: &mut RenderFrame) {
        self.draw_plates(f);
        for dead in &self.wood.dead {
            let moved = self.segment_moved(&dead.segment);
            for (z, primitive) in &dead.drawing {
                let primitive = primitive.clone();
                f.push_primitive(
                    *z,
                    match moved {
                        Some(m) => m.primitive(primitive),
                        None => primitive,
                    },
                );
            }
        }
        if let Some(snow) = &self.wood.snow
            && let Some(motion) = self.world.motion(snow.body())
        {
            let corners = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].map(|[x, y]| {
                let v = motion.position
                    + Vec2::new(x * SNOW_HALF.x, y * SNOW_HALF.y).rotate_radians(motion.angle);
                RenderPoint::new(v.x, v.y)
            });
            // Over the needles it lies on.
            f.push_primitive(
                4,
                RenderPrimitive::Polygon(RenderPolygon::filled(corners.to_vec(), SNOW)),
            );
        }
    }

    /// Each living wood tissue's hinge as the last step left it.
    pub fn hinges(&self) -> Vec<Hinge> {
        let o = self.plants.observe();
        self.wood
            .living
            .iter()
            .filter_map(|(&tissue, segment)| {
                let state = self.world.connection(segment.hinge())?;
                let motion = self.world.motion(segment.body())?;
                Some(Hinge {
                    tissue,
                    cell: o.tissues.iter().find(|t| t.id == tissue)?.cell,
                    utilization: state.load.utilization,
                    turn: state.deflection.unwrap_or(0.0),
                    droop: segment.end(segment.rest.0, segment.rest.1).y
                        - segment.end(motion.position, motion.angle).y,
                })
            })
            .collect()
    }

    /// The wood: at rest, or on its hinges with the most loaded one and the
    /// snow; and the dead wood against its cap, and the breaks. It follows
    /// the light's work on one line of the bed's text, so it stays short.
    pub fn wood_summary(&self) -> String {
        let dead = format!(
            "dead {}/{} | breaks {}",
            self.wood.dead.len(),
            self.wood.max_dead,
            self.wood.breaks
        );
        if !self.wood.physical {
            return format!("Wood at rest | {dead}");
        }
        let hinges = self.hinges();
        let wood = hinges
            .iter()
            .max_by(|a, b| a.utilization.total_cmp(&b.utilization))
            .map_or("Wood physical, none living".into(), |h| {
                format!(
                    "Wood on {} hinge{}, most loaded {:.2} of safe at {:?}",
                    hinges.len(),
                    if hinges.len() == 1 { "" } else { "s" },
                    h.utilization,
                    h.cell
                )
            });
        let snow = self.wood.snow.map_or("none".into(), |s| {
            let weight = if s.heavy() { "heavy" } else { "light" };
            match hinges.iter().find(|h| s.fastened && h.tissue == s.on) {
                // Its weight is still building up.
                Some(h) if s.share() < 1.0 => {
                    format!("{weight} {:.0}% at {:?}", 100.0 * s.share(), h.cell)
                }
                Some(h) => format!("{weight} at {:?}", h.cell),
                None => format!("{weight} slid off"),
            }
        });
        format!("{wood} | snow {snow} | {dead}")
    }
}
