//! Opt-in inspection for endurance runners. None of these scans run in gameplay.

use engine_rapier::world::{ColliderId, ColliderRole};

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct TerrainDiagnostics {
    /// Raw storage, excluding geometry caches and allocator overhead.
    pub cell_bytes: usize,
    pub surface_sample_bytes: usize,
    pub terrain_rectangles: usize,
    pub terrain_polygons: usize,
    pub terrain_polygon_vertices: usize,
    pub occupied_cells: u64,
    pub removed_cells: u64,
    pub fragments: usize,
    pub terrain_colliders: usize,
    pub physics_bodies: usize,
    pub physics_colliders: usize,
    pub pending_edits: usize,
    pub supported_bases: usize,
    pub cannon_hits: u64,
    pub budget_skips: u64,
    pub max_speed: f32,
    pub fastest_body: Option<String>,
    pub max_spin: f32,
    /// Same-build comparison aid, not a portable save format.
    pub motion_hash: String,
    pub issues: Vec<String>,
}

impl SpacewarsState {
    /// Scan material, cache revisions, terrain body/collider identities, and all
    /// rigid-body motions. Call between ticks; its cost is not simulation time.
    pub fn terrain_diagnostics(&self) -> TerrainDiagnostics {
        let world = &self.physics.world;
        let mut result = TerrainDiagnostics {
            cell_bytes: 0,
            surface_sample_bytes: 0,
            terrain_rectangles: 0,
            terrain_polygons: 0,
            terrain_polygon_vertices: 0,
            occupied_cells: 0,
            removed_cells: self.terrain.removed_cells,
            fragments: self.terrain.fragments.len(),
            terrain_colliders: 0,
            physics_bodies: world.body_count(),
            physics_colliders: world.collider_count(),
            pending_edits: self.terrain.pending.len(),
            supported_bases: self
                .terrain
                .planets
                .values()
                .filter(|p| p.supported && self.terrain.legacy_services)
                .count(),
            cannon_hits: self.terrain.cannon_hits,
            budget_skips: self.terrain.budget_skips,
            max_speed: 0.0,
            fastest_body: None,
            max_spin: 0.0,
            motion_hash: String::new(),
            issues: Vec::new(),
        };
        let mut expected_bodies = BTreeSet::new();
        let mut expected_colliders = BTreeSet::new();
        let fields = self
            .terrain
            .planets
            .values()
            .map(|p| (&p.field, &p.geometry, &p.assembly, p.hash))
            .chain(
                self.terrain
                    .fragments
                    .values()
                    .map(|f| (&f.terrain, &f.geometry, &f.assembly, f.hash)),
            );
        for (field, geometry, assembly, hash) in fields {
            result.cell_bytes += field.cell_bytes();
            result.surface_sample_bytes += field.surface_sample_bytes();
            let body = assembly.body();
            expected_bodies.insert(body);
            let occupied = field
                .cells()
                .iter()
                .filter(|c| c.material != MaterialId::VOID)
                .count();
            result.occupied_cells += occupied as u64;
            if hash != field.hash() {
                result.issues.push(format!(
                    "terrain {} has a stale content hash",
                    body.entity.value()
                ));
            }
            if body.entity.value() >= FRAGMENT_ID_BASE
                && (occupied == 0
                    || !world
                        .body_mass(body)
                        .is_some_and(|m| m.is_finite() && m > 0.0))
            {
                result.issues.push(format!(
                    "fragment {} is empty or has invalid mass",
                    body.entity.value()
                ));
            }
            if !geometry.is_current(field) {
                result.issues.push(format!(
                    "terrain {} has stale surface dependencies",
                    body.entity.value()
                ));
            }
            let mut covered = 0_u64;
            for chunk in geometry.chunks() {
                result.terrain_rectangles += chunk.rectangles.len();
                result.terrain_polygons += chunk.polygons.len();
                result.terrain_polygon_vertices += chunk
                    .polygons
                    .iter()
                    .map(|p| p.vertices.len())
                    .sum::<usize>();
                if field.chunk_revision(chunk.id) != Some(chunk.revision) {
                    result.issues.push(format!(
                        "terrain {} has stale chunk {}",
                        body.entity.value(),
                        chunk.id.0
                    ));
                }
                covered += chunk.material_cells();
                for part in 0..chunk.shape_count() {
                    expected_colliders.insert(ColliderId::new(
                        body.entity,
                        ColliderRole::new(physics::terrain_spec().first_chunk_role + chunk.id.0),
                        part as u16,
                    ));
                }
            }
            if covered != occupied as u64 {
                result.issues.push(format!(
                    "terrain {} geometry covers {covered} cells, material has {occupied}",
                    body.entity.value()
                ));
            }
        }
        result.terrain_colliders = expected_colliders.len();
        for (&index, planet) in &self.terrain.planets {
            let supported = planet.footing.iter().all(|c| {
                planet
                    .field
                    .cell(*c)
                    .is_some_and(|c| c.material != MaterialId::VOID)
            });
            if supported != planet.supported {
                result
                    .issues
                    .push(format!("planet {index} support does not match its footing"));
            }
            if planet.supported && self.terrain.legacy_services {
                expected_colliders.insert(physics::spaceport_sensor_id(index));
            } else if self.spaceport_contacts.iter().any(|c| c.planet == index)
                || self.planets[index].previous_docked_ship.is_some()
                || self.planets[index].building_new_ship_time != 0.0
            {
                result.issues.push(format!(
                    "unsupported planet {index} still has base service state"
                ));
            }
        }
        let is_terrain = |id: PhysicsId| {
            id.value() >= FRAGMENT_ID_BASE
                || physics::planet_index(id).is_some_and(|i| self.terrain.planets.contains_key(&i))
        };
        let motions = world
            .motions()
            .map(|r| (r.id, r.motion))
            .collect::<BTreeMap<_, _>>();
        let actual_bodies = motions
            .keys()
            .copied()
            .filter(|id| is_terrain(id.entity))
            .collect::<BTreeSet<_>>();
        let actual_colliders = world
            .collider_ids()
            .filter(|id| is_terrain(id.entity))
            .collect::<BTreeSet<_>>();
        if actual_bodies != expected_bodies {
            result.issues.push(format!(
                "terrain body identities differ: {} missing, {} stale",
                expected_bodies.difference(&actual_bodies).count(),
                actual_bodies.difference(&expected_bodies).count()
            ));
        }
        if actual_colliders != expected_colliders {
            result.issues.push(format!(
                "terrain collider identities differ: {} missing, {} stale",
                expected_colliders.difference(&actual_colliders).count(),
                actual_colliders.difference(&expected_colliders).count()
            ));
        }
        let tracked = self
            .terrain
            .fragments
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        if tracked != self.physics.terrain_fragments {
            result
                .issues
                .push("terrain fragment lifecycle registry differs from material ownership".into());
        }
        let mut hash = 0xcbf29ce484222325_u64;
        for (id, motion) in motions {
            let values = [
                motion.position.x,
                motion.position.y,
                motion.angle,
                motion.linear_velocity.x,
                motion.linear_velocity.y,
                motion.angular_velocity,
            ];
            if values.iter().any(|v| !v.is_finite()) {
                result
                    .issues
                    .push(format!("body {id:?} has non-finite motion"));
            } else {
                let speed = motion.linear_velocity.x.hypot(motion.linear_velocity.y);
                if speed > result.max_speed {
                    result.max_speed = speed;
                    result.fastest_body = Some(format!("{id:?}"));
                }
                result.max_spin = result.max_spin.max(motion.angular_velocity.abs());
            }
            for byte in id
                .entity
                .value()
                .to_le_bytes()
                .into_iter()
                .chain(id.role.value().to_le_bytes())
                .chain(values.into_iter().flat_map(|v| v.to_bits().to_le_bytes()))
            {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        for (index, ship) in self.ships.iter().enumerate() {
            if [
                ship.position.x,
                ship.position.y,
                ship.velocity.x,
                ship.velocity.y,
                ship.life,
                ship.rotation_radians,
                ship.omega,
            ]
            .iter()
            .any(|v| !v.is_finite())
            {
                result
                    .issues
                    .push(format!("ship {index} has non-finite state"));
            }
        }
        result.motion_hash = format!("{hash:016x}");
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_reports_stale_material_caches_and_non_finite_ship_state() {
        let mut state = SpacewarsScenario::init_terrain_fixture(42);
        state
            .terrain
            .planets
            .get_mut(&0)
            .unwrap()
            .field
            .apply(TerrainEdit {
                brush: Brush::Circle {
                    center: CellCoord::new(60, 60),
                    radius: 2,
                },
                mode: EditMode::Remove,
            })
            .unwrap();
        state.ships[0].life = f32::NAN;
        let issues = state.terrain_diagnostics().issues;
        assert!(issues.iter().any(|s| s.contains("stale content hash")));
        assert!(issues.iter().any(|s| s.contains("stale chunk")));
        assert!(issues.iter().any(|s| s.contains("non-finite state")));
    }

    #[test]
    fn audit_detects_missing_and_stale_terrain_bodies_and_colliders() {
        let mut state = SpacewarsScenario::init_terrain_fixture(42);
        assert!(state.terrain_diagnostics().issues.is_empty());
        let missing = physics::planet_entity(0);
        state.physics.world.remove_entity(missing);
        let stale = PhysicsId::new(FRAGMENT_ID_BASE + 123);
        state
            .physics
            .world
            .insert_body(physics::primary_body(stale), BodySpec::default(), &[]);
        let issues = state.terrain_diagnostics().issues;
        assert!(
            issues
                .iter()
                .any(|s| s.contains("body identities differ: 1 missing, 1 stale"))
        );
        assert!(
            issues
                .iter()
                .any(|s| s.contains("collider identities differ"))
        );
    }

    #[test]
    fn audit_tracks_split_and_empty_planet_without_false_leak_reports() {
        let mut state = SpacewarsScenario::init_terrain_fixture(42);
        state
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Capsule {
                        start: CellCoord::new(60, 0),
                        end: CellCoord::new(60, 120),
                        radius: 2,
                    },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
        commit(&mut state);
        let split = state.terrain_diagnostics();
        assert!(split.fragments > 0);
        assert!(split.issues.is_empty(), "{:?}", split.issues);
        state
            .queue_planet_edit(
                0,
                TerrainEdit {
                    brush: Brush::Circle {
                        center: CellCoord::new(60, 60),
                        radius: 200,
                    },
                    mode: EditMode::Remove,
                },
            )
            .unwrap();
        commit(&mut state);
        let empty = state.terrain_diagnostics();
        assert!(empty.issues.is_empty(), "{:?}", empty.issues);
        assert_eq!(empty.supported_bases, 0);
    }
}
