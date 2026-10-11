//! Offline graph witnesses for a route failure, using the native measurements.
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize)]
pub struct TopologyRoute {
    pub route: LandingObjectiveRoute,
    pub outbound_path: Vec<u16>,
    pub returning_path: Option<Vec<u16>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LandingSiteTopology {
    pub site: LandingSiteId,
    pub removed_nodes: Vec<u16>,
    /// Indices into the base graph's directed edge list.
    pub removed_edges: Vec<usize>,
    pub with_ship: TopologyRoute,
    /// Counterfactual graph only: omits the proposed parked observing ship.
    /// Other physical obstacles remain. Never authorizes a landing or walk.
    pub without_ship: TopologyRoute,
}

#[derive(Debug, Clone, Serialize)]
pub struct LandingTopology {
    pub objective: LandingObjective,
    pub gravity: f32,
    /// As in the native preview, the observing actor and its current ship are
    /// excluded. Each candidate adds the proposed parked ship's hull and feet.
    pub base: GroundMap,
    pub sites: Vec<LandingSiteTopology>,
}

impl SurfaceSortieState {
    /// Retain the measured graph, candidate hull exclusions and exact paths.
    /// This repeats offline physical work and must not feed the playing bot.
    pub fn diagnose_landing_topology(
        &self,
        player: usize,
        p: &PilotObservationV1,
    ) -> Result<LandingTopology, &'static str> {
        self.validate_landing_diagnostic(player, p, ObjectivePlanning::JointRoundTrip)?;
        if !p.queries_ready
            || !matches!(p.location, PilotLocation::Aboard(_))
            || p.ship_form != ShipForm::Ship
            || !p.ship_available
            || !(p.tick + player as u64 * 15).is_multiple_of(GROUND_REFRESH_TICKS)
        {
            return Err("objective sensor unavailable at this tick");
        }
        let objective = LandingObjective::read(p).unwrap();
        let flag =
            p.planet.motion.position + objective.position.rotate_radians(p.planet.motion.angle);
        // Match the native preview's gravity and clearance arithmetic exactly.
        // Replay validation compares every resulting route with native batches.
        let gravity = self
            .world
            .planets
            .iter()
            .map(|body| compatibility::source_acceleration(body.position, body.mass, flag).length())
            .sum::<f32>()
            + self.world.sun.map_or(0.0, |sun| {
                compatibility::source_acceleration(sun.position, sun.mass, flag).length()
            });
        let base = self
            .survey_ground_with_gravity(
                player,
                p.planet.index,
                0..GROUND_SAMPLES as u16,
                true,
                gravity,
            )
            .ok_or("ground sensor unavailable")?;
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
        let mut sites = Vec::new();
        for site in &p.sites {
            let map = base.avoiding(gravity, |position| {
                let world =
                    p.planet.motion.position + position.rotate_radians(p.planet.motion.angle);
                world.distance_to(site.vehicle_position) > clearance_radius
                    || preview(
                        world,
                        rotation_for_direction(position) + p.planet.motion.angle,
                        site.vehicle_position,
                        rotation_for_direction(site.normal),
                    )
            });
            let nodes: BTreeSet<_> = map.nodes.iter().map(|n| n.id).collect();
            let edges: BTreeSet<_> = map.edges.iter().map(|e| (e.from, e.to)).collect();
            let measure = |map: &GroundMap| {
                let local = |point: Vec2| {
                    (point - p.planet.motion.position).rotate_radians(-p.planet.motion.angle)
                };
                let trip = map.routes().round_trip_to_hatches(
                    local(site.hatch_position),
                    objective.position,
                    objective.range,
                    site.boarding_hatches.map(|h| h.map(local)),
                );
                TopologyRoute {
                    route: LandingObjectiveRoute {
                        crossing: None,
                        site: Some(site.id),
                        outbound: trip.outbound.diagnostics,
                        returning: trip.returning.as_ref().map(|r| r.diagnostics.clone()),
                        endpoint: trip.endpoint,
                    },
                    outbound_path: trip.outbound.path,
                    returning_path: trip.returning.map(|r| r.path),
                }
            };
            sites.push(LandingSiteTopology {
                site: site.id,
                removed_nodes: base
                    .nodes
                    .iter()
                    .filter(|n| !nodes.contains(&n.id))
                    .map(|n| n.id)
                    .collect(),
                removed_edges: base
                    .edges
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| !edges.contains(&(e.from, e.to)))
                    .map(|(i, _)| i)
                    .collect(),
                with_ship: measure(&map),
                without_ship: measure(&base),
            });
        }
        Ok(LandingTopology {
            objective,
            gravity,
            base,
            sites,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_reproduces_native_routes_and_witnesses_paths_without_mutation() {
        let (state, p) = super::super::tests::fixture();
        let before = state.world.physics.world.snapshot_bytes().unwrap();
        let topology = state.diagnose_landing_topology(0, &p).unwrap();
        let batches = state
            .diagnose_landing_routes(0, &p, ObjectivePlanning::JointRoundTrip)
            .unwrap();
        assert_eq!(topology.sites.len(), p.sites.len());
        assert!(
            topology
                .sites
                .iter()
                .any(|s| s.with_ship.route.cost().is_some())
        );
        assert!(topology.sites.iter().any(|s| !s.removed_nodes.is_empty()));
        for site in &topology.sites {
            let native = batches
                .iter()
                .flat_map(|s| &s.sites)
                .find(|r| r.site == Some(site.site))
                .unwrap();
            assert_eq!(site.with_ship.route, *native);
            let mut map = topology.base.clone();
            map.nodes.retain(|n| !site.removed_nodes.contains(&n.id));
            map.edges = map
                .edges
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !site.removed_edges.contains(i))
                .map(|(_, e)| e)
                .collect();
            for (map, trip) in [
                (&map, &site.with_ship),
                (&topology.base, &site.without_ship),
            ] {
                for path in std::iter::once(&trip.outbound_path).chain(&trip.returning_path) {
                    assert!(path.iter().all(|id| map.nodes.iter().any(|n| n.id == *id)));
                    assert!(
                        path.windows(2)
                            .all(|w| { map.edges.iter().any(|e| e.from == w[0] && e.to == w[1]) })
                    );
                }
            }
        }
        assert_eq!(state.world.physics.world.snapshot_bytes().unwrap(), before);
        let mut stale = p.clone();
        stale.tick -= 1;
        assert!(state.diagnose_landing_topology(0, &stale).is_err());
        let mut partial = p.clone();
        partial.site_query = pilot::LandingSiteQuery::NotRequested;
        assert!(state.diagnose_landing_topology(0, &partial).is_err());
        assert!(state.diagnose_landing_topology(1, &p).is_err());
    }
}
