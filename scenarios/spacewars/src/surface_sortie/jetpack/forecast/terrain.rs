//! Opt-in forecasts for one nearby high terrain gap per completed survey.
//! Real vehicles remain collision obstacles; no prospective hull is substituted.
use super::*;

pub const MAX_GRAPH_WORK: u32 = 8192;
pub const MAX_QUERY_WORK: u32 = 8192;
pub const LAUNCH_WINDOW: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TerrainCrossingForecast {
    pub version: u32,
    pub measured_tick: u64,
    pub launch_until_tick: u64,
    pub plan: CrossingPlan,
    pub nodes: [u16; 2],
    pub flights: [FlightEstimate; 2],
}
impl TerrainCrossingForecast {
    pub fn valid_at(self, tick: u64) -> bool {
        let p = self.plan;
        self.version == 1
            && self.launch_until_tick.checked_sub(self.measured_tick) == Some(LAUNCH_WINDOW)
            && (self.measured_tick..=self.launch_until_tick).contains(&tick)
            && [p.start, p.destination]
                .iter()
                .all(|v| v.x.is_finite() && v.y.is_finite())
            && (1.0..=24.0).contains(&p.start.distance_to(p.destination))
            && p.cruise_radius.is_finite()
            && p.cruise_radius > p.start.length().max(p.destination.length())
            && p.cruise_radius > p.start.length().min(p.destination.length()) + 10.0
            && p.cruise_radius < p.start.length().min(p.destination.length()) + 20.0
            && matches!(p.anchor, CrossingAnchor::GroundGap { from, to }
                if from != to && from < 512 && to < 512)
            && self.nodes[0] != self.nodes[1]
            && self.nodes.iter().all(|n| *n < 512)
            && self.flights.iter().all(|f| {
                f.seconds.is_finite()
                    && f.seconds > 0.0
                    && f.seconds <= MAX_STEPS as f32 * DT
                    && f.burn_seconds.is_finite()
                    && f.burn_seconds >= 0.0
                    && f.burn_seconds <= (LAUNCH_CHARGE - LANDING_RESERVE) * motor::BURN_SECONDS
                    && f.arrival_speed.is_finite()
                    && (0.0..=7.0).contains(&f.arrival_speed)
            })
    }
    pub fn valid_for(self, map: &GroundMap, tick: u64) -> bool {
        self.valid_at(tick)
            && map.version == 1
            && map.tick <= tick
            && map.planet == self.plan.planet
            && map.revision == self.plan.revision
            && self
                .nodes
                .into_iter()
                .zip([self.plan.start, self.plan.destination])
                .all(|(id, p)| {
                    map.nodes
                        .iter()
                        .any(|n| n.id == id && n.position.distance_to(p) < 0.01)
                })
    }
    pub fn edges(self) -> [GroundEdge; 2] {
        crossing_edges(self.nodes, self.plan)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerrainFlightAttempt {
    pub margin: usize,
    pub height: f32,
    pub nodes: [u16; 2],
    pub rejection: Option<&'static str>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TerrainFlightSurvey {
    pub forecast: Option<TerrainCrossingForecast>,
    pub graph_work: u32,
    pub query_work: u32,
    pub attempts: Vec<TerrainFlightAttempt>,
}

impl FlightScene {
    fn read_terrain(
        state: &SurfaceSortieState,
        player: usize,
        p: &pilot::PilotObservationV1,
    ) -> Option<Self> {
        let environment = FlightEnvironment::read(state, p)?;
        let spec = SurfaceSortieState::spec();
        Some(Self {
            environment,
            snapshot: Arc::new(state.world.physics.world.query_snapshot()),
            frame_position: p.planet.motion.position,
            frame_angle: p.planet.motion.angle,
            capsules: [0.02, 0.20].map(|margin| {
                CapsuleQuery::new(
                    spec.half_segment,
                    spec.radius + margin,
                    spec.collision_groups,
                    vec![pilot_physics_id(state.pilots[player].owner)],
                )
            }),
            terrain: true,
            // Never queried for terrain; the actual vehicle remains in snapshot.
            previews: [Arc::new(|_, _, _, _| true), Arc::new(|_, _, _, _| true)],
            hull: Arc::new(Vec::new()),
            vehicle: state.pilots[player].vehicle.0,
        })
    }
}

impl SurfaceSortieState {
    /// Read-only and bounded across all 24 candidates, both directions, and
    /// three launch samples. Exhaustion publishes no edge, even after one leg.
    pub fn forecast_terrain_gap(
        &self,
        player: usize,
        o: &recovery_sensors::RecoveryTaskObservationV1,
        from: u16,
        to: u16,
    ) -> Option<TerrainFlightSurvey> {
        let p = &o.flight.pilot;
        let map = o.ground.as_ref()?;
        let j = o.jetpack.as_ref()?;
        if self.pilots.get(player)?.owner != p.owner
            || p.tick != self.world.tick
            || p.location != PilotLocation::OnFoot
            || self.location(player) != PilotLocation::OnFoot
            || p.planet.index != self.motion_planet_index(player)
            || map.revision
                != self
                    .world
                    .terrain
                    .planets
                    .get(&map.planet)?
                    .field
                    .revision()
            || !p.queries_ready
            || self.world.physics.material_queries_dirty
            || !j.surveyed
            || !j.charge.is_finite()
            || !(0.0..=1.0).contains(&j.charge)
            || map.version != 1
            || map.actor != p.owner
            || map.tick != p.tick
            || map.planet != p.planet.index
            || map.revision != p.planet.revision
            || !(2..=512).contains(&map.nodes.len())
        {
            return None;
        }
        let n = map.nodes.len();
        let i = map.nodes.iter().position(|node| node.id == from)?;
        if map.nodes[(i + 1) % n].id != to {
            return None;
        }
        let scene = FlightScene::read_terrain(self, player, p)?;
        let mut result = TerrainFlightSurvey::default();
        for margin in 0..8.min(n / 2) {
            let a = map.nodes[(i + n - margin) % n];
            let b = map.nodes[(i + 1 + margin) % n];
            for height in [3.0, 5.0, 7.0] {
                let cruise = a.position.length().max(b.position.length()) + height;
                if !(1.0..=24.0).contains(&a.position.distance_to(b.position))
                    || [a, b].iter().any(|node| {
                        node.normal.dot(node.position.normalized())
                            < Self::spec().min_support_alignment
                    })
                    || cruise <= a.position.length().min(b.position.length()) + 10.0
                    || cruise >= a.position.length().min(b.position.length()) + 20.0
                    || a.position
                        .normalized()
                        .dot(b.position.normalized())
                        .clamp(-1.0, 1.0)
                        .acos()
                        > 0.55
                {
                    continue;
                }
                let proposal = Proposal {
                    plan: CrossingPlan {
                        planet: map.planet,
                        revision: map.revision,
                        direction: CrossingDirection::Left,
                        start: a.position,
                        destination: b.position,
                        cruise_radius: cruise,
                        anchor: CrossingAnchor::GroundGap { from, to },
                    },
                    nodes: [a.id, b.id],
                    measured_nodes: [a, b],
                };
                // Retain the ordinary corridor's full capsule margin during
                // the high ascent too. Charge these geometric queries against
                // the same total as prediction queries, including rejections.
                let queries = std::cell::Cell::new(result.query_work);
                let clear = |point: Vec2| {
                    if queries.get() == MAX_QUERY_WORK {
                        return false;
                    }
                    queries.set(queries.get() + 1);
                    scene.capsules[1].is_clear(
                        &scene.snapshot,
                        scene.frame_position + point.rotate_radians(scene.frame_angle),
                        rotation_for_direction(point) + scene.frame_angle,
                    )
                };
                let corridor = corridor_clear(a.position, b.position, cruise, &clear);
                result.query_work = queries.get();
                if !corridor {
                    let exhausted = result.query_work == MAX_QUERY_WORK;
                    result.attempts.push(TerrainFlightAttempt {
                        margin,
                        height,
                        nodes: proposal.nodes,
                        rejection: Some(if exhausted {
                            "work_limit"
                        } else {
                            "corridor_clearance"
                        }),
                    });
                    if exhausted {
                        return Some(result);
                    }
                    continue;
                }
                let mut job = FlightForecastJob::new(&scene, proposal);
                while let Some(work) = job.next_work() {
                    let (used, limit) = match work {
                        WorkKind::Graph => (&mut result.graph_work, MAX_GRAPH_WORK),
                        WorkKind::PhysicsQuery => (&mut result.query_work, MAX_QUERY_WORK),
                    };
                    if *used == limit {
                        job.reject("work_limit");
                        break;
                    }
                    *used += 1;
                    job.step();
                }
                result.attempts.push(TerrainFlightAttempt {
                    margin,
                    height,
                    nodes: proposal.nodes,
                    rejection: job.rejection(),
                });
                if let Some(f) = job.output().copied().flatten() {
                    let forecast = TerrainCrossingForecast {
                        version: 1,
                        measured_tick: f.measured_tick,
                        launch_until_tick: f.launch_until_tick,
                        plan: f.plan,
                        nodes: f.nodes,
                        flights: f.flights,
                    };
                    if forecast.valid_for(map, p.tick) {
                        result.forecast = Some(forecast);
                    }
                    return Some(result);
                }
                if job.rejection() == Some("work_limit") {
                    return Some(result);
                }
            }
        }
        Some(result)
    }

    /// Optional runtime sensor. Only the nearest high, disconnected gap is
    /// considered. The ordinary survey and all lower corridors are unchanged.
    pub fn add_terrain_flight_forecast(
        &self,
        player: usize,
        o: &mut recovery_sensors::RecoveryTaskObservationV1,
    ) {
        #[cfg(feature = "sensor-profile")]
        let _profile = super::super::super::sensor_profile::Scope::new("terrain_flight_forecast");
        let p = &o.flight.pilot;
        if p.location != PilotLocation::OnFoot
            || p.ship_form != ShipForm::EscapePod
            || !o.jetpack.as_ref().is_some_and(|j| j.surveyed)
        {
            return;
        }
        let Some(map) = &o.ground else {
            return;
        };
        let Some(actor) = p.actor else {
            return;
        };
        let local =
            (actor.position - p.planet.motion.position).rotate_radians(-p.planet.motion.angle);
        let n = map.nodes.len();
        if !(2..=512).contains(&n) {
            return;
        }
        let gap = (0..n)
            .filter_map(|i| {
                let a = map.nodes[i];
                let b = map.nodes[(i + 1) % n];
                let rise = (a.position.length() - b.position.length()).abs();
                let connected = |from, to| map.edges.iter().any(|e| e.from == from && e.to == to);
                (rise > 7.0
                    && rise < 17.0
                    && a.position.distance_to(b.position) <= 12.0
                    && (!connected(a.id, b.id) || !connected(b.id, a.id)))
                .then_some((a, b))
            })
            .min_by(|(a, _), (b, _)| {
                a.position
                    .distance_to(local)
                    .total_cmp(&b.position.distance_to(local))
                    .then(a.id.cmp(&b.id))
            });
        if let Some((a, b)) = gap {
            let survey = self.forecast_terrain_gap(player, o, a.id, b.id);
            o.jetpack.as_mut().unwrap().terrain_flight = survey;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_rapier::world::{
        BodyId, BodyKind, BodyRole, BodySpec, ColliderId, ColliderRole, ColliderSpec,
    };

    #[test]
    fn terrain_scene_keeps_real_vehicle_and_other_obstacles_and_is_read_only() {
        let mut state = SurfaceSortieScenario::init_material_jetpack(42, 1);
        for _ in 0..120 {
            SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        }
        let p = state.pilot_observation(0, None);
        let before = state.world.physics.snapshot_bytes();
        let scene = FlightScene::read_terrain(&state, 0, &p).unwrap();
        // The real ship center is occupied. A terrain scene excludes only the
        // pilot body, unlike a prospective replacement-vehicle scene.
        assert!(!scene.capsules[0].is_clear(&scene.snapshot, p.ship.position, p.ship.angle));
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        let position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 20.0);
        assert!(scene.capsules[0].is_clear(&scene.snapshot, position, 0.0));
        let entity = PhysicsId::new(9_100_000);
        assert!(state.world.physics.world.insert_body(
            BodyId::new(entity, BodyRole::PRIMARY),
            BodySpec {
                kind: BodyKind::Fixed,
                position,
                ..Default::default()
            },
            &[ColliderSpec::cuboid(
                ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                2.0,
                2.0
            )],
        ));
        SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
        let p = state.pilot_observation(0, None);
        let scene = FlightScene::read_terrain(&state, 0, &p).unwrap();
        assert!(!scene.capsules[0].is_clear(&scene.snapshot, position, 0.0));
    }

    #[test]
    fn terrain_authorization_requires_both_legs_current_time_and_actual_endpoints() {
        let plan = CrossingPlan {
            planet: 0,
            revision: 1,
            direction: CrossingDirection::Left,
            start: Vec2::new(0.0, 30.0),
            destination: Vec2::new(-3.0, 40.0),
            cruise_radius: 43.2,
            anchor: CrossingAnchor::GroundGap { from: 2, to: 4 },
        };
        let f = TerrainCrossingForecast {
            version: 1,
            measured_tick: 60,
            launch_until_tick: 90,
            plan,
            nodes: [1, 5],
            flights: [FlightEstimate {
                seconds: 5.0,
                burn_seconds: 1.8,
                arrival_speed: 2.0,
            }; 2],
        };
        let mut map = GroundMap {
            version: 1,
            actor: PlayerId::PLAYER_1,
            planet: 0,
            revision: 1,
            tick: 60,
            nodes: f
                .nodes
                .into_iter()
                .zip([plan.start, plan.destination])
                .map(|(id, position)| GroundNode {
                    id,
                    position,
                    normal: position.normalized(),
                })
                .collect(),
            edges: vec![],
            rejected: vec![],
        };
        assert!(f.valid_for(&map, 60) && f.valid_for(&map, 90));
        assert!(!f.valid_at(59) && !f.valid_at(91));
        assert_eq!(f.edges().map(|e| (e.from, e.to)), [(1, 5), (5, 1)]);
        map.nodes[1].position.x += 0.02;
        assert!(!f.valid_for(&map, 60));
        map.nodes[1].position = plan.destination;
        map.revision += 1;
        assert!(!f.valid_for(&map, 60));
        for leg in 0..2 {
            let mut bad = f;
            bad.flights[leg].burn_seconds = motor::BURN_SECONDS;
            assert!(!bad.valid_at(60));
            bad = f;
            bad.flights[leg].seconds = 12.1;
            assert!(!bad.valid_at(60));
            bad = f;
            bad.flights[leg].arrival_speed = f32::NAN;
            assert!(!bad.valid_at(60));
        }
    }
}
