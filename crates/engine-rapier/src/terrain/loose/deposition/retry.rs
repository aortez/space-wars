//! Failed plans are pure query results. Reuse one only when every input is
//! unchanged; successful deposits always run fresh clearance queries.
use super::*;
use crate::world::{QueryArea, SolidDependency};

pub(super) const MAX_FAILURES: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Inputs {
    body: BodyId,
    position: Vec2,
    angle: f32,
    revision: u64,
    surface: TerrainSurface,
    groups: CollisionGroups,
    grains: Vec<PhysicsId>,
    repacking: bool,
    repose: Option<packing::Repose>,
    solids: Vec<SolidDependency>,
}

#[derive(Debug, Clone)]
pub(in super::super) struct Failure {
    pub(super) inputs: Inputs,
    pub(super) reason: Reason,
}

impl Inputs {
    pub fn capture(
        world: &PhysicsWorld,
        field: &packing::Field<'_>,
        grains: &[&TerrainGrain],
    ) -> Option<Self> {
        let _span = crate::profile::span("packing_dependencies");
        let motion = world.motion(field.body)?;
        let mut minimum = Vec2::new(f32::INFINITY, f32::INFINITY);
        let mut maximum = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for grain in grains {
            let local = (world.motion(grain.body())?.position - motion.position)
                .rotate_radians(-motion.angle);
            minimum.x = minimum.x.min(local.x);
            minimum.y = minimum.y.min(local.y);
            maximum.x = maximum.x.max(local.x);
            maximum.y = maximum.y.max(local.y);
        }
        // Candidate reach is at most 1.75 cells. Four cells cover its cell tile,
        // interpolated-surface halo and the clearance query's sliver envelope.
        let padding = Vec2::new(4.0, 4.0) * field.terrain.cell_size();
        let solids = world.current_solid_dependencies(
            motion.position,
            motion.angle,
            QueryArea {
                minimum: minimum - padding,
                maximum: maximum + padding,
                groups: field.groups,
            },
        )?;
        Some(Self {
            body: field.body,
            position: motion.position,
            angle: motion.angle,
            revision: field.terrain.revision(),
            surface: field.geometry.surface(),
            groups: field.groups,
            grains: grains.iter().map(|g| g.id()).collect(),
            repacking: field.repacking,
            repose: field.repose.clone(),
            solids,
        })
    }

    pub fn retained(&self, settling: &BTreeMap<PhysicsId, SettlingState>) -> bool {
        self.grains.iter().all(|id| settling.contains_key(id))
    }
}
