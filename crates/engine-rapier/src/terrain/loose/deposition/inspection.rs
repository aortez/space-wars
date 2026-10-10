//! Frozen, query-only packing inputs. Capture outside simulation timing; these
//! are development fixtures tied to the current physics snapshot format.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct PackingSnapshot {
    version: u32,
    physics: Vec<u8>,
    terrain: Terrain,
    surface: TerrainSurface,
    body: BodyId,
    groups: CollisionGroups,
    grains: Vec<TerrainGrain>,
    loose_ids: BTreeSet<PhysicsId>,
    terrain_ids: BTreeSet<PhysicsId>,
    repose: Option<packing::Repose>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackingPatch {
    /// World-space convex addition, rather than an envelope of the edited cell.
    pub vertices: Vec<Vec2>,
    pub clear: bool,
    pub blockers: Vec<ColliderId>,
}
#[derive(Debug, Clone, Serialize)]
pub struct PackingAttempt {
    pub stage: &'static str,
    pub grains: Vec<PhysicsId>,
    pub cells: Vec<CellCoord>,
    pub centers: Vec<Vec2>,
    pub patches: Vec<PackingPatch>,
    pub unstable_centers: Vec<Vec2>,
}
#[derive(Debug, Serialize)]
pub struct PackingInspection {
    pub body: BodyId,
    pub cell_size: f32,
    pub angle: f32,
    pub grains: Vec<PackingGrain>,
    pub terrain: Vec<Vec<Vec2>>,
    pub obstacles: Vec<PackingObstacle>,
    pub attempts: Vec<PackingAttempt>,
    /// Chosen successful surface plan; later exploratory attempts may fail.
    pub selected_attempt: Option<usize>,
    pub omitted_bodies: Vec<PhysicsId>,
    pub accepted: Vec<PhysicsId>,
    pub remaining_reason: String,
}
#[derive(Debug, Serialize)]
pub struct PackingGrain {
    pub id: PhysicsId,
    pub position: Vec2,
    pub angle: f32,
    pub radius: f32,
    pub shape: GrainShape,
}
#[derive(Debug, Serialize)]
pub struct PackingObstacle {
    pub body: BodyId,
    pub kind: &'static str,
    /// World-axis bounds, explicitly an envelope for visual identification.
    pub bounds: (Vec2, Vec2),
}

impl PackingSnapshot {
    pub fn to_bytes(&self) -> Result<Vec<u8>, TerrainError> {
        bincode::serialize(self).map_err(|_| TerrainError("packing snapshot serialization failed"))
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TerrainError> {
        let value: Self =
            bincode::deserialize(bytes).map_err(|_| TerrainError("invalid packing snapshot"))?;
        if value.version != 2 || value.grains.is_empty() || value.grains.len() > MAX_DEPOSITS {
            return Err(TerrainError("unsupported packing snapshot"));
        }
        Ok(value)
    }
    pub fn inspect(&self) -> Result<PackingInspection, TerrainError> {
        self.inspect_without(&[])
    }

    /// Counterfactual development query: remove specified obstacles from a copy
    /// of the captured world. Destination and candidate grains must remain.
    pub fn inspect_without(
        &self,
        omitted_bodies: &[PhysicsId],
    ) -> Result<PackingInspection, TerrainError> {
        let mut world = PhysicsWorld::from_snapshot_bytes(&self.physics)
            .map_err(|_| TerrainError("invalid packing physics snapshot"))?;
        for &id in omitted_bodies {
            if id == self.body.entity || self.grains.iter().any(|g| g.id() == id) {
                return Err(TerrainError("cannot omit packing destination or candidate"));
            }
            if !world.remove_entity(id) {
                return Err(TerrainError("missing packing obstacle"));
            }
        }
        let motion = world
            .motion(self.body)
            .ok_or(TerrainError("missing packing destination"))?;
        if self.grains.iter().any(|g| !world.contains_body(g.body())) {
            return Err(TerrainError("missing packing grain"));
        }
        let geometry = TerrainGeometry::with_surface(&self.terrain, self.surface);
        let field = packing::Field {
            terrain: &self.terrain,
            geometry: &geometry,
            body: self.body,
            groups: self.groups,
            repose: self.repose.clone(),
            repacking: true,
        };
        let grains: Vec<_> = self.grains.iter().collect();
        let mut attempts = Vec::new();
        let result = packing::prepare(&world, &field, &grains, Some(&mut attempts));
        let selected_attempt = result.as_ref().ok().and_then(|plan| {
            let ids: Vec<_> = plan.accepted.iter().map(|&i| grains[i].id()).collect();
            attempts.iter().rposition(|a| {
                a.cells == plan.changed
                    && a.grains == ids
                    && a.unstable_centers.is_empty()
                    && a.patches.iter().all(|p| p.clear)
            })
        });
        let (accepted, reason): (Vec<PhysicsId>, Reason) = match result {
            Ok(plan) => (
                plan.accepted.iter().map(|&i| grains[i].id()).collect(),
                plan.remaining_reason,
            ),
            Err(reason) => (Vec::new(), reason),
        };
        let transform = |p: Vec2| motion.position + p.rotate_radians(motion.angle);
        let terrain = geometry
            .chunks()
            .iter()
            .flat_map(|c| {
                c.polygons
                    .iter()
                    .map(|p| p.vertices.clone())
                    .chain(c.rectangles.iter().map(|r| {
                        let center = r.local_center(&self.terrain);
                        let half = r.half_extents(&self.terrain);
                        [
                            Vec2::new(-half.x, -half.y),
                            Vec2::new(half.x, -half.y),
                            Vec2::new(half.x, half.y),
                            Vec2::new(-half.x, half.y),
                        ]
                        .into_iter()
                        .map(|p| center + p)
                        .collect()
                    }))
            })
            .map(|p| p.into_iter().map(transform).collect())
            .collect();
        let blockers: BTreeSet<_> = attempts
            .iter()
            .flat_map(|a| &a.patches)
            .flat_map(|p| &p.blockers)
            .filter_map(|&id| world.collider_body(id))
            .collect();
        let obstacles = blockers
            .into_iter()
            .filter_map(|body| {
                Some(PackingObstacle {
                    body,
                    kind: if self.loose_ids.contains(&body.entity) {
                        "loose grain"
                    } else if self.terrain_ids.contains(&body.entity) {
                        "terrain"
                    } else {
                        "actor/body"
                    },
                    bounds: world.body_solid_bounds(body)?,
                })
            })
            .collect();
        let remaining_reason = if accepted.len() == grains.len() {
            "Complete".into()
        } else {
            format!("{reason:?}")
        };
        Ok(PackingInspection {
            body: self.body,
            cell_size: self.terrain.cell_size(),
            angle: motion.angle,
            grains: grains
                .iter()
                .map(|g| {
                    let m = world.motion(g.body()).unwrap();
                    PackingGrain {
                        id: g.id(),
                        position: m.position,
                        angle: m.angle,
                        radius: g.radius(),
                        shape: g.shape(),
                    }
                })
                .collect(),
            terrain,
            obstacles,
            attempts,
            selected_attempt,
            omitted_bodies: omitted_bodies.to_vec(),
            accepted,
            remaining_reason,
        })
    }
}

impl LooseTerrain {
    /// Capture the next bounded set of quiet, grounded candidates, ignoring
    /// retry and recovery delays for inspection only. Never advances timers or changes the
    /// world. Call outside the timed step; snapshots include current colliders.
    pub fn capture_packing<'a>(
        &self,
        world: &PhysicsWorld,
        destinations: impl IntoIterator<Item = TerrainBodyMut<'a>>,
    ) -> Result<Vec<PackingSnapshot>, TerrainError> {
        let gravity = world.gravity();
        self.capture_packing_with_gravity(world, destinations, |_, _| gravity)
    }

    /// Capture using the scenario's force law, as in `settle_with_gravity`.
    pub fn capture_packing_with_gravity<'a>(
        &self,
        world: &PhysicsWorld,
        destinations: impl IntoIterator<Item = TerrainBodyMut<'a>>,
        mut gravity: impl FnMut(BodyId, Vec2) -> Vec2,
    ) -> Result<Vec<PackingSnapshot>, TerrainError> {
        let fields: BTreeMap<_, _> = destinations
            .into_iter()
            .map(|f| (f.assembly.body(), f))
            .collect();
        if fields
            .iter()
            .any(|(id, f)| !world.contains_body(*id) || !f.geometry.is_current(f.terrain))
            || self.grains.iter().any(|g| !world.contains_body(g.body()))
        {
            return Err(TerrainError("invalid packing inputs"));
        }
        let graph = support::ContactGraph::new(world, &self.grains, &fields);
        let ready = graph.grounded(
            &self
                .grains
                .iter()
                .enumerate()
                .filter_map(|(i, g)| {
                    self.settling
                        .get(&g.id())
                        .filter(|s| {
                            s.seconds >= QUIET_SECONDS
                                && graph.destination[i] == Some(s.destination)
                        })
                        .map(|_| i)
                })
                .collect(),
        );
        let mut seeds: Vec<_> = ready
            .iter()
            .copied()
            .filter(|&i| graph.roots[i].contains(&graph.destination[i].unwrap()))
            .collect();
        seeds.sort_by(|&a, &b| {
            self.settling[&self.grains[b].id()]
                .retry_seconds
                .total_cmp(&self.settling[&self.grains[a].id()].retry_seconds)
                .then(self.grains[a].id().cmp(&self.grains[b].id()))
        });
        let mut attempted = BTreeSet::new();
        let mut snapshots = Vec::new();
        let mut budget = MAX_DEPOSITS;
        for seed in seeds {
            if budget == 0 {
                break;
            }
            if attempted.contains(&seed) {
                continue;
            }
            let group = graph.group(seed, &ready, &attempted, budget);
            budget -= group.len();
            attempted.extend(group.iter().copied());
            let body = graph.destination[seed].unwrap();
            let f = &fields[&body];
            let grains: Vec<_> = group.iter().map(|&i| &self.grains[i]).collect();
            let packing_field = packing::Field::from(f).with_repose(
                world,
                &grains,
                self.config.slumping,
                &mut gravity,
            );
            snapshots.push(PackingSnapshot {
                version: 2,
                physics: world
                    .snapshot_bytes()
                    .map_err(|_| TerrainError("packing capture failed"))?,
                terrain: f.terrain.clone(),
                surface: f.geometry.surface(),
                body,
                groups: f.assembly.spec.collision_groups,
                grains: group.iter().map(|&i| self.grains[i].clone()).collect(),
                loose_ids: self.grains.iter().map(|g| g.id()).collect(),
                terrain_ids: fields.keys().map(|b| b.entity).collect(),
                repose: packing_field.repose,
            });
        }
        Ok(snapshots)
    }
}

pub(super) fn attempt(
    world: &PhysicsWorld,
    field: &packing::Field<'_>,
    grains: &[&TerrainGrain],
    cells: &[CellCoord],
    patches: &[Vec<Vec2>],
    stage: &'static str,
) -> PackingAttempt {
    let motion = world.motion(field.body).unwrap();
    let transform = |p: Vec2| motion.position + p.rotate_radians(motion.angle);
    let excluded: Vec<_> = std::iter::once(field.body.entity)
        .chain(grains.iter().map(|g| g.id()))
        .collect();
    let results = world.current_polygon_obstacles(
        motion.position,
        motion.angle,
        patches,
        field.groups,
        &excluded,
    );
    PackingAttempt {
        stage,
        unstable_centers: Vec::new(),
        grains: grains.iter().map(|g| g.id()).collect(),
        cells: cells.to_vec(),
        centers: cells
            .iter()
            .map(|&c| transform(field.terrain.cell_center(c)))
            .collect(),
        patches: patches
            .iter()
            .zip(results)
            .map(|(p, (clear, blockers))| PackingPatch {
                vertices: p.iter().copied().map(transform).collect(),
                clear,
                blockers,
            })
            .collect(),
    }
}

#[cfg(test)]
mod oracle;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_a_captured_actor_is_a_query_only_comparison() {
        for (bytes, actor, grains, accepted) in [
            (
                include_bytes!("../../../../tests/fixtures/scorched-angular-actor.packing")
                    .as_slice(),
                11,
                50,
                50,
            ),
            (
                include_bytes!("../../../../tests/fixtures/spacewars-round-roof-actor.packing")
                    .as_slice(),
                10000,
                9,
                3,
            ),
        ] {
            let snapshot = PackingSnapshot::from_bytes(bytes).unwrap();
            let before = snapshot.to_bytes().unwrap();
            let ordinary = snapshot.inspect().unwrap();
            assert_eq!(ordinary.grains.len(), grains);
            assert!(ordinary.accepted.is_empty());
            let actor = PhysicsId::new(actor);
            assert!(
                ordinary
                    .obstacles
                    .iter()
                    .any(|o| o.body.entity == actor && o.kind == "actor/body")
            );
            let without = snapshot.inspect_without(&[actor]).unwrap();
            assert_eq!(without.accepted.len(), accepted);
            assert_eq!(without.omitted_bodies, [actor]);
            assert!(!without.obstacles.iter().any(|o| o.body.entity == actor));
            let chosen = &without.attempts[without.selected_attempt.unwrap()];
            assert!(
                chosen
                    .patches
                    .iter()
                    .all(|p| p.clear && p.blockers.is_empty())
            );
            assert!(chosen.unstable_centers.is_empty());
            for invalid in [
                snapshot.body.entity,
                snapshot.grains[0].id(),
                PhysicsId::new(u64::MAX),
            ] {
                assert!(snapshot.inspect_without(&[invalid]).is_err());
            }
            assert_eq!(snapshot.to_bytes().unwrap(), before);
            assert_eq!(
                bincode::serialize(&snapshot.inspect().unwrap()).unwrap(),
                bincode::serialize(&ordinary).unwrap()
            );
        }
    }

    #[test]
    fn captured_piles_pack_supported_cells_without_rebuilding_an_unstable_bank() {
        for (bytes, minimum) in [
            (
                include_bytes!("../../../../tests/fixtures/scorched-round-repose.packing")
                    .as_slice(),
                32,
            ),
            (
                include_bytes!("../../../../tests/fixtures/scorched-angular-repose.packing")
                    .as_slice(),
                48,
            ),
            (
                include_bytes!("../../../../tests/fixtures/spacewars-round-curved.packing")
                    .as_slice(),
                56,
            ),
        ] {
            let snapshot = PackingSnapshot::from_bytes(bytes).unwrap();
            let report = snapshot.inspect().unwrap();
            assert!(
                report.accepted.len() >= minimum,
                "{}: {} accepted",
                report.remaining_reason,
                report.accepted.len()
            );
            assert!(
                report
                    .attempts
                    .iter()
                    .filter(|a| a.stage == "surface additions")
                    .count()
                    <= 8
            );
            let last = &report.attempts[report.selected_attempt.unwrap()];
            assert!(last.unstable_centers.is_empty());
            assert!(
                last.patches
                    .iter()
                    .all(|p| p.clear && p.blockers.is_empty())
            );

            let world = PhysicsWorld::from_snapshot_bytes(&snapshot.physics).unwrap();
            let geometry = TerrainGeometry::with_surface(&snapshot.terrain, snapshot.surface);
            let field = packing::Field {
                terrain: &snapshot.terrain,
                geometry: &geometry,
                body: snapshot.body,
                groups: snapshot.groups,
                repose: snapshot.repose.clone(),
                repacking: true,
            };
            let grains: Vec<_> = snapshot.grains.iter().collect();
            let plan = packing::prepare(&world, &field, &grains, None).unwrap();
            assert!(snapshot.repose.is_some());
            assert_eq!(plan.changed.len(), plan.accepted.len());
            for (&i, &c) in plan.accepted.iter().zip(&plan.changed) {
                assert_eq!(snapshot.terrain.cell(c), Some(Cell::VOID));
                assert_eq!(plan.terrain.cell(c), Some(grains[i].cell()));
                // Space Wars supplies a sampled planetary field rather than
                // Rapier's uniform world gravity. Check the captured rule.
                let repose = field.repose.as_ref().unwrap();
                assert!(!super::super::super::slumping::oversteep(
                    &plan.terrain,
                    c,
                    repose.gravity[&(c.y, c.x)],
                    repose.degrees,
                ));
            }
        }
    }

    #[test]
    fn captured_angular_pile_repacks_around_tank_and_unselected_grains() {
        // PR #171: seed 42, 60 seconds of barrage followed by 60 seconds quiet,
        // Angular/collapse enabled. The old shrinking planner accepted zero.
        let bytes = include_bytes!("../../../../tests/fixtures/scorched-angular-pile.packing");
        let snapshot = PackingSnapshot::from_bytes(bytes).unwrap();
        let before = snapshot.to_bytes().unwrap();
        let report = snapshot.inspect().unwrap();
        assert!(
            report.accepted.len() >= 22,
            "retain the existing recovery fallback"
        );
        assert!(report.accepted.len() < report.grains.len());
        assert!(
            report
                .attempts
                .iter()
                .filter(|a| a.stage == "surface additions")
                .count()
                <= 8
        );
        assert!(report.obstacles.iter().any(|o| o.kind == "actor/body"));
        assert!(report.obstacles.iter().any(|o| o.kind == "loose grain"));
        assert!(
            report.attempts[report.selected_attempt.unwrap()]
                .patches
                .iter()
                .all(|p| p.clear && p.blockers.is_empty())
        );
        assert_eq!(snapshot.to_bytes().unwrap(), before);
        let replay = PackingSnapshot::from_bytes(&before)
            .unwrap()
            .inspect()
            .unwrap();
        assert_eq!(
            bincode::serialize(&report).unwrap(),
            bincode::serialize(&replay).unwrap()
        );

        let world = PhysicsWorld::from_snapshot_bytes(&snapshot.physics).unwrap();
        let geometry = TerrainGeometry::with_surface(&snapshot.terrain, snapshot.surface);
        let field = packing::Field {
            terrain: &snapshot.terrain,
            geometry: &geometry,
            body: snapshot.body,
            groups: snapshot.groups,
            repose: snapshot.repose.clone(),
            repacking: true,
        };
        let grains: Vec<_> = snapshot.grains.iter().collect();
        let plan = packing::prepare(&world, &field, &grains, None).unwrap();
        assert_eq!(plan.changed.len(), plan.accepted.len());
        for (&i, &coordinate) in plan.accepted.iter().zip(&plan.changed) {
            assert_eq!(snapshot.terrain.cell(coordinate), Some(Cell::VOID));
            assert_eq!(plan.terrain.cell(coordinate), Some(grains[i].cell()));
        }
        assert!(plan.geometry.is_current(&plan.terrain));
    }
}
