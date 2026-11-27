//! The tree bed's wood as bodies under gravity. Each wood tissue becomes a
//! segment from the tissue it grew from to its own cell, as it is drawn,
//! joined there by a hinge with the bending stiffness and strength of a square
//! wood beam as wide as the segment. The trunk's foot is hinged to the dirt.
//! Needles and cones weigh on the wood they grow on. Snow on a branch bends it
//! further. A load that exhausts a hinge breaks the wood beyond it away: the
//! tree loses those tissues, as in a cut, and they fall to the dirt as dead
//! wood, while the snow on them slides off. While the wood is physical, growth
//! waits.
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
const SNOW_HALF: Vec2 = Vec2::new(0.45, 0.2);
const SNOW: RenderColor = RenderColor::rgb(0.88, 0.94, 1.0);
/// The wood's and snow's physics identities count up from here, past the
/// dirt's and the shade's, and are never reused.
const FIRST_ENTITY: u64 = 16;
const WOOD_GROUP: u32 = 1 << 8;
/// Living wood touches nothing. Its hinges hold it, and its segments overlap
/// where they join and where the trunk meets the dirt.
const LIVING: CollisionGroups = CollisionGroups::NONE;
/// Fallen wood, and snow that slid off it, lands on the dirt and passes
/// through wood.
const FALLEN: CollisionGroups = CollisionGroups {
    memberships: WOOD_GROUP,
    filter: !WOOD_GROUP,
};
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
#[derive(Clone, Debug, Default)]
pub struct Wood {
    /// While the wood is physical, growth waits.
    pub physical: bool,
    /// Each living wood tissue's segment.
    pub living: BTreeMap<u64, Segment>,
    /// Wood that broke or was cut away while physical, where it fell.
    pub fallen: Vec<Fallen>,
    pub snow: Option<Snow>,
    /// Hinges that the load broke.
    pub breaks: u32,
    entities: u64,
}

impl Wood {
    fn entity(&mut self) -> PhysicsId {
        self.entities += 1;
        PhysicsId::new(FIRST_ENTITY + self.entities - 1)
    }
}

/// A wood tissue's body: a capsule along its segment, as wide as the wood is
/// drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub entity: PhysicsId,
    pub length: f32,
    pub width: f32,
    /// Where the body lay when it was built, where the tissue's drawing lies:
    /// its center and its angle, with its y axis along the segment.
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

    /// Its far end, at the tissue's cell when it was built, for a body at
    /// `center` turned to `angle`.
    fn end(&self, center: Vec2, angle: f32) -> Vec2 {
        center + Vec2::new(0.0, 0.5 * self.length).rotate_radians(angle)
    }
}

/// A segment that broke or was cut away, with the drawing it and what grew
/// on it had, in grey.
#[derive(Clone, Debug)]
pub struct Fallen {
    pub segment: Segment,
    drawing: Vec<(i32, RenderPrimitive)>,
}

/// Snow fastened on top of a segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snow {
    pub entity: PhysicsId,
    /// Its mass once built up.
    pub mass: f32,
    /// The wood tissue it was laid on.
    pub on: u64,
    /// Whether it still lies on that wood, or slid off as the wood fell.
    pub fastened: bool,
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
}

/// A living wood tissue's hinge, as the last step left it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hinge {
    pub tissue: u64,
    pub cell: [i32; 2],
    /// Its last load over its safe limit.
    pub utilization: f32,
    /// How far it has turned since the wood became physical, in radians,
    /// counterclockwise.
    pub turn: f32,
    /// How far the wood's end has dropped since then, in cells.
    pub droop: f32,
}

/// How a body has moved since it was built, to move a drawing with it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Moved {
    from: Vec2,
    to: Vec2,
    turn: f32,
}

impl Moved {
    pub(super) fn point(self, p: RenderPoint) -> RenderPoint {
        let v = self.to + (Vec2::new(p.x, p.y) - self.from).rotate_radians(self.turn);
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

impl TreeBed {
    pub(super) fn at(&self, cell: [i32; 2]) -> Vec2 {
        self.terrain.cell_center(CellCoord::new(cell[0], cell[1]))
    }

    /// Make the wood physical, or let it rest and grow again.
    pub(super) fn toggle_wood(&mut self) {
        if self.wood.physical {
            self.rest_wood();
        } else {
            self.build_wood();
        }
    }

    /// Every tree's wood as segments where it now grows, each hinged where it
    /// grew from: to the wood it grew on, or for a trunk's foot to the dirt.
    pub(super) fn build_wood(&mut self) {
        let o = self.plants.observe();
        let tissues: BTreeMap<u64, &Tissue> = o.tissues.iter().map(|t| (t.id, t)).collect();
        let wood: Vec<(&Tissue, &Tissue)> = o
            .connections
            .iter()
            .map(|(p, c)| (tissues[c], tissues[p]))
            .filter(|(t, _)| t.policy == tree::POLICY && t.role == Role::Wood)
            .collect();
        for &(t, parent) in &wood {
            let (from, to) = (self.at(parent.cell), self.at(t.cell));
            let axis = to - from;
            let length = axis.length();
            let width = 0.5 * f32::from(t.size).sqrt();
            let rest = ((from + to) * 0.5, axis.y.atan2(axis.x) - FRAC_PI_2);
            // A square beam of wood, and what grows on it, in the body's
            // frame.
            let own = engine_material::WOOD.density * width * width * length;
            let mut parts = vec![(
                own,
                Vec2::ZERO,
                own * (length * length + width * width) / 12.0,
            )];
            for (_, c) in o.connections.iter().filter(|&&(p, _)| p == t.id) {
                let per_size = match tissues[c].role {
                    Role::Shoot => NEEDLE_MASS,
                    Role::Cone => CONE_MASS,
                    _ => continue,
                };
                let at = (self.at(tissues[c].cell) - rest.0).rotate_radians(-rest.1);
                parts.push((per_size * f32::from(tissues[c].size), at, 0.0));
            }
            let mass: f32 = parts.iter().map(|p| p.0).sum();
            let center = parts.iter().fold(Vec2::ZERO, |sum, p| sum + p.1 * p.0) / mass;
            let inertia = parts
                .iter()
                .map(|&(m, at, own)| own + m * (at - center).length_squared())
                .sum();
            let segment = Segment {
                entity: self.wood.entity(),
                length,
                width,
                rest,
            };
            let mut collider = ColliderSpec::capsule(segment.collider(), 0.5 * length, 0.5 * width);
            collider.mass_properties = Some(ColliderMassProperties {
                center,
                mass,
                inertia,
            });
            collider.collision_groups = LIVING;
            assert!(self.world.insert_body(
                segment.body(),
                BodySpec {
                    kind: BodyKind::Dynamic,
                    position: rest.0,
                    angle: rest.1,
                    linear_damping: DRAG,
                    angular_damping: DRAG,
                    ..Default::default()
                },
                &[collider],
            ));
            self.wood.living.insert(t.id, segment);
        }
        for &(t, parent) in &wood {
            let segment = self.wood.living[&t.id];
            let (from, to) = (self.at(parent.cell), self.at(t.cell));
            let beam = Beam::new(segment.width, segment.width, segment.length);
            self.world
                .insert_connection(
                    segment.hinge(),
                    ConnectionSpec {
                        root_body: self
                            .wood
                            .living
                            .get(&parent.id)
                            .map_or(self.assembly.body(), Segment::body),
                        body: segment.body(),
                        root: from,
                        axis: to - from,
                        connection: Connection::new(engine_material::WOOD, beam)
                            .expect("a wood segment's beam"),
                        bends: true,
                    },
                )
                .expect("a fresh hinge between two bodies");
        }
        self.wood.physical = true;
    }

    /// The living wood leaves the world with any snow, and the tree grows
    /// again as it stands. Wood that has fallen stays where it lies.
    pub(super) fn rest_wood(&mut self) {
        self.clear_snow();
        for segment in std::mem::take(&mut self.wood.living).into_values() {
            self.world.remove_entity(segment.entity);
        }
        self.wood.physical = false;
    }

    /// Move the bodies on by `dt`: snow builds up, and the wood beyond a hinge
    /// the load exhausted falls away.
    pub(super) fn step_wood(&mut self, dt: f32) {
        if let Some(snow) = &mut self.wood.snow
            && snow.share() < 1.0
        {
            snow.age += dt;
            self.world
                .set_gravity_scale(snow.body(), snow.share(), true);
        }
        self.world.step(dt);
        let severed = self.world.severed_connections().to_vec();
        for joint in severed {
            let broken = self
                .wood
                .living
                .iter()
                .find(|(_, s)| s.hinge() == joint)
                .map(|(&id, _)| id);
            if let Some(id) = broken {
                self.wood.breaks += 1;
                self.fall(id);
            }
        }
    }

    /// The wood beyond a broken or cut hinge leaves its tree, as in a cut, and
    /// falls as dead wood. Each of its segments keeps the drawing it and what
    /// grew on it had, grey, and lands on the dirt. Snow on it slides off.
    pub(super) fn fall(&mut self, id: u64) {
        let o = self.plants.observe();
        let gone = beyond(&o, id);
        let mut drawings: BTreeMap<u64, Vec<(i32, RenderPrimitive)>> = BTreeMap::new();
        {
            let scene = self.scene(&o);
            for t in gone.iter().map(|id| scene.tissues[id]) {
                if let Some(carrier) = scene.carrier(t) {
                    drawings
                        .entry(carrier)
                        .or_default()
                        .extend(self.tissue_drawing(&scene, t, DEAD));
                }
            }
        }
        for id in &gone {
            if let Some(segment) = self.wood.living.remove(id) {
                self.world.set_collision_groups(segment.collider(), FALLEN);
                self.wood.fallen.push(Fallen {
                    segment,
                    drawing: drawings.remove(id).unwrap_or_default(),
                });
            }
        }
        if let Some(snow) = &mut self.wood.snow
            && gone.contains(&snow.on)
        {
            // It slides off, and falls with its full weight.
            self.world
                .remove_connection(JointId::new(snow.entity, JointRole::new(1)));
            self.world.set_collision_groups(snow.collider(), FALLEN);
            self.world.set_gravity_scale(snow.body(), 1.0, true);
            snow.from = 1.0;
            snow.fastened = false;
        }
        self.plants.remove_many(&gone).unwrap();
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
        collider.collision_groups = LIVING;
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
                JointId::new(snow.entity, JointRole::new(1)),
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

    /// How the body that carries a tissue has moved since it was built: wood
    /// rides its own segment, and a needle or cone the wood it grows on.
    pub(super) fn moved(&self, scene: &Scene, t: &Tissue) -> Option<Moved> {
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

    /// Fallen wood where it lies, and the snow.
    pub(super) fn draw_wood(&self, f: &mut RenderFrame) {
        for fallen in &self.wood.fallen {
            let moved = self.segment_moved(&fallen.segment);
            for (z, primitive) in &fallen.drawing {
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

    /// The wood: at rest while it grows, or physical, with its most loaded
    /// hinge and its snow; and what has fallen.
    pub fn wood_summary(&self) -> String {
        let fallen = format!(
            "fallen segments {} | breaks {}",
            self.wood.fallen.len(),
            self.wood.breaks
        );
        if !self.wood.physical {
            return format!("Wood at rest while it grows | {fallen}");
        }
        let hinges = self.hinges();
        let worst = hinges
            .iter()
            .max_by(|a, b| a.utilization.total_cmp(&b.utilization))
            .map_or("none".into(), |h| {
                format!("{:.2} of safe at {:?}", h.utilization, h.cell)
            });
        let snow = self.wood.snow.map_or("none".into(), |s| {
            let place = hinges
                .iter()
                .find(|h| s.fastened && h.tissue == s.on)
                .map_or("the dirt, slid off fallen wood".into(), |h| {
                    format!("wood at {:?}", h.cell)
                });
            let building = if s.share() < 1.0 {
                format!(", {:.0}% built up", 100.0 * s.share())
            } else {
                String::new()
            };
            let weight = if s.heavy() { "heavy" } else { "light" };
            format!("{weight} on {place}{building}")
        });
        format!(
            "Wood physical, growth waits | hinges {}, most loaded {worst} | snow {snow} | {fallen}",
            hinges.len()
        )
    }
}
