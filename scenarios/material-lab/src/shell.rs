//! The shell case on ordinary steps: a shell flies through three tufts of moss
//! into a fixed armor plate and bursts where it stops. One weapon source
//! charges the moss it passes, the plate it hits and what its blast reaches;
//! the plate shadows the tuft behind it.
use crate::{MaterialLabState, PHYSICS_DT, Phase, scene};
use engine_core::Vec2;
use engine_core::plants::{Config, Environment, Exposure, Plants, Role, Support, Tissue, TissueId};
use engine_material::{
    ARMOR, Outcome, Part, Region, SHELL, SOFT_TISSUE, Section, SourceId, SourceLedger,
    TissueIntegrity, TissueMaterial, TissueTable, WOOD,
};
use engine_rapier::world::{
    BodyKind, BodySpec, ColliderSpec, PhysicsWorld,
    loading::response::ResponseRejection,
    loading::{ContactSource, RegionId, RegionToken},
    weapon::{Blast, Reachable, Recipient, Recipients, ShotSpec, WeaponEvent, Weapons},
};
use serde::Serialize;

/// The shell starts at the origin and flies along +x. Each step moves it
/// exactly 25/512.
pub const SPEED: f32 = 200.0;
pub const SHELL_RADIUS: f32 = 0.25;
pub const SHELL_MASS: f32 = 0.5;
/// The payload: its energy, how far its blast reaches and its front's height.
pub const PAYLOAD: Blast = Blast {
    energy: 5000.0,
    radius: 6.0,
    height: 1.0,
};
/// The fixed plate's center and half extents. Its face is at x = 5.5.
pub const PLATE_CENTER: Vec2 = Vec2::new(6.0, 0.0);
pub const PLATE_HALF: Vec2 = Vec2::new(0.5, 2.0);
/// Three tufts of moss on the shell's path, one in the open above it and one
/// behind the plate.
pub const TUFTS: [Vec2; 5] = [
    Vec2::new(1.5, 0.0),
    Vec2::new(2.5, 0.0),
    Vec2::new(3.5, 0.0),
    Vec2::new(4.5, 2.5),
    Vec2::new(7.5, 0.0),
];
/// How many of the tufts stand on the shell's path.
pub const PATH: usize = 3;
pub const TUFT_RADIUS: f32 = 0.1;
const TUFT: Section = Section::new(0.2, 0.1);
pub(crate) const PLATE_SECTION: Section = Section::new(1.0, 0.5);
/// The shell's own section, which its hit's patch meets the plate with.
pub(crate) const CASING: Section = Section::new(0.5, 0.1);
/// Predicted contacts stop the shell at the plate's face within the step that
/// reaches it; each step moves it less than this.
const PREDICTION: f32 = 0.1;
const SOURCE: SourceId = 1;
/// The run ends here if the shell has not burst.
pub const WINDOW: u32 = 160;
/// Body roles: the shell, then the plate.
pub(crate) const SHELL_BODY: u64 = 0;
pub(crate) const PLATE: u64 = 1;

/// The plate's armor, at full capacity.
pub fn armor() -> Region {
    Region::new(ARMOR, 4.0).unwrap()
}

/// The shell's own material, which its hit also charges.
pub(crate) fn casing() -> Region {
    Region::new(SHELL, 0.05).unwrap()
}

/// The plate, with its armor registered and bound, and the shell flying
/// toward it. The shell is bound to no region.
pub(crate) fn build() -> (PhysicsWorld, RegionToken) {
    let mut world = scene::empty_world(Vec2::ZERO);
    let plate = scene::body(PLATE);
    let mut shape = ColliderSpec::cuboid(scene::collider(PLATE), PLATE_HALF.x, PLATE_HALF.y);
    shape.friction = 0.0;
    assert!(world.insert_body(
        plate,
        BodySpec {
            kind: BodyKind::Fixed,
            position: PLATE_CENTER,
            can_sleep: false,
            ..Default::default()
        },
        &[shape],
    ));
    let token = world
        .create_loading_region(
            RegionId {
                entity: plate.entity,
                part: 0,
            },
            plate,
        )
        .unwrap();
    world
        .register_loading_material(token, armor(), PLATE_SECTION)
        .unwrap();
    world
        .bind_loading_collider(
            scene::collider(PLATE),
            ContactSource {
                region: token,
                patch: 0,
            },
        )
        .unwrap();
    let mut ball = ColliderSpec::ball(scene::collider(SHELL_BODY), SHELL_RADIUS);
    ball.density = SHELL_MASS / (std::f32::consts::PI * SHELL_RADIUS * SHELL_RADIUS);
    ball.friction = 0.0;
    assert!(world.insert_body(
        scene::body(SHELL_BODY),
        BodySpec {
            linear_velocity: Vec2::X * SPEED,
            can_sleep: false,
            soft_ccd_prediction: PREDICTION,
            ..Default::default()
        },
        &[ball],
    ));
    (world, token)
}

struct Meadow;

impl Environment for Meadow {
    fn exposure(&self, _: &Tissue) -> Exposure {
        Exposure {
            dirt_contact: true,
            surface_contact: true,
            nutrients: 1000,
            water: 1000,
            light: 1000,
        }
    }

    fn space_available(&self, _: [i32; 2], _: Role, _: u16) -> bool {
        true
    }

    fn support_valid(&self, _: Option<Support>, _: [i32; 2]) -> bool {
        true
    }
}

/// The lab's side of each delivery: the moss, kept by the tissue adapter, and
/// the circles it admits to the shell's sweeps and blast. A cut tuft is lost,
/// so nothing meets it again.
#[derive(Debug)]
struct Field {
    integrity: TissueIntegrity,
    tufts: Vec<TissueId>,
    circles: Vec<Reachable>,
}

impl Field {
    fn new(plate: RegionToken) -> Self {
        let mut plants = Plants::new(Config::default(), 3).unwrap();
        let tufts: Vec<TissueId> = (0..TUFTS.len() as i32)
            .map(|x| {
                plants
                    .seed(1, Role::Moss, 1, [x, 0], None, [0; 3], &Meadow)
                    .unwrap()
            })
            .collect();
        let soft = TissueMaterial {
            material: SOFT_TISSUE,
            volume_per_size: 0.02,
        };
        let table = TissueTable {
            identity: 1,
            roles: [
                TissueMaterial {
                    material: WOOD,
                    ..soft
                },
                soft,
                soft,
                soft,
            ],
        };
        let mut circles: Vec<Reachable> = tufts
            .iter()
            .zip(TUFTS)
            .map(|(id, center)| Reachable {
                recipient: Recipient::Tissue(*id),
                center,
                radius: TUFT_RADIUS,
                thickness: TUFT.thickness,
            })
            .collect();
        // The plate meets the blast as a column of four circles across its
        // face, each as thick as the plate.
        circles.extend([-1.5, -0.5, 0.5, 1.5].map(|y| Reachable {
            recipient: Recipient::Region(plate),
            center: Vec2::new(PLATE_CENTER.x, y),
            radius: PLATE_HALF.x,
            thickness: PLATE_SECTION.thickness,
        }));
        Field {
            integrity: TissueIntegrity::new(table, &plants).unwrap(),
            tufts,
            circles,
        }
    }

    fn index(&self, key: TissueId) -> Option<usize> {
        self.tufts.iter().position(|tuft| *tuft == key)
    }

    /// Whether a tuft has been cut.
    fn cut(&self, key: TissueId) -> bool {
        self.integrity.region(key).is_some_and(Region::failed)
    }

    fn admits(&self, recipient: Recipient) -> bool {
        match recipient {
            Recipient::Tissue(key) => self.index(key).is_some() && !self.cut(key),
            _ => true,
        }
    }
}

impl Recipients for Field {
    fn near(&self, center: Vec2, radius: f32) -> Vec<Reachable> {
        self.circles
            .iter()
            .filter(|circle| {
                self.admits(circle.recipient)
                    && circle.center.distance_to(center) - circle.radius <= radius
            })
            .copied()
            .collect()
    }

    fn tissue(&self, key: u64) -> Option<(Region, Section)> {
        if !self.admits(Recipient::Tissue(key)) {
            return None;
        }
        Some((*self.integrity.region(key)?, TUFT))
    }

    fn absorb(&mut self, key: u64, work: f32) {
        self.integrity.charge(key, work).unwrap();
    }
}

/// How far the shell has come.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum ShellStage {
    /// The shell flies through the moss; the lab pauses once it has cut the
    /// last tuft on its path.
    #[default]
    Moss,
    /// The moss on its path is cut, and the shell flies on toward the plate.
    Clear,
    /// The shell has stopped on the plate and burst. Its source is retired.
    Spent,
}

/// The shell case's source and what it reached. Other cases have none.
#[derive(Debug)]
pub(crate) struct Range {
    weapons: Weapons,
    field: Field,
    stage: ShellStage,
    /// Each tuft's work from the shell's pass and from its blast.
    passes: Vec<Option<f32>>,
    blasts: Vec<Option<f32>>,
    /// The hard hit on the plate, and the plate's work from the blast.
    hit: Option<Outcome>,
    plate_blast: Option<f32>,
    /// Where the shell struck the plate, and where it burst.
    struck_at: Option<Vec2>,
    burst: Option<Vec2>,
    /// The ledger the source retired with.
    retired: Option<SourceLedger>,
    /// Response batches the shell's arrival refused.
    refused: u32,
}

impl Range {
    pub(crate) fn new(world: &PhysicsWorld, plate: RegionToken) -> Self {
        let mut weapons = Weapons::new();
        weapons
            .launch(
                world,
                ShotSpec {
                    id: SOURCE,
                    owner: None,
                    body: scene::body(SHELL_BODY),
                    region: casing(),
                    section: CASING,
                    radius: SHELL_RADIUS,
                    lifetime: 1.0,
                    blast: Some(PAYLOAD),
                    protected: None,
                },
            )
            .unwrap();
        Range {
            weapons,
            field: Field::new(plate),
            stage: ShellStage::default(),
            passes: vec![None; TUFTS.len()],
            blasts: vec![None; TUFTS.len()],
            hit: None,
            plate_blast: None,
            struck_at: None,
            burst: None,
            retired: None,
            refused: 0,
        }
    }

    /// Records what one step's events delivered, and returns the history
    /// labels they earn, in order.
    fn take(&mut self, events: Vec<WeaponEvent>) -> Vec<String> {
        let mut labels = Vec::new();
        for event in events {
            match event {
                WeaponEvent::Delivered {
                    part,
                    recipient,
                    outcome,
                    ..
                } => match (recipient, part) {
                    (Recipient::Tissue(key), Part::Kinetic) => {
                        let i = self.field.index(key).unwrap();
                        self.passes[i] = Some(outcome.consumed[0]);
                    }
                    (Recipient::Tissue(key), _) => {
                        let i = self.field.index(key).unwrap();
                        self.blasts[i] = Some(outcome.consumed[0]);
                    }
                    (_, Part::Kinetic) => {
                        self.hit = Some(outcome);
                        labels.push("Hit the plate".into());
                    }
                    (_, _) => self.plate_blast = Some(outcome.consumed[0]),
                },
                WeaponEvent::Stopped { point, .. } => self.struck_at = Some(point),
                WeaponEvent::Burst { center, .. } => {
                    self.burst = Some(center);
                    labels.push("Burst".into());
                }
                WeaponEvent::Retired { ledger, .. } => self.retired = Some(ledger),
                WeaponEvent::Excluded { .. }
                | WeaponEvent::Refused { .. }
                | WeaponEvent::Traced { .. } => {}
            }
        }
        for key in self.field.integrity.take_failures() {
            let i = self.field.index(key).unwrap();
            labels.push(format!("Cut M{}", i + 1));
        }
        labels
    }
}

/// One tuft of moss, as the source left it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Tuft {
    pub center: Vec2,
    pub remaining: f32,
    pub capacity: f32,
    /// Work the shell's pass left in it.
    pub pass: Option<f32>,
    /// Work the blast left in it.
    pub blast: Option<f32>,
    pub cut: bool,
}

/// The shell, its source's ledger and every recipient, read from the run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shell {
    pub stage: ShellStage,
    /// Where the shell is and how fast it flies, until it stops.
    pub position: Option<Vec2>,
    pub speed: Option<f32>,
    pub ledger: SourceLedger,
    pub tufts: Vec<Tuft>,
    pub hit: Option<Outcome>,
    pub plate_blast: Option<f32>,
    pub plate_remaining: f64,
    pub plate_capacity: f64,
    /// Where the shell struck the plate, and where it burst.
    pub struck_at: Option<Vec2>,
    pub burst: Option<Vec2>,
    /// Response batches the shell's arrival refused. The shell is bound to
    /// no region, so the contact response charges nothing for it.
    pub refused: u32,
}

impl MaterialLabState {
    /// One ordinary step. The shell's arrival is the only contact the
    /// response may refuse; any other refusal stops the run before the step
    /// changes anything.
    pub(crate) fn advance_shell(&mut self) {
        let mut world = self.world.clone();
        let range = self.range.as_ref().unwrap();
        let prepared = range.weapons.prepare(&world, PHYSICS_DT);
        world.step(PHYSICS_DT);
        let mut refused = 0;
        for batch in world.material_response_batches() {
            match batch.result {
                Ok(_) => {}
                Err(ResponseRejection::UnboundContact) => refused += 1,
                Err(reason) => {
                    return self.stop(format!(
                        "Run stopped: material response refused ({reason:?}). Last valid state retained."
                    ));
                }
            }
        }
        let range = self.range.as_mut().unwrap();
        range
            .weapons
            .settle(prepared, &mut world, PHYSICS_DT, &mut range.field);
        range.refused += refused;
        let events = range.weapons.take_events();
        let labels = range.take(events);
        let hit = range.hit.is_some();
        if range.stage == ShellStage::Moss && range.passes[..PATH].iter().all(Option::is_some) {
            range.stage = ShellStage::Clear;
            self.paused = true;
        }
        let spent = range.retired.is_some();
        if spent {
            range.stage = ShellStage::Spent;
        }
        self.world = world;
        self.steps += 1;
        self.impacts = u32::from(hit);
        let at = self.elapsed();
        for label in labels {
            self.record(at, &label);
        }
        if spent || self.steps == WINDOW {
            self.stop("Run complete — restart or choose another case".into());
        }
    }
    pub(crate) fn shell_phase(&self) -> Phase {
        if self.range.as_ref().unwrap().stage == ShellStage::Spent {
            Phase::Spent
        } else {
            Phase::Flight
        }
    }
    /// The next step to take. A spent shell ends the run, so the run's
    /// reason replaces this.
    pub(crate) fn shell_guidance(&self) -> &'static str {
        if self.range.as_ref().unwrap().stage == ShellStage::Moss {
            "Run to fire the shell; the lab pauses once it has cut the last tuft on its path."
        } else {
            "The shell kept its speed. Run to strike the plate."
        }
    }
    /// The shell case's source, shell and recipients; other cases have none.
    pub fn shell(&self) -> Option<Shell> {
        let range = self.range.as_ref()?;
        let motion = self.world.motion(scene::body(SHELL_BODY));
        let tufts = range
            .field
            .tufts
            .iter()
            .enumerate()
            .map(|(i, key)| {
                let region = range.field.integrity.region(*key).unwrap();
                Tuft {
                    center: TUFTS[i],
                    remaining: region.remaining(),
                    capacity: region.capacity(),
                    pass: range.passes[i],
                    blast: range.blasts[i],
                    cut: range.field.cut(*key),
                }
            })
            .collect();
        Some(Shell {
            stage: range.stage,
            position: motion.map(|m| m.position),
            speed: motion.map(|m| m.linear_velocity.length()),
            ledger: range
                .retired
                .unwrap_or_else(|| range.weapons.source(SOURCE).unwrap().ledger()),
            tufts,
            hit: range.hit,
            plate_blast: range.plate_blast,
            plate_remaining: self.remaining(),
            plate_capacity: self.capacity(),
            struck_at: range.struck_at,
            burst: range.burst,
            refused: range.refused,
        })
    }
}
