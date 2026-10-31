//! Versioned policy tensors from current physical facts. Evaluation outcomes,
//! native recommendations and diagnostic landing telemetry are not inputs.
use super::*;
use engine_rapier::world::{
    BodyId as PhysicsBodyId, ColliderId,
    polar::{PolarFilter, PolarSensor},
};
use flight_environment::{ArrivalCriteria, FLIGHT_HZ, FlightAction, flight_sensor_config};

mod objects;

pub const FLIGHT_POLICY_SCHEMA_VERSION: u32 = 2;
pub const OBJECT_CAPACITY: usize = 32;
pub const SELF_FEATURES: [&str; 35] = [
    "goal_com_x",
    "goal_com_y",
    "target_minus_com_velocity_x",
    "target_minus_com_velocity_y",
    "com_world_velocity_x",
    "com_world_velocity_y",
    "origin_world_velocity_x",
    "origin_world_velocity_y",
    "com_offset_x",
    "com_offset_y",
    "spin",
    "gravity_x",
    "gravity_y",
    "control_frame_velocity_x",
    "control_frame_velocity_y",
    "control_frame_spin",
    "thrust_acceleration",
    "turn_speed",
    "turn_acceleration",
    "brake_acceleration",
    "brake_gain",
    "cruise_speed",
    "sweep",
    "position_tolerance",
    "velocity_tolerance",
    "spin_tolerance",
    "required_dwell",
    "dwell_fraction",
    "remaining_time",
    "previous_steering",
    "previous_thrust",
    "previous_brake",
    "health_fraction",
    "solar_exposure",
    "solar_available",
];
pub const CELL_FEATURES: [&str; 2] = ["sampled_occupancy", "overlap_coverage"];
pub const RAY_FEATURES: [&str; 4] = [
    "hit",
    "distance",
    "surface_velocity_x",
    "surface_velocity_y",
];
pub const NEAREST_FEATURES: [&str; 6] = [
    "hit",
    "distance",
    "point_x",
    "point_y",
    "surface_velocity_x",
    "surface_velocity_y",
];
pub const OBJECT_FEATURES: [&str; 16] = [
    "present",
    "planet",
    "sun",
    "enclosing_edge",
    "other_solid",
    "center_x",
    "center_y",
    "center_velocity_x",
    "center_velocity_y",
    "orientation_sin",
    "orientation_cos",
    "spin",
    "bounding_radius",
    "hazard_radius",
    "hazard_enabled",
    "active_control_frame",
];
pub const DISTANCE_SCALE: f32 = 300.0;
pub const VELOCITY_SCALE: f32 = 70.0;
pub const SPIN_SCALE: f32 = 1.8;
pub const TIME_SCALE: f32 = 30.0;

/// Planner request and task-clock state, shared by training and runtime.
/// No trajectory samples, route witness or physical outcome is supplied here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlightPolicyContext {
    pub target_position: Vec2,
    pub target_velocity: Vec2,
    pub arrival: ArrivalCriteria,
    pub elapsed_ticks: u32,
    pub dwell_ticks: u32,
    pub deadline_ticks: u32,
    pub previous_action: FlightAction,
}

impl FlightPolicyContext {
    fn validate(self) -> Result<(), &'static str> {
        if !finite(self.target_position)
            || !finite(self.target_velocity)
            || ![
                self.arrival.position,
                self.arrival.velocity,
                self.arrival.spin,
                self.previous_action.steering,
            ]
            .into_iter()
            .all(f32::is_finite)
            || self.arrival.position <= 0.0
            || self.arrival.velocity <= 0.0
            || self.arrival.spin < 0.0
            || self.arrival.dwell_ticks == 0
            || self.arrival.dwell_ticks > self.deadline_ticks
            || self.deadline_ticks > 1800
            || self.elapsed_ticks > self.deadline_ticks
            || self.dwell_ticks > self.arrival.dwell_ticks
            || !(-1.0..=1.0).contains(&self.previous_action.steering)
        {
            return Err("invalid flight policy context");
        }
        Ok(())
    }
}

/// The only record intended for network consumption. Fixed sizes are specified
/// by schema(); IDs, ticks and evaluator diagnostics are kept outside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPolicyInputs {
    pub self_goal: Vec<f32>,
    /// Band-major, then sector; +x sector zero, +y thrust-forward.
    pub cells: Vec<[f32; 2]>,
    pub rays: Vec<[f32; 4]>,
    pub nearest: [f32; 6],
    /// Always 32 records; empty slots are all zero, including presence mask.
    /// Consume as a masked set, never as semantic numbered slots.
    pub objects: Vec<[f32; 16]>,
}

impl FlightPolicyInputs {
    pub fn values(&self) -> impl Iterator<Item = f32> + '_ {
        self.self_goal
            .iter()
            .copied()
            .chain(self.cells.iter().flatten().copied())
            .chain(self.rays.iter().flatten().copied())
            .chain(self.nearest)
            .chain(self.objects.iter().flatten().copied())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPolicyObservation {
    pub version: u32,
    pub tick: u64,
    pub inputs: FlightPolicyInputs,
    /// Trace associations only; never encode stable IDs as numeric features.
    pub object_sources: Vec<Option<PhysicsBodyId>>,
    pub ray_sources: Vec<Option<ColliderId>>,
    pub nearest_source: Option<ColliderId>,
}

/// Explicit export metadata: dimensions, ordering, scales and angular/radial
/// coordinates. Checkpoints must retain this alongside the schema version.
pub fn schema() -> serde_json::Value {
    let config = flight_sensor_config();
    serde_json::json!({
        "version": FLIGHT_POLICY_SCHEMA_VERSION,
        "dimensions": {"self_goal":35,"cells":[23,64,2],"rays":[128,4],"nearest":6,"objects":[32,16],"total_values":4009},
        "self_features": SELF_FEATURES.as_slice(), "cell_features": CELL_FEATURES, "ray_features": RAY_FEATURES,
        "nearest_features": NEAREST_FEATURES, "object_features": OBJECT_FEATURES,
        "radial_edges": config.radial_edges,
        "ray_bearings": (0..128).map(|i| (i as f32 + 0.5) * std::f32::consts::TAU / 128.0).collect::<Vec<_>>(),
        "scales": {"local_distance":DISTANCE_SCALE,"velocity":VELOCITY_SCALE,"linear_acceleration":45.0,"spin":SPIN_SCALE,"angular_acceleration":6.0,"brake_gain":4.0,"time_seconds":TIME_SCALE,"global_distance":"current world radius","health":"ship maximum health"},
        "frame":"rotate world vectors by -ship angle; local +y is forward; polar/object origin is body origin; goal is COM-relative",
        "padding":"angular circular; objects masked set; absent payload zero; valid values not clipped",
        "ties":"smallest stable collider ID at exactly equal native distance",
        "decision_hz": FLIGHT_HZ,
        "action_contract": "continuous-flight-v2",
        "action_ranges": {"steering":[-1.0,1.0],"thrust":[0.0,1.0],"brake":[0.0,1.0]}
    })
}

fn finite(v: Vec2) -> bool {
    v.x.is_finite() && v.y.is_finite()
}
fn push_vec(out: &mut Vec<f32>, v: Vec2) {
    out.extend([v.x, v.y]);
}

impl SurfaceSortieState {
    pub fn flight_policy_observation(
        &self,
        player: usize,
        sensor: &PolarSensor,
        context: FlightPolicyContext,
    ) -> Result<FlightPolicyObservation, &'static str> {
        context.validate()?;
        if sensor.config() != &flight_sensor_config() {
            return Err("wrong flight sensor configuration");
        }
        if self.world.physics.material_queries_dirty {
            return Err("flight geometry is not synchronized");
        }
        let flight = self
            .free_flight_observation(player)
            .ok_or("flight state unavailable")?;
        if self.flight_control_mode(player) != Some(flight::FlightControlMode::Learned)
            || !flight.controls_armed
            || !flight.alive
            || !flight.available
            || flight.form != ShipForm::Ship
            || !flight.flight.enabled
            || flight.flight.wings_closed
            || flight.flight.sweep != 0.0
            || flight.location != PilotLocation::Aboard(flight.vehicle)
        {
            return Err("unsupported flight policy control state");
        }
        let world = &self.world.physics.world;
        let body = self.world.physics.ship_body(flight.vehicle.0);
        let com = world.center_of_mass(body).ok_or("missing ship COM")?;
        let velocity = world
            .motion(body)
            .ok_or("missing ship motion")?
            .linear_velocity;
        let health_max = self.world.ships[flight.vehicle.0].life_max;
        let world_radius = self.world.config.universe_radius as f32;
        if !health_max.is_finite()
            || health_max <= 0.0
            || !world_radius.is_finite()
            || world_radius <= 0.0
        {
            return Err("invalid flight normalization scale");
        }
        let local = |v: Vec2| v.rotate_radians(-flight.ship.angle);
        let (objects, object_sources) = objects::encode(self, &flight, world_radius)?;
        let geometry = sensor.observe(
            world,
            flight.ship.position,
            flight.ship.angle,
            PolarFilter {
                excluded: &[body.entity],
                ..Default::default()
            },
        )?;
        let mut self_goal = Vec::with_capacity(SELF_FEATURES.len());
        push_vec(
            &mut self_goal,
            local(context.target_position - com) / DISTANCE_SCALE,
        );
        push_vec(
            &mut self_goal,
            local(context.target_velocity - velocity) / VELOCITY_SCALE,
        );
        push_vec(&mut self_goal, local(velocity) / VELOCITY_SCALE);
        push_vec(&mut self_goal, local(flight.ship.velocity) / VELOCITY_SCALE);
        push_vec(
            &mut self_goal,
            local(com - flight.ship.position) / DISTANCE_SCALE,
        );
        self_goal.push(flight.ship.spin / SPIN_SCALE);
        push_vec(&mut self_goal, local(flight.gravity) / 45.0);
        push_vec(
            &mut self_goal,
            local(flight.frame_velocity) / VELOCITY_SCALE,
        );
        let limits = flight.flight.limits;
        let solar = self.solar_exposure(player);
        self_goal.extend([
            flight.planet_motion.spin / SPIN_SCALE,
            limits.thrust_acceleration / 45.0,
            limits.turn_speed / SPIN_SCALE,
            limits.turn_acceleration / 6.0,
            limits.brake_acceleration / 45.0,
            limits.brake_gain / 4.0,
            limits.cruise_speed / VELOCITY_SCALE,
            flight.flight.sweep,
            context.arrival.position / DISTANCE_SCALE,
            context.arrival.velocity / VELOCITY_SCALE,
            context.arrival.spin / SPIN_SCALE,
            context.arrival.dwell_ticks as f32 / (FLIGHT_HZ as f32 * TIME_SCALE),
            context.dwell_ticks as f32 / context.arrival.dwell_ticks as f32,
            (context.deadline_ticks - context.elapsed_ticks) as f32
                / (FLIGHT_HZ as f32 * TIME_SCALE),
            context.previous_action.steering,
            context.previous_action.thrust,
            context.previous_action.brake,
            flight.health / health_max,
            solar.map_or(0.0, |s| s.intensity),
            f32::from(solar.is_some()),
        ]);
        let rays = geometry
            .rays
            .iter()
            .map(
                |ray| match (ray.distance, ray.surface_velocity, ray.collider) {
                    (Some(d), Some(v), Some(_)) => {
                        let v = local(v - flight.ship.velocity) / VELOCITY_SCALE;
                        Ok([1.0, d / DISTANCE_SCALE, v.x, v.y])
                    }
                    (None, None, None) => Ok([0.0; 4]),
                    _ => Err("inconsistent flight ray hit"),
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let nearest = match (
            geometry.nearest_distance,
            geometry.nearest_point,
            geometry.nearest_surface_velocity,
            geometry.nearest_collider,
        ) {
            (Some(d), Some(p), Some(v), Some(_)) => {
                let p = local(p - flight.ship.position) / DISTANCE_SCALE;
                let v = local(v - flight.ship.velocity) / VELOCITY_SCALE;
                [1.0, d / DISTANCE_SCALE, p.x, p.y, v.x, v.y]
            }
            (None, None, None, None) => [0.0; 6],
            _ => return Err("inconsistent nearest flight hit"),
        };
        let inputs = FlightPolicyInputs {
            self_goal,
            cells: geometry
                .cells
                .iter()
                .map(|c| [c.sampled_occupancy, c.overlap_coverage])
                .collect(),
            rays,
            nearest,
            objects,
        };
        if inputs.self_goal.len() != SELF_FEATURES.len()
            || inputs.values().count() != 4009
            || !inputs.values().all(f32::is_finite)
        {
            return Err("invalid flight policy tensors");
        }
        Ok(FlightPolicyObservation {
            version: FLIGHT_POLICY_SCHEMA_VERSION,
            tick: flight.tick,
            inputs,
            object_sources,
            ray_sources: geometry.rays.iter().map(|r| r.collider).collect(),
            nearest_source: geometry.nearest_collider,
        })
    }
}
