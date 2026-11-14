//! The split case on the ordinary native response path: an armor striker
//! charges a fixed wood block, the lab splits the block's back quarter off onto
//! a fragment, rebuilds the rest on a replacement body and strikes it again.
use crate::{Case, Event, MaterialLabState, PHYSICS_DT, scene};
use engine_core::Vec2;
use engine_material::{Region, Section, WOOD};
use engine_rapier::world::{
    PhysicsWorld,
    loading::response::split::RegionPiece,
    loading::{ContactSource, EpisodeTransition, RegionId, RegionToken},
};
use serde::Serialize;

/// From this gap the striker moves 1/256 per step, farther than the solver's
/// contact prediction distance, so it arrives exactly touching after 16 steps.
/// A slower striker meets a speculative contact first, which the response
/// refuses while the solver brakes it.
pub const GAP: f32 = 1.0 / 16.0;
pub const STRIKER_SPEED: f32 = 16.0;
/// The striker's motion energy, 256, charges about a fifth of the whole wood.
pub const STRIKER_MASS: f32 = 2.0;
/// The fragment takes the back quarter of the wood's volume.
pub const FRACTION: f32 = 0.25;
const VOLUME: f32 = 20.0;
/// Body roles the case adds: the fragment, then the wood's replacement.
const FRAGMENT: u64 = 2;
const REPLACEMENT: u64 = 3;

/// The undamaged wood block.
pub fn whole() -> Region {
    Region::new(WOOD, VOLUME).unwrap()
}
/// The fragment and the kept wood, as the split divides the volume.
fn parts() -> (Region, Region) {
    whole().split(FRACTION).unwrap()
}

/// What the case has done to the wood. Each edit happens at the start of the
/// step after a pause, so a replay repeats it at the same step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Stage {
    /// The striker approaches, strikes and leaves; the lab pauses when the
    /// contact ends.
    #[default]
    Strike,
    /// The next step splits the back quarter off onto a fragment.
    Struck,
    /// The next step moves the kept wood to a replacement body.
    Split,
    /// The next step returns the striker to its start and launches it again.
    Rebuilt,
    /// The striker charges the rebuilt wood; the run ends when it leaves.
    Restrike,
}

/// The split case's progress. Other cases keep it at its start.
#[derive(Debug, Clone, Default)]
pub(crate) struct Progress {
    stage: Stage,
    fragment: Option<RegionToken>,
    /// What each strike charged the wood, in order.
    charges: Vec<f64>,
    /// The wood's balance when it was split.
    before_split: Option<f64>,
    /// The kept wood's and the fragment's balances just after the split.
    parts: Option<[f64; 2]>,
    /// The kept wood's balance just after it moved to its replacement body.
    moved: Option<f64>,
}

/// One block: where it is, how it moves and the material it holds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SplitBlock {
    pub material: &'static str,
    /// The physics entity that holds the region; a rebuild changes it.
    pub body: u64,
    /// Extent of its collider along the strike.
    pub left: f32,
    pub right: f32,
    pub velocity: f32,
    pub volume: f32,
    pub remaining: f64,
    pub capacity: f64,
}

/// The striker, the wood that stays and the fragment, with the wood's history.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Split {
    pub stage: Stage,
    pub striker: SplitBlock,
    pub wood: SplitBlock,
    pub fragment: Option<SplitBlock>,
    /// What each strike charged the wood, in order.
    pub charges: Vec<f64>,
    /// The wood's balance when it was split. Its two parts never hold more.
    pub before_split: Option<f64>,
    /// The kept wood's and the fragment's balances just after the split.
    pub parts: Option<[f64; 2]>,
    /// The kept wood's balance just after it moved to its replacement body.
    pub moved: Option<f64>,
}

/// Split the back quarter off onto a fragment set 0.25 behind the wood. The
/// wood keeps its struck face, and each part deforms only as deep as it
/// reaches behind that face.
fn split_off(world: &mut PhysicsWorld, wood: RegionToken) -> Result<RegionToken, String> {
    scene::block(world, FRAGMENT, Vec2::X * 1.125, Vec2::new(0.125, 1.0));
    let body = scene::body(FRAGMENT);
    let piece = RegionPiece {
        id: RegionId {
            entity: body.entity,
            part: 0,
        },
        body,
        fraction: FRACTION,
        section: Section::new(1.0, 0.25),
        colliders: Vec::new(),
    };
    let fragment = world
        .split_loading_region(wood, Section::new(1.0, 0.75), &piece)
        .map_err(|e| format!("split refused ({e:?})"))?;
    world
        .bind_loading_collider(
            scene::collider(FRAGMENT),
            ContactSource {
                region: fragment,
                patch: 0,
            },
        )
        .map_err(|e| format!("fragment binding refused ({e:?})"))?;
    if !world.set_cuboid(
        scene::collider(1),
        Vec2::new(0.375, 1.0),
        Vec2::new(-0.125, 0.0),
    ) {
        return Err("the wood could not be reshaped".into());
    }
    Ok(fragment)
}

/// Move the kept wood to a replacement body of the same shape, bind its
/// collider, then remove the old body. Removing the old body first would retire
/// the region and its balance.
fn rebuild(world: &mut PhysicsWorld, wood: RegionToken) -> Result<RegionToken, String> {
    scene::block(world, REPLACEMENT, Vec2::X * 0.375, Vec2::new(0.375, 1.0));
    let body = scene::body(REPLACEMENT);
    let moved = world
        .transfer_loading_region(
            wood,
            RegionId {
                entity: body.entity,
                part: 0,
            },
            body,
        )
        .map_err(|e| format!("transfer refused ({e:?})"))?;
    world
        .bind_loading_collider(
            scene::collider(REPLACEMENT),
            ContactSource {
                region: moved,
                patch: 0,
            },
        )
        .map_err(|e| format!("replacement binding refused ({e:?})"))?;
    if !world.remove_entity(scene::body(1).entity) {
        return Err("the old wood body could not be removed".into());
    }
    Ok(moved)
}

/// Return the striker to its start and launch it again.
pub(crate) fn relaunch(world: &mut PhysicsWorld) {
    let striker = scene::body(0);
    world.set_pose(striker, Vec2::X * (-0.5 - GAP), 0.0, true);
    world.set_velocity(striker, Vec2::X * STRIKER_SPEED, 0.0, true);
}

impl MaterialLabState {
    /// One step of the split case. It runs on a copy of the world, which
    /// replaces the world only if every edit and material response succeeded;
    /// a refused response would otherwise leave ordinary physics unaccounted.
    pub(crate) fn advance_split(&mut self) {
        let at = self.elapsed();
        let mut world = self.world.clone();
        let mut wood = self.materials[1].unwrap();
        let mut progress = self.progress.clone();
        let edit = match progress.stage {
            Stage::Struck => {
                progress.before_split = world.loading_material_remaining(wood);
                split_off(&mut world, wood).map(|fragment| {
                    let kept = world.loading_material_remaining(wood);
                    let piece = world.loading_material_remaining(fragment);
                    progress.parts = kept.zip(piece).map(|(k, p)| [k, p]);
                    progress.fragment = Some(fragment);
                    progress.stage = Stage::Split;
                    Some("Split")
                })
            }
            Stage::Split => rebuild(&mut world, wood).map(|moved| {
                progress.moved = world.loading_material_remaining(moved);
                wood = moved;
                progress.stage = Stage::Rebuilt;
                Some("Rebuilt")
            }),
            Stage::Rebuilt => {
                relaunch(&mut world);
                progress.stage = Stage::Restrike;
                Ok(None)
            }
            Stage::Strike | Stage::Restrike => Ok(None),
        };
        let edit = match edit {
            Ok(edit) => edit,
            Err(reason) => {
                return self.stop(format!("Run stopped: {reason}. Last valid state retained."));
            }
        };
        world.step(PHYSICS_DT);
        let responses = world.material_response_batches();
        if let Some(reason) = responses.iter().find_map(|b| b.result.as_ref().err()) {
            return self.stop(format!(
                "Run stopped: material response refused ({reason:?}). Last valid state retained."
            ));
        }
        for impact in responses
            .iter()
            .filter_map(|b| b.result.as_ref().ok())
            .flat_map(|r| &r.impacts)
        {
            if let Some(side) = impact.sources.iter().position(|s| s.region == wood) {
                progress
                    .charges
                    .push(impact.before[side] - impact.after[side]);
            }
        }
        // Capture times are offsets within this step.
        let mut offset = 0.0;
        let mut transitions = Vec::new();
        for batch in world.loading_batches() {
            transitions.extend(batch.events.iter().map(|e| (offset, e.transition)));
            offset += f64::from(batch.duration);
        }
        self.world = world;
        self.materials[1] = Some(wood);
        self.steps += 1;
        if let Some(label) = edit {
            self.record(at, label);
            self.paused = true;
        }
        for (offset, transition) in transitions {
            let label = match transition {
                EpisodeTransition::Impact => {
                    self.impacts += 1;
                    "Impact"
                }
                EpisodeTransition::Support => "Support",
                EpisodeTransition::End => {
                    self.separations += 1;
                    match progress.stage {
                        Stage::Strike => {
                            progress.stage = Stage::Struck;
                            self.paused = true;
                        }
                        Stage::Restrike => {
                            self.stopped =
                                Some("Run complete — restart or choose another case".into());
                            self.paused = true;
                        }
                        _ => {}
                    }
                    "Separation"
                }
            };
            self.record(at + offset, label);
        }
        self.progress = progress;
        if self.stopped.is_none() && self.steps == Case::RegionSplit.parameters().2 {
            self.stop("Run complete — restart or choose another case".into());
        }
    }
    fn stop(&mut self, reason: String) {
        self.stopped = Some(reason);
        self.paused = true;
    }
    pub(crate) fn record(&mut self, time: f64, label: &str) {
        if self.events.len() == 8 {
            self.events.remove(0);
        }
        self.events.push(Event {
            time,
            label: label.into(),
            episode: self.impacts,
        });
    }
    /// The split case's stage; other cases have none.
    pub fn split_stage(&self) -> Option<Stage> {
        (self.case == Case::RegionSplit).then_some(self.progress.stage)
    }
    /// Capacity of the wood that stays: the whole block until the split.
    pub(crate) fn split_capacity(&self) -> f64 {
        let region = if self.progress.stage > Stage::Struck {
            parts().1
        } else {
            whole()
        };
        f64::from(region.capacity())
    }
    pub(crate) fn split_guidance(&self) -> &'static str {
        match self.progress.stage {
            Stage::Strike => "Run until the striker leaves the wood; the lab will pause.",
            Stage::Struck => "Run to split the back quarter off onto a fragment.",
            Stage::Split => "Run to rebuild the wood on a replacement body.",
            Stage::Rebuilt => "Run to strike the rebuilt wood again.",
            Stage::Restrike => "Run to watch the second strike; nothing was restored.",
        }
    }
    /// The split case's three blocks, read from the physical state.
    pub fn split(&self) -> Option<Split> {
        let stage = self.split_stage()?;
        let (piece, kept) = parts();
        let block = |role: u64, material, region: &Region, token: RegionToken| {
            let id = scene::body(role);
            let (min, max) = self.world.body_solid_bounds(id).unwrap();
            SplitBlock {
                material,
                body: id.entity.value(),
                left: min.x,
                right: max.x,
                velocity: self.world.motion(id).unwrap().linear_velocity.x,
                volume: region.volume(),
                remaining: self.world.loading_material_remaining(token).unwrap(),
                capacity: f64::from(region.capacity()),
            }
        };
        let striker = scene::striker();
        let wood_region = if stage > Stage::Struck { kept } else { whole() };
        let wood_role = if stage > Stage::Split { REPLACEMENT } else { 1 };
        Some(Split {
            stage,
            striker: block(
                0,
                striker.material().name,
                &striker,
                self.materials[0].unwrap(),
            ),
            wood: block(
                wood_role,
                WOOD.name,
                &wood_region,
                self.materials[1].unwrap(),
            ),
            fragment: self
                .progress
                .fragment
                .map(|token| block(FRAGMENT, WOOD.name, &piece, token)),
            charges: self.progress.charges.clone(),
            before_split: self.progress.before_split,
            parts: self.progress.parts,
            moved: self.progress.moved,
        })
    }
}
