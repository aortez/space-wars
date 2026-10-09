//! Preconstruction diagnostic with two physical bodies and resumable work.
//! This is conditional settling evidence, never permission to build or board.
use super::*;
use serde_json::{Value, json};
use std::time::Instant;

const HORIZON: usize = 120;
const MAX_STEPS_PER_CALL: usize = 4;
const MAX_PLANETS: usize = 32;
const MAX_LOCAL_TRAVEL: f32 = 16.0;
const DT: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone)]
struct Model {
    physics: physics::SpacewarsPhysics,
    pilot: SurfacePilot,
    ship: ShipState,
    planets: Vec<PlanetState>,
    material: Vec<bool>,
    source_positions: Vec<Vec2>,
    sun: Option<SunState>,
    preset: SurfaceMotionPreset,
    solver: GravitySolver,
    participants: Vec<GravityParticipant>,
    initial_local: Vec2,
}

/// A frozen candidate, independent of future actions and live simulation state.
/// Only the diagnostic harness opts in. `advance` caps every call at four steps.
#[derive(Clone)]
pub struct RebuildLocalForecast {
    tick: u64,
    seat: usize,
    report: RebuildPlacementReport,
    model: Option<Model>,
    input: Value,
    samples: Vec<Value>,
    chunks: Vec<Value>,
    steps: usize,
    first_settled: Option<u64>,
    stop: Option<&'static str>,
    setup_time: Duration,
    step_time: Duration,
    diagnostic_time: Duration,
}

impl SurfaceSortieState {
    pub fn enable_rebuild_local_forecasts(&mut self) {
        self.rebuild_local_forecasts.get_or_insert_with(Vec::new);
    }

    pub fn take_rebuild_local_forecasts(&mut self) -> Vec<RebuildLocalForecast> {
        self.rebuild_local_forecasts
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
}

impl RebuildLocalForecast {
    pub(in crate::surface_sortie) fn new(
        state: &SurfaceSortieState,
        seat: usize,
        planet: usize,
        pose: &RebuildPose,
    ) -> Self {
        let start = Instant::now();
        let report = state.pilots[seat]
            .recovery
            .as_ref()
            .unwrap()
            .observation()
            .placement
            .unwrap();
        let model = Model::new(state, seat, planet, pose);
        let (model, stop) = match model {
            Ok(m) => (Some(m), None),
            Err(reason) => (None, Some(reason)),
        };
        let mut result = Self {
            tick: state.tick(),
            seat,
            report,
            model,
            input: json!({"captured_before_construction":true,
                "live_vehicle_available":state.vehicle_available(seat),
                "live_bodies":state.world.physics.world.body_count(),
                "live_colliders":state.world.physics.world.collider_count(),
                "planet_count":state.world.planets.len(),"sun_present":state.world.sun.is_some(),
                "candidate_center":pose.center,"candidate_normal":pose.normal}),
            samples: Vec::with_capacity(HORIZON + 1),
            chunks: Vec::new(),
            steps: 0,
            first_settled: None,
            stop,
            setup_time: start.elapsed(),
            step_time: Duration::ZERO,
            diagnostic_time: Duration::ZERO,
        };
        if let Some(model) = &result.model {
            result.input["bodies"] = json!(model.physics.world.body_count());
            result.input["colliders"] = json!(model.physics.world.collider_count());
            let start = Instant::now();
            result
                .samples
                .push(model.sample(result.tick, result.report.revision));
            result.diagnostic_time += start.elapsed();
        }
        result
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn seat(&self) -> usize {
        self.seat
    }
    pub fn is_complete(&self) -> bool {
        self.stop.is_some()
    }

    /// A work quota, not a wall-time deadline. Calling with zero does no work.
    pub fn advance(&mut self, budget: usize) -> usize {
        if self.is_complete() || budget == 0 {
            return 0;
        }
        let start = Instant::now();
        let before = self.steps;
        let model = self.model.as_mut().unwrap();
        for _ in 0..budget.min(MAX_STEPS_PER_CALL).min(HORIZON - self.steps) {
            let step_start = Instant::now();
            let valid = model.step();
            self.step_time += step_start.elapsed();
            self.steps += 1;
            let sample_start = Instant::now();
            let tick = self.tick + self.steps as u64;
            self.samples.push(model.sample(tick, self.report.revision));
            self.diagnostic_time += sample_start.elapsed();
            if !valid {
                self.stop = Some("outside_model_scope");
                break;
            }
            if model.pilot.landing.phase == LandingPhase::Landed {
                self.first_settled.get_or_insert(tick);
            }
        }
        if self.steps == HORIZON && self.stop.is_none() {
            self.stop = Some("horizon");
        }
        let advanced = self.steps - before;
        self.chunks.push(
            json!({"first_step":before + 1,"steps":advanced,"elapsed_ms":ms(start.elapsed())}),
        );
        advanced
    }

    pub fn diagnostics(&self) -> Value {
        let prediction = if self.stop == Some("horizon") {
            Some(self.first_settled.is_some())
        } else {
            None
        };
        json!({"tick":self.tick,"seat":self.seat,"report":self.report,"input":self.input,
            "horizon_ticks":HORIZON,"max_steps_per_call":MAX_STEPS_PER_CALL,"step_nanoseconds":DT.as_nanos(),
            "max_planets":MAX_PLANETS,"max_planet_colliders":128,"max_shape_parts":16_384,
            "max_local_travel":MAX_LOCAL_TRAVEL,"steps":self.steps,"stop":self.stop,
            "settles_within_horizon":prediction,"first_settled_tick":self.first_settled,
            "samples":self.samples,"chunks":self.chunks,
            "scope":"selected_planet_geometry_and_gravity_ephemeris",
            "action_policy":"unoccupied_replacement_neutral","future_actions_read":false,
            "terrain_updates":false,"other_actors":false,"damage_and_hazards":false,
            "production_qualified":false,"read_only":true,
            "timing":{"setup_ms":ms(self.setup_time),"physics_steps_ms":ms(self.step_time),
                "diagnostics_ms":ms(self.diagnostic_time)}})
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

impl Model {
    fn new(
        state: &SurfaceSortieState,
        seat: usize,
        planet: usize,
        pose: &RebuildPose,
    ) -> Result<Self, &'static str> {
        if state.world.planets.len() > MAX_PLANETS || state.vehicle_available(seat) {
            return Err("source_limit_or_existing_vehicle");
        }
        let mut ship = state.replacement_ship_at(seat, planet, pose);
        let index = state.pilots[seat].vehicle.0;
        let mut physics = state
            .world
            .physics
            .rebuild_forecast_world(planet, index, &ship)
            .ok_or("planet_geometry_unavailable_or_over_limit")?;
        let frame = motion::SurfaceFrame::read(&physics, planet);
        let body = physics.ship_body(index);
        let origin_velocity = physics.world.velocity_at_point(body, pose.center).unwrap();
        physics.world.apply_velocity_delta(
            body,
            motion::point_velocity(frame, pose.center) - origin_velocity,
            true,
        );
        ship.velocity = physics.world.motion(body).unwrap().linear_velocity;
        // Only physical source/ephemeris fields cross into the model. No claims,
        // construction timers, opponents, debris, terrain evolution or RNG.
        let planets = state
            .world
            .planets
            .iter()
            .map(|p| PlanetState {
                position: p.position,
                radius: p.radius,
                mass: p.mass,
                color: Color::WHITE,
                owner_id: None,
                capturing_player_id: None,
                previous_docked_ship: None,
                dock_contest_time: 0.0,
                taking_ownership_time: 0.0,
                building_new_ship_time: 0.0,
                orbit_radius: p.orbit_radius,
                orbit_angle: p.orbit_angle,
                orbit_omega: p.orbit_omega,
                wrapper_angle: p.wrapper_angle,
                wrapper_omega: p.wrapper_omega,
            })
            .collect();
        let source_positions = (0..state.world.planets.len())
            .map(|i| {
                state
                    .world
                    .physics
                    .world
                    .motion(state.world.physics.planet_body(i))
                    .unwrap()
                    .position
            })
            .collect();
        let mut pilot = SurfacePilot::new(state.pilots[seat].owner, planet, false);
        pilot.vehicle = state.pilots[seat].vehicle;
        pilot.flight_enabled = state.pilots[seat].flight_enabled;
        Ok(Self {
            physics,
            pilot,
            ship,
            planets,
            source_positions,
            material: (0..state.world.planets.len())
                .map(|i| state.world.terrain.planets.contains_key(&i))
                .collect(),
            sun: state.world.sun,
            preset: state.motion_preset,
            solver: GravitySolver::default(),
            participants: Vec::with_capacity(MAX_PLANETS + 2),
            initial_local: (pose.center - frame.position).rotate_radians(-frame.angle),
        })
    }

    fn step(&mut self) -> bool {
        let dt = DT.as_secs_f32();
        self.participants.clear();
        if let Some(sun) = self.sun {
            self.participants.push(GravityParticipant::direct_source(
                tagged_gravity_id(GRAVITY_BODY_TAG, 0),
                sun.position,
                sun.mass,
            ));
        }
        for (i, p) in self.planets.iter_mut().enumerate() {
            let id = tagged_gravity_id(GRAVITY_BODY_TAG, i as u64 + 1);
            self.participants.push(if self.material[i] {
                GravityParticipant::spherical_source(id, self.source_positions[i], p.mass, p.radius)
            } else {
                GravityParticipant::direct_source(id, self.source_positions[i], p.mass)
            });
            self.preset.advance(p, self.sun, dt);
        }
        let planet = self.pilot.planet;
        self.physics.world.set_next_kinematic_pose(
            self.physics.planet_body(planet),
            self.planets[planet].position,
            self.planets[planet].wrapper_angle,
        );
        let index = self.pilot.vehicle.0;
        self.physics.reconcile_surface_vehicle(index, &self.ship);
        self.participants.push(GravityParticipant::target(
            tagged_gravity_id(GRAVITY_SHIP_TAG, index as u64),
            self.ship.position,
            1.0,
        ));
        let dv = self
            .solver
            .solve(
                &self.participants,
                GravityConfig {
                    backend: GravityBackend::BarnesHut { theta: 0.7 },
                    softening: GRAVITY_SOFTENING,
                    interaction_scale: GRAVITY,
                },
            )
            .expect("native source inputs")
            .last()
            .unwrap()
            .velocity_delta;
        self.pilot.ship_gravity_delta = dv;
        self.physics
            .world
            .apply_velocity_delta(self.physics.ship_body(index), dv, true);
        self.pilot
            .control_vehicle(&mut self.physics, &self.ship, &self.planets, dt);
        self.physics.step(dt);
        let body = self
            .physics
            .world
            .motion(self.physics.ship_body(index))
            .unwrap();
        self.ship.position = body.position - SHIP_PIVOT;
        self.ship.velocity = body.linear_velocity;
        self.ship.rotation_radians = body.angle;
        self.ship.direction = direction_from_rotation(body.angle);
        self.ship.omega = physics::control_angular_velocity(&self.ship, body.angular_velocity);
        self.pilot.landing.update(
            &self.physics,
            index,
            planet,
            &self.planets[planet],
            &self.ship,
            dt,
        );
        for (i, p) in self.planets.iter().enumerate() {
            self.source_positions[i] = p.position;
        }
        let frame = motion::SurfaceFrame::read(&self.physics, planet);
        self.source_positions[planet] = frame.position;
        let local = (body.position - frame.position).rotate_radians(-frame.angle);
        let distance = body.position.distance_to(frame.position)
            - self.planets[planet].radius * BODY_BOUNDS_RADIUS_SCALE;
        local.distance_to(self.initial_local) <= MAX_LOCAL_TRAVEL
            && self.planets.iter().enumerate().all(|(i, p)| {
                i == planet
                    || body.position.distance_to(p.position) - p.radius * BODY_BOUNDS_RADIUS_SCALE
                        + 2.0
                        >= distance
            })
    }

    fn sample(&self, tick: u64, revision: Option<u64>) -> Value {
        let index = self.pilot.vehicle.0;
        let body_id = self.physics.ship_body(index);
        let body = self.physics.world.motion(body_id).unwrap();
        let frame = motion::SurfaceFrame::read(&self.physics, self.pilot.planet);
        let up = (body.position - frame.position).normalized();
        let contacts = std::array::from_fn::<_, 3, _>(|part| {
            let count = self.physics.surface_vehicle_contacts(index, part).count();
            let contacts = self.physics.surface_vehicle_contacts(index, part).take(16).map(|c| {
                let relative = self.physics.world.velocity_at_point(body_id,c.position).unwrap() - c.velocity;
                json!({"collider":format!("{:?}",c.collider),"retained_planet":physics::is_planet_surface_support(c.collider,self.pilot.planet),
                    "position":c.position,"local_position":c.local_surface.position,"normal":c.normal,
                    "up_alignment":c.normal.dot(up),"separation":c.separation,"normal_speed":relative.dot(c.normal)})
            }).collect::<Vec<_>>();
            json!({"count":count,"contacts":contacts})
        });
        json!({"tick":tick,"ship":pilot::PilotMotion { position:body.position,
            velocity:self.physics.world.velocity_at_point(body_id,body.position).unwrap(),angle:body.angle,spin:body.angular_velocity },
            "planet":pilot::PilotMotion { position:frame.position,velocity:frame.linear_velocity,angle:frame.angle,spin:frame.angular_velocity },
            "revision":revision,"landing":self.pilot.landing,"contacts":contacts,
            "settled":self.pilot.landing.phase == LandingPhase::Landed})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn released(owner: PlayerId) -> [Action; 5] {
        [
            SurfaceSortieAction::default().encode(owner),
            SurfaceWingAction::default().encode(owner),
            combat::SurfaceWeaponAction::default().encode(owner),
            SurfaceMiningAction::default().encode(owner),
            impact::SurfaceImpactAction::default().encode(owner),
        ]
    }

    #[test]
    fn rebuild_local_forecast_starts_before_construction_and_matches_native_settling() {
        let mut state = native_forecast::tests::fresh_build(true);
        let mut jobs = state.take_rebuild_local_forecasts();
        assert_eq!(jobs.len(), 1);
        let mut job = jobs.pop().unwrap();
        assert!(job.stop.is_none(), "{}", job.diagnostics());
        assert_eq!(job.input["live_vehicle_available"], false);
        assert_eq!(job.input["bodies"], 2);
        assert_eq!(
            job.samples[0]["ship"],
            json!(state.pilot_observation(0, None).ship)
        );
        let before = state.world.physics.snapshot_bytes();
        let mut single = job.clone();
        assert_eq!(job.advance(0), 0);
        assert_eq!(job.steps, 0);
        while !job.is_complete() {
            assert!(job.advance(usize::MAX) <= MAX_STEPS_PER_CALL);
        }
        while !single.is_complete() {
            assert_eq!(single.advance(1), 1);
        }
        assert_eq!(job.samples, single.samples);
        assert_eq!(state.world.physics.snapshot_bytes(), before);
        assert_eq!(job.steps, HORIZON);
        assert_eq!(job.chunks.len(), 30);
        assert_eq!(single.chunks.len(), 120);
        assert_eq!(job.advance(1), 0);
        let mut first_settled = None;
        let actions = released(PlayerId::PLAYER_1);
        for sample in job.samples.iter().skip(1) {
            SurfaceSortieScenario::step(&mut state, &actions, DT);
            if state.vehicle_settled(0) {
                first_settled.get_or_insert(state.tick());
            }
            let actual = state.pilot_observation(0, None);
            let expected = &sample["ship"]["position"];
            let position = Vec2::new(
                expected["x"].as_f64().unwrap() as f32,
                expected["y"].as_f64().unwrap() as f32,
            );
            assert!(
                position.distance_to(actual.ship.position) < 0.01,
                "tick {}",
                state.tick()
            );
            assert_eq!(sample["planet"], json!(actual.planet.motion));
        }
        assert!(first_settled.is_some());
        assert_eq!(job.first_settled, first_settled);
        assert_eq!(job.diagnostics()["settles_within_horizon"], true);
    }

    #[test]
    fn rebuild_local_forecast_out_of_scope_is_inconclusive_even_after_a_landing() {
        let mut state = native_forecast::tests::fresh_build(true);
        let mut job = state.take_rebuild_local_forecasts().pop().unwrap();
        job.first_settled = Some(job.tick);
        job.model.as_mut().unwrap().initial_local += Vec2::new(100.0, 0.0);
        assert_eq!(job.advance(4), 1);
        assert_eq!(job.stop, Some("outside_model_scope"));
        assert!(job.diagnostics()["settles_within_horizon"].is_null());
    }

    #[test]
    fn rebuild_local_forecast_rejects_unready_geometry_and_excess_sources() {
        let mut state = native_forecast::tests::fresh_build(true);
        let job = state.take_rebuild_local_forecasts().pop().unwrap();
        let model = job.model.unwrap();
        let body = model
            .physics
            .world
            .motion(model.physics.ship_body(0))
            .unwrap();
        let pose = RebuildPose {
            center: body.position,
            normal: Vec2::Y.rotate_radians(body.angle),
        };
        assert!(matches!(
            Model::new(&state, 0, 0, &pose),
            Err("source_limit_or_existing_vehicle")
        ));
        state.world.ships[0].dead = true;
        state.world.physics.material_queries_dirty = true;
        assert!(matches!(
            Model::new(&state, 0, 0, &pose),
            Err("planet_geometry_unavailable_or_over_limit")
        ));
        state.world.physics.material_queries_dirty = false;
        state
            .world
            .planets
            .resize(MAX_PLANETS + 1, state.world.planets[0]);
        assert!(matches!(
            Model::new(&state, 0, 0, &pose),
            Err("source_limit_or_existing_vehicle")
        ));
    }
}
