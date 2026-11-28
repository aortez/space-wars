//! The trees' roots as what holds them. While the wood is physical, each
//! tree's roots are one body, its root plate, to which the trunk's foot is
//! hinged. Every root that lies in dirt anchors the plate there, with the
//! strength and the give of the dirt's grip on a root of its size. The
//! anchors share the tree's weight, and whatever turns the tree, by where
//! they hold, so roots that reach wide and deep hold a tree that roots close
//! under its trunk would let fall. An anchor the load exhausts lets go for
//! good: its root grips nothing again. A tree whose last anchor lets go is
//! uprooted, and dies where it falls with its roots.
//!
//! The wind is the bed's sideways load: a push toward the grass on every
//! piece of wood that stands, by the face it turns to the wind and the
//! needles and cones it carries, still at the dirt and full from `WIND_RISE`
//! cells above it. Living wood stands, and dead wood stands while it is
//! joined, through the wood it grew on, to roots that have not gone over:
//! fallen wood lies where it fell.
use super::*;
use engine_material::{Beam, Connection, FailureMode, Material};

/// The dirt's grip on a root, as the material of an anchor whose beam is the
/// root's own section over one cell. It is a hundredth as stiff as wood and a
/// twelfth as strong, so a root gives an eighth of a cell and then pulls out
/// of the dirt, long before it would snap, and a loaded tree rocks on its
/// roots before they let go. Dirt soaks up motion, and yields where wood
/// snaps.
pub const GRIP: Material = Material {
    name: "grip",
    density: 1.0,
    stiffness: 2.0e3,
    strength: 240.0,
    toughness: 20.0,
    damping: 0.9,
    mode: FailureMode::Ductile,
};
/// The mass of a root tissue per size: a cell's length of wood as wide as
/// wood of its size.
pub const ROOT_MASS: f32 = 0.15;
/// Each wind's name and its push per unit of sail.
pub const WINDS: [(&str, f32); 3] = [("calm", 0.0), ("breeze", 2.0), ("gale", 6.0)];
/// The wind rises and drops by the gale's whole push in this many seconds,
/// so that it leans on a tree rather than striking it.
pub const WIND_SECONDS: f32 = 3.0;
/// The wind is still at the dirt and reaches its full push this many cells
/// above it, so what lies on the dirt stays there.
pub const WIND_RISE: f32 = 4.0;
/// The sail a needle or cone tissue adds, per size, to the wood it grows on.
/// A cell of the thinnest wood, side on, is 0.5.
pub const NEEDLE_SAIL: f32 = 0.15;
pub const CONE_SAIL: f32 = 0.05;
/// A dead tree whose roots have turned this far from where they grew has
/// gone over, and the wind no longer pushes it.
pub const FALLEN: f32 = std::f32::consts::FRAC_PI_4;
/// Dead wood and roots that fall this many cells below the bed's dirt are
/// lost, and cleared away.
const LOST_BELOW: f32 = 4.0;

/// A tree's roots as one body, anchored in the dirt at each root that grips
/// it.
#[derive(Clone, Debug)]
pub struct Plate {
    pub entity: PhysicsId,
    /// The tree's seed, the root its trunk stands on.
    pub seed: u64,
    /// The anchor of each root that grips, by its tissue.
    pub anchors: BTreeMap<u64, JointId>,
    /// Where it lay when it was built, at the seed's cell.
    rest: Vec2,
    /// Its roots' mass in its body's frame, when it was last shaped.
    mass: ColliderMassProperties,
    /// Its anchors' identities count up from one, and are never reused.
    joints: u32,
}

impl Plate {
    pub fn body(&self) -> BodyId {
        BodyId::new(self.entity, BodyRole::PRIMARY)
    }

    fn collider(&self) -> ColliderId {
        ColliderId::new(self.entity, ColliderRole::PRIMARY, 0)
    }

    /// Its body touches nothing and shades nothing: it only weighs, and
    /// carries its anchors.
    fn shape(&self) -> ColliderSpec {
        let mut collider = ColliderSpec::cuboid(self.collider(), 0.25, 0.25);
        collider.mass_properties = Some(self.mass);
        collider.sensor = true;
        collider.collision_groups = CollisionGroups::NONE;
        collider
    }
}

/// A dead tree's root plate, with the drawing its roots had, in grey. It
/// holds the dead tree by the anchors it had left.
#[derive(Clone, Debug)]
pub struct DeadPlate {
    pub plate: Plate,
    pub(super) drawing: Vec<(i32, RenderPrimitive)>,
    /// The death it shares with its tree's wood, with which it is cleared.
    pub(super) death: u32,
}

/// A root's anchor, as the last step left it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub tissue: u64,
    pub cell: [i32; 2],
    /// Its last load over its safe limit.
    pub utilization: f32,
    /// The force it carried along its root, positive where the root is
    /// pulled out along its length, and across it.
    pub pull: [f32; 2],
}

/// The wind: which of `WINDS` blows, and its push now as it rises or drops
/// toward that.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wind {
    pub level: usize,
    pub push: f32,
}

/// A tree's roots, each with the root it grew from, in ID order, and whether
/// the tree has wood.
struct Rooted<'a> {
    roots: Vec<(&'a Tissue, Option<&'a Tissue>)>,
    wood: bool,
}

/// Every tree's roots, by its organism.
fn rooted(o: &plants::Observation) -> BTreeMap<u64, Rooted<'_>> {
    let tissues: BTreeMap<u64, &Tissue> = o.tissues.iter().map(|t| (t.id, t)).collect();
    let parents: BTreeMap<u64, u64> = o.connections.iter().map(|&(p, c)| (c, p)).collect();
    let mut trees: BTreeMap<u64, Rooted> = BTreeMap::new();
    for t in o.tissues.iter().filter(|t| t.policy == tree::POLICY) {
        let tree = trees.entry(t.organism).or_insert(Rooted {
            roots: Vec::new(),
            wood: false,
        });
        match t.role {
            Role::Root => tree.roots.push((t, parents.get(&t.id).map(|p| tissues[p]))),
            Role::Wood => tree.wood = true,
            _ => {}
        }
    }
    trees
}

impl TreeBed {
    /// The mass of `roots` about a plate that rests at `rest`: each root a
    /// cell of wood at its own cell.
    fn plate_mass(
        &self,
        rest: Vec2,
        roots: &[(&Tissue, Option<&Tissue>)],
    ) -> ColliderMassProperties {
        let parts: Vec<(f32, Vec2)> = roots
            .iter()
            .map(|(t, _)| (ROOT_MASS * f32::from(t.size), self.at(t.cell) - rest))
            .collect();
        let mass: f32 = parts.iter().map(|p| p.0).sum();
        let center = parts.iter().fold(Vec2::ZERO, |sum, p| sum + p.1 * p.0) / mass;
        let inertia = parts
            .iter()
            .map(|&(m, at)| m / 6.0 + m * (at - center).length_squared())
            .sum();
        ColliderMassProperties {
            center,
            mass,
            inertia,
        }
    }

    /// Whether a root at `cell` lies in dirt, and so grips it.
    fn grips(&self, cell: [i32; 2]) -> bool {
        uptake(
            &self.world,
            &self.assembly,
            &self.terrain,
            &self.geometry,
            self.availability,
        )
        .soil(cell)
        .is_some()
    }

    /// Bring the root plates in step with the trees' roots, before the next
    /// step: a plate for each tree with wood, an anchor for each of its roots
    /// that lies in dirt and has not pulled out, where the plate has carried
    /// it, and none for a root the tree no longer has.
    pub(super) fn sync_plates(&mut self, o: &plants::Observation) {
        let living: BTreeSet<u64> = o.tissues.iter().map(|t| t.id).collect();
        self.wood.pulled.retain(|id| living.contains(id));
        let trees = rooted(o);
        // A tree that is gone without wood to hold leaves nothing behind.
        let gone: Vec<u64> = self
            .wood
            .plates
            .keys()
            .filter(|organism| !trees.contains_key(organism))
            .copied()
            .collect();
        for organism in gone {
            if let Some(plate) = self.wood.plates.remove(&organism) {
                self.world.remove_entity(plate.entity);
            }
        }
        for (&organism, tree) in &trees {
            let Some(&(seed, _)) = tree.roots.iter().find(|(_, parent)| parent.is_none()) else {
                continue;
            };
            if !self.wood.plates.contains_key(&organism) {
                if !tree.wood {
                    continue;
                }
                let rest = self.at(seed.cell);
                let plate = Plate {
                    entity: self.wood.entity(),
                    seed: seed.id,
                    anchors: BTreeMap::new(),
                    rest,
                    mass: self.plate_mass(rest, &tree.roots),
                    joints: 0,
                };
                assert!(self.world.insert_body(
                    plate.body(),
                    BodySpec {
                        kind: BodyKind::Dynamic,
                        position: rest,
                        linear_damping: DRAG,
                        angular_damping: DRAG,
                        ..Default::default()
                    },
                    &[plate.shape()],
                ));
                self.wood.plates.insert(organism, plate);
            }
            let plate = &self.wood.plates[&organism];
            let mass = self.plate_mass(plate.rest, &tree.roots);
            if mass != plate.mass {
                let plate = self.wood.plates.get_mut(&organism).unwrap();
                plate.mass = mass;
                let (body, shape) = (plate.body(), plate.shape());
                assert!(
                    self.world
                        .replace_colliders(body, ColliderRole::PRIMARY, &[shape])
                );
                self.world.refresh_mass_properties(body);
            }
            let plate = &self.wood.plates[&organism];
            let shed: Vec<u64> = plate
                .anchors
                .keys()
                .filter(|id| !tree.roots.iter().any(|(t, _)| t.id == **id))
                .copied()
                .collect();
            for id in shed {
                let plate = self.wood.plates.get_mut(&organism).unwrap();
                if let Some(joint) = plate.anchors.remove(&id) {
                    self.world.remove_connection(joint);
                }
            }
            for &(t, parent) in &tree.roots {
                let plate = &self.wood.plates[&organism];
                if plate.anchors.contains_key(&t.id)
                    || self.wood.pulled.contains(&t.id)
                    || !self.grips(t.cell)
                {
                    continue;
                }
                self.anchor(organism, t, parent);
            }
        }
    }

    /// Anchor root `t` of the tree `organism` in the dirt where its plate
    /// has carried its cell, along the way it grew: from the root it grew
    /// from, or for the seed downward.
    fn anchor(&mut self, organism: u64, t: &Tissue, parent: Option<&Tissue>) {
        let plate = &self.wood.plates[&organism];
        let moved = self.plate_moved(plate);
        let at = moved.map_or(self.at(t.cell), |m| m.apply(self.at(t.cell)));
        let along = parent.map_or(Vec2::new(0.0, -1.0), |p| self.at(t.cell) - self.at(p.cell));
        let width = 0.5 * f32::from(t.size).sqrt();
        let joint = JointId::new(plate.entity, JointRole::new(plate.joints + 1));
        let body = plate.body();
        self.world
            .insert_anchor(
                joint,
                AnchorSpec {
                    support: self.assembly.body(),
                    body,
                    at,
                    axis: along.rotate_radians(moved.map_or(0.0, |m| m.turn())),
                    connection: Connection::new(GRIP, Beam::new(width, width, 1.0))
                        .expect("a root's beam"),
                },
            )
            .expect("a fresh anchor between the dirt and a root plate");
        let plate = self.wood.plates.get_mut(&organism).unwrap();
        plate.joints += 1;
        plate.anchors.insert(t.id, joint);
    }

    /// How a plate's body has moved from where it was built.
    pub(super) fn plate_moved(&self, plate: &Plate) -> Option<Moved> {
        let motion = self.world.motion(plate.body())?;
        Some(Moved::between(plate.rest, motion.position, motion.angle))
    }

    /// The living plates leave the world with their anchors. Dead trees keep
    /// theirs.
    pub(super) fn rest_plates(&mut self) {
        for plate in std::mem::take(&mut self.wood.plates).into_values() {
            self.world.remove_entity(plate.entity);
        }
    }

    /// The plates of the trees whose seed is among `gone` die with them:
    /// each stays where it is with the anchors it has left, its roots drawn
    /// grey as `drawings` has them, and shares `death` with its tree's wood.
    pub(super) fn kill_plates(
        &mut self,
        gone: &[u64],
        mut drawings: BTreeMap<u64, Vec<(i32, RenderPrimitive)>>,
        death: u32,
    ) {
        let dying: Vec<u64> = self
            .wood
            .plates
            .iter()
            .filter(|(_, plate)| gone.contains(&plate.seed))
            .map(|(&organism, _)| organism)
            .collect();
        for organism in dying {
            let plate = self.wood.plates.remove(&organism).unwrap();
            self.wood.dead_plates.push(DeadPlate {
                plate,
                drawing: drawings.remove(&organism).unwrap_or_default(),
                death,
            });
        }
    }

    /// The bodies of the plates whose trees live on though `gone` leaves
    /// them.
    pub(super) fn standing_plates(&self, gone: &[u64]) -> BTreeSet<BodyId> {
        self.wood
            .plates
            .values()
            .filter(|plate| !gone.contains(&plate.seed))
            .map(Plate::body)
            .collect()
    }

    /// Take the next of `WINDS`.
    pub(super) fn turn_wind(&mut self) {
        if !self.wood.physical {
            self.build_wood();
        }
        self.wood.wind.level = (self.wood.wind.level + 1) % WINDS.len();
    }

    /// The dead wood that still stands: joined, through other dead wood, to
    /// a dead tree's roots that have not gone over.
    fn standing_dead(&self) -> Vec<&Segment> {
        let mut held: BTreeSet<BodyId> = self
            .wood
            .dead_plates
            .iter()
            .filter(|d| {
                self.plate_moved(&d.plate)
                    .is_some_and(|m| m.turn().abs() < FALLEN)
            })
            .map(|d| d.plate.body())
            .collect();
        let mut standing = Vec::new();
        if held.is_empty() {
            return standing;
        }
        let mut lying: Vec<&Segment> = self.wood.dead.iter().map(|d| &d.segment).collect();
        loop {
            let before = lying.len();
            lying.retain(|segment| {
                let joined = self
                    .world
                    .connection(segment.hinge())
                    .is_some_and(|c| c.failure.is_none() && held.contains(&c.root_body));
                if joined {
                    held.insert(segment.body());
                    standing.push(*segment);
                }
                !joined
            });
            if lying.len() == before {
                return standing;
            }
        }
    }

    /// The wind rises or drops toward its level, and pushes every piece of
    /// wood that stands toward the grass: by its sail, and by how high it
    /// stands above the dirt.
    pub(super) fn blow(&mut self, dt: f32) {
        let wind = &mut self.wood.wind;
        let (to, gale) = (WINDS[wind.level].1, WINDS[WINDS.len() - 1].1);
        let step = dt * gale / WIND_SECONDS;
        wind.push = if wind.push < to {
            (wind.push + step).min(to)
        } else {
            (wind.push - step).max(to)
        };
        let push = wind.push;
        self.world.clear_forces();
        if push == 0.0 {
            return;
        }
        let surface = self.at([0, TOP_DIRT]).y + 0.5;
        let sails: Vec<(BodyId, (f32, f32, f32))> = self
            .wood
            .living
            .values()
            .chain(self.standing_dead())
            .map(|s| (s.body(), s.sail()))
            .collect();
        for (body, (width, length, carried)) in sails {
            let Some(motion) = self.world.motion(body) else {
                continue;
            };
            // The face it turns to the wind, as deep as it is wide: its
            // length where it stands, its width where it points along the
            // wind.
            let (upright, level) = (motion.angle.cos().abs(), motion.angle.sin().abs());
            let face = width * (length * upright + width * level);
            let height = ((motion.position.y - surface) / WIND_RISE).clamp(0.0, 1.0);
            self.world
                .apply_force(body, Vec2::new(push * height * (face + carried), 0.0), true);
        }
    }

    /// The anchor `joint` gave way: its root grips nothing again. False for
    /// a joint that is no plate's anchor.
    pub(super) fn pull(&mut self, joint: JointId) -> bool {
        let living = self.wood.plates.values_mut().map(|p| (p, true));
        let dead = self
            .wood
            .dead_plates
            .iter_mut()
            .map(|d| (&mut d.plate, false));
        for (plate, alive) in living.chain(dead) {
            let Some(tissue) = plate
                .anchors
                .iter()
                .find(|(_, anchor)| **anchor == joint)
                .map(|(&tissue, _)| tissue)
            else {
                continue;
            };
            plate.anchors.remove(&tissue);
            if alive {
                self.wood.pulled.insert(tissue);
            }
            self.world.remove_connection(joint);
            self.wood.pulls += 1;
            return true;
        }
        false
    }

    /// A living tree that no anchor holds is uprooted: it leaves the bed's
    /// plants, as in a cut at its seed, and its wood and roots die where
    /// they are.
    pub(super) fn uproot(&mut self) {
        let loose: Vec<u64> = self
            .wood
            .plates
            .iter()
            .filter(|(_, plate)| plate.anchors.is_empty())
            .map(|(&organism, _)| organism)
            .collect();
        for organism in loose {
            let o = self.plants.observe();
            let gone: Vec<u64> = o
                .tissues
                .iter()
                .filter(|t| t.organism == organism)
                .map(|t| t.id)
                .collect();
            self.kill(&o, &gone);
            self.plants.remove_many(&gone).unwrap();
            self.wood.pulled.retain(|id| !gone.contains(id));
            self.wood.uprooted += 1;
        }
    }

    /// Dead wood and dead roots that have fallen out of the bed are cleared
    /// away.
    pub(super) fn sweep(&mut self) {
        let floor = self.at([0, 0]).y - 0.5 - LOST_BELOW;
        let lost = |world: &PhysicsWorld, body: BodyId| {
            world.motion(body).is_none_or(|m| m.position.y < floor)
        };
        let mut cleared = 0;
        let world = &mut self.world;
        self.wood.dead.retain(|d| {
            let gone = lost(world, d.segment.body());
            if gone {
                world.remove_entity(d.segment.entity);
                cleared += 1;
            }
            !gone
        });
        self.wood.dead_plates.retain(|d| {
            let gone = lost(world, d.plate.body());
            if gone {
                world.remove_entity(d.plate.entity);
            }
            !gone
        });
        self.wood.work.cleared += cleared;
    }

    /// The anchor of each living tree's gripping roots, as the last step left
    /// it.
    pub fn anchors(&self) -> Vec<Anchor> {
        let o = self.plants.observe();
        let cells: BTreeMap<u64, [i32; 2]> = o.tissues.iter().map(|t| (t.id, t.cell)).collect();
        self.wood
            .plates
            .values()
            .flat_map(|plate| &plate.anchors)
            .filter_map(|(&tissue, &joint)| {
                let state = self.world.connection(joint)?;
                Some(Anchor {
                    tissue,
                    cell: *cells.get(&tissue)?,
                    utilization: state.load.utilization,
                    pull: [state.load.axial, state.load.shear],
                })
            })
            .collect()
    }

    /// How far each living tree's root plate has turned from where it was
    /// built, in radians, counterclockwise, by its organism.
    pub fn leans(&self) -> BTreeMap<u64, f32> {
        self.wood
            .plates
            .iter()
            .filter_map(|(&organism, plate)| Some((organism, self.plate_moved(plate)?.turn())))
            .collect()
    }

    /// The living trees' roots that hold: how many anchors, and the most
    /// loaded; then the wind, with its push while that rises or drops, the
    /// anchors that gave way and the trees uprooted.
    pub fn roots_summary(&self) -> String {
        let anchors = self.anchors();
        let held = anchors
            .iter()
            .max_by(|a, b| a.utilization.total_cmp(&b.utilization))
            .map_or("No living roots hold".into(), |a| {
                format!(
                    "Roots on {} anchor{}, most loaded {:.2} of safe at {:?}",
                    anchors.len(),
                    if anchors.len() == 1 { "" } else { "s" },
                    a.utilization,
                    a.cell
                )
            });
        let wind = self.wood.wind;
        let (name, full) = WINDS[wind.level];
        let wind = if wind.push == full {
            name.to_string()
        } else {
            format!("{name}, now {:.1}", wind.push)
        };
        format!(
            "{held} | wind {wind} | pulled {} | uprooted {}",
            self.wood.pulls, self.wood.uprooted
        )
    }

    /// Dead trees' roots where their plates lie: under inspection, and
    /// wherever a plate has left the dirt's hold.
    pub(super) fn draw_plates(&self, f: &mut RenderFrame) {
        for dead in &self.wood.dead_plates {
            if !self.inspect && !dead.plate.anchors.is_empty() {
                continue;
            }
            let moved = self.plate_moved(&dead.plate);
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
    }
}
