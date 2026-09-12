//! Forecast access to an existing flag with the proposed landed hull present.
//! These bounded measurements guide site selection, never transfer or claim permissions.
use super::*;
use ground_navigation::{GROUND_REFRESH_TICKS, GROUND_SAMPLES, GroundMap, GroundRouteDiagnostics};
use pilot::{LandingSiteId, PilotObservationV1};

pub const MAX_OBJECTIVE_SITES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LandingObjective {
    pub planet: usize,
    pub revision: u64,
    pub owner: PlayerId,
    /// Flag position in the retained planet's local frame.
    pub position: Vec2,
    pub range: f32,
}

impl LandingObjective {
    pub fn read(p: &PilotObservationV1) -> Option<Self> {
        let claim = p.planet.claim.as_ref()?;
        let flag = claim.flag.filter(|_| claim.owner != Some(p.owner))?;
        Some(Self {
            planet: p.planet.index,
            revision: p.planet.revision,
            owner: flag.player,
            position: (flag.position - p.planet.motion.position)
                .rotate_radians(-p.planet.motion.angle),
            range: claim.flag_interaction_range - 0.2,
        })
    }

    pub fn matches(self, other: Self) -> bool {
        self.revision == other.revision && self.same_flag(other)
    }

    pub fn same_flag(self, other: Self) -> bool {
        self.planet == other.planet
            && self.owner == other.owner
            && self.position.distance_to(other.position) < 0.5
            && (self.range - other.range).abs() < 0.01
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LandingObjectiveRoute {
    /// Absent for the check at the actual touchdown pose.
    pub site: Option<LandingSiteId>,
    pub outbound: GroundRouteDiagnostics,
    pub returning: Option<GroundRouteDiagnostics>,
}

impl LandingObjectiveRoute {
    pub fn cost(&self) -> Option<f32> {
        let returning = self.returning.as_ref()?;
        if [&self.outbound, returning].iter().any(|r| {
            r.failure.is_some()
                || r.partial
                || !r.length.is_finite()
                || r.length < 0.0
                || r.start_node.is_none()
                || r.reachable_nodes == 0
                || r.destination_nodes == 0
                || r.jumps > GROUND_SAMPLES
                || r.flights != 0
        }) {
            return None;
        }
        // Express the round trip at walking speed in the flight selector's
        // distance units (38 / 5 = 7.6), with a small per-jump time allowance.
        Some(
            (self.outbound.length + returning.length) * 7.6
                + (self.outbound.jumps + returning.jumps) as f32 * 15.0,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LandingObjectiveSurvey {
    pub version: u32,
    pub actor: PlayerId,
    pub tick: u64,
    pub objective: LandingObjective,
    pub sites: Vec<LandingObjectiveRoute>,
    pub actual: Option<LandingObjectiveRoute>,
}

impl SurfaceSortieState {
    pub(super) fn landing_objective_survey(
        &self,
        player: usize,
        p: &PilotObservationV1,
        cover: &[combat::LandingCover],
    ) -> Option<LandingObjectiveSurvey> {
        let objective = LandingObjective::read(p)?;
        if !p.queries_ready
            || !matches!(p.location, PilotLocation::Aboard(_))
            || p.ship_form != ShipForm::Ship
            || !p.ship_available
            || !(p.tick + player as u64 * 15).is_multiple_of(GROUND_REFRESH_TICKS)
        {
            return None;
        }
        // Forecast ground jumps at the objective, rather than using an airborne
        // ship's weaker gravity or an absent spaceling's zero acceleration.
        let flag =
            p.planet.motion.position + objective.position.rotate_radians(p.planet.motion.angle);
        let gravity = self
            .world
            .planets
            .iter()
            .map(|body| compatibility::source_acceleration(body.position, body.mass, flag).length())
            .sum::<f32>()
            + self.world.sun.map_or(0.0, |sun| {
                compatibility::source_acceleration(sun.position, sun.mass, flag).length()
            });
        let map = self.survey_ground_with_gravity(
            player,
            p.planet.index,
            0..GROUND_SAMPLES as u16,
            true,
            gravity,
        )?;
        let ship = self.replacement_ship(player);
        let spec = Self::spec();
        let clearance_radius = physics::SpacewarsPhysics::surface_vehicle_clearance_radius(&ship)
            + spec.half_height()
            + 0.02;
        let preview = self.world.physics.surface_vehicle_capsule_clearance(
            self.pilots[player].vehicle.0,
            &ship,
            spec.half_segment,
            spec.radius + 0.02,
        );
        let measure = |id, vehicle: Vec2, angle: f32, hatch: Vec2| {
            let avoiding = map.avoiding(gravity, |position| {
                let world =
                    p.planet.motion.position + position.rotate_radians(p.planet.motion.angle);
                world.distance_to(vehicle) > clearance_radius
                    || preview(
                        world,
                        rotation_for_direction(position) + p.planet.motion.angle,
                        vehicle,
                        angle,
                    )
            });
            let hatch = (hatch - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
            measure_route(&avoiding, id, hatch, objective)
        };
        let mut candidates: Vec<_> = p.sites.iter().collect();
        candidates.sort_by(|a, b| {
            a.hatch_position
                .distance_to(flag)
                .total_cmp(&b.hatch_position.distance_to(flag))
        });
        // Reserve half the bounded shortlist for sheltered alternatives. A
        // short exposed walk must not hide every usable covered approach.
        let mut shortlisted: Vec<_> = candidates
            .iter()
            .copied()
            .take(MAX_OBJECTIVE_SITES / 2)
            .collect();
        for candidate in candidates
            .iter()
            .copied()
            .filter(|site| {
                cover
                    .iter()
                    .any(|c| c.site == site.id && c.grounded && c.approach && c.departure)
            })
            .chain(candidates.iter().copied())
        {
            if shortlisted.len() == MAX_OBJECTIVE_SITES {
                break;
            }
            if !shortlisted.iter().any(|site| site.id == candidate.id) {
                shortlisted.push(candidate);
            }
        }
        let sites = shortlisted
            .into_iter()
            .map(|site| {
                measure(
                    Some(site.id),
                    site.vehicle_position,
                    rotation_for_direction(site.normal),
                    site.hatch_position,
                )
            })
            .collect();
        let actual = if p.landing.phase == LandingPhase::Landed {
            p.hatch
                .map(|hatch| measure(None, p.ship.position, p.ship.angle, hatch))
        } else {
            None
        };
        Some(LandingObjectiveSurvey {
            version: 1,
            actor: p.owner,
            tick: p.tick,
            objective,
            sites,
            actual,
        })
    }
}

fn measure_route(
    map: &GroundMap,
    site: Option<LandingSiteId>,
    hatch: Vec2,
    objective: LandingObjective,
) -> LandingObjectiveRoute {
    let outbound = map.route_to_actor_target(hatch, objective.position, objective.range);
    let returning = outbound
        .path
        .last()
        .and_then(|id| map.nodes.iter().find(|n| n.id == *id))
        .map(|node| map.route_to_hatch(node.position, hatch).diagnostics);
    LandingObjectiveRoute {
        site,
        outbound: outbound.diagnostics,
        returning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ground_navigation::{GroundEdge, GroundEdgeKind, GroundNode, GroundRouteFailure};

    #[test]
    fn one_way_ground_is_not_a_round_trip_and_partial_routes_are_not_access() {
        let mut map = GroundMap {
            version: 1,
            actor: PlayerId::PLAYER_1,
            planet: 0,
            revision: 1,
            tick: 0,
            nodes: (0..4)
                .map(|id| GroundNode {
                    id,
                    position: Vec2::new(f32::from(id) * 2.0, 60.0),
                    normal: Vec2::Y,
                })
                .collect(),
            edges: (0..3)
                .map(|id| GroundEdge {
                    from: id,
                    to: id + 1,
                    kind: GroundEdgeKind::Walk,
                    length: 2.0,
                })
                .collect(),
            rejected: vec![],
        };
        let objective = LandingObjective {
            planet: 0,
            revision: 1,
            owner: PlayerId::PLAYER_2,
            position: Vec2::new(6.0, 60.9),
            range: 1.0,
        };
        let start = Vec2::new(0.0, 60.0);
        let one_way = measure_route(&map, None, start, objective);
        assert!(one_way.outbound.failure.is_none());
        assert_eq!(
            one_way.returning.as_ref().unwrap().failure,
            Some(GroundRouteFailure::Disconnected)
        );
        assert!(one_way.cost().is_none());
        map.edges.extend((0..3).map(|id| GroundEdge {
            from: id + 1,
            to: id,
            kind: GroundEdgeKind::Walk,
            length: 2.0,
        }));
        assert!(measure_route(&map, None, start, objective).cost().is_some());
        map.edges.clear();
        let disconnected = measure_route(&map, None, start, objective);
        assert!(disconnected.cost().is_none());
        assert!(disconnected.returning.is_none());
    }

    #[test]
    fn objective_surveys_are_bounded_read_only_and_absent_without_an_enemy_flag() {
        let mut state = SurfaceSortieScenario::init_material_combat(42);
        for _ in 0..GROUND_REFRESH_TICKS {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        let mut p = state.pilot_observation(0, None);
        assert!(state.landing_objective_survey(0, &p, &[]).is_none());
        // Supply a target observation while retaining the real material and hull
        // geometry. This sensor test makes no claim or ownership transition.
        let site = p.sites[0];
        let claim = p.planet.claim.as_mut().unwrap();
        claim.owner = Some(PlayerId::PLAYER_2);
        claim.flag = Some(PlanetFlagObservation {
            player: PlayerId::PLAYER_2,
            position: site.hatch_position,
            normal: site.normal,
            raised_fraction: 1.0,
        });
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let audit = state.terrain_diagnostics();
        let survey = state.landing_objective_survey(0, &p, &[]).unwrap();
        assert!(!survey.sites.is_empty() && survey.sites.len() <= MAX_OBJECTIVE_SITES);
        assert_eq!(survey, state.landing_objective_survey(0, &p, &[]).unwrap());
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        assert_eq!(
            serde_json::to_value(state.terrain_diagnostics()).unwrap(),
            serde_json::to_value(audit).unwrap()
        );
        p.sites = vec![site];
        assert_eq!(
            state
                .landing_objective_survey(0, &p, &[])
                .unwrap()
                .sites
                .len(),
            1
        );
        p.tick += 1;
        assert!(state.landing_objective_survey(0, &p, &[]).is_none());
        p.tick -= 1;
        state.world.physics.material_queries_dirty = true;
        assert!(state.landing_objective_survey(0, &p, &[]).is_none());
        assert!(state.world.physics.material_queries_dirty);
        state.world.physics.material_queries_dirty = false;
        p.planet.claim.as_mut().unwrap().owner = Some(p.owner);
        assert!(state.landing_objective_survey(0, &p, &[]).is_none());
    }
}
