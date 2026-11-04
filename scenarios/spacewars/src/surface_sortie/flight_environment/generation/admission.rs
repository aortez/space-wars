use super::*;
use engine_gravity::{GravityBackend, GravityConfig, GravityId, GravityParticipant, GravitySolver};

impl FlightCandidate {
    pub fn inspect(&self) -> FlightAdmission {
        self.inspect_distance_range(30.0, 200.0)
    }

    /// Separate clear-control-v1 recipe; the original four-family admission is unchanged.
    pub fn inspect_clear_control(&self) -> FlightAdmission {
        let bounds = match self.recipe.as_str() {
            "clear-control-v1/stratum-0" => Some((4.0, 12.0, 1.0, 0.2)),
            "clear-control-v1/stratum-1" => Some((12.0, 24.0, 2.0, 0.4)),
            "clear-control-v1/stratum-2" => Some((24.0, 48.0, 4.0, 0.6)),
            "clear-control-v1/stratum-3" => Some((48.0, 80.0, 6.0, 0.8)),
            _ => None,
        };
        let mut report = self.inspect_distance_range(4.0, 80.0);
        let valid = bounds.is_some_and(|(low, high, speed, spin)| {
            let task = &self.task;
            let distance = task.goal_offset.length();
            self.family == FlightFamily::Clear
                && (low..=high).contains(&distance)
                && task.start.relative_velocity.length() <= speed
                && task.start.spin.abs() <= spin
                && (140.0..=150.0).contains(&task.start.altitude)
                && !task.start.gravity
                && task.start.planet_spin == 0.0
                && task.goal_velocity == Vec2::ZERO
                && task.obstacles.is_empty()
                && task.terrain_edits.is_empty()
                && task.arrival.position == 3.0
                && task.arrival.velocity == 1.0
                && task.arrival.spin == 0.2
                && task.arrival.dwell_ticks == 30
                && task.deadline_ticks == 1800
                && task.hold_failure.is_none()
        });
        if !valid {
            report.admitted = false;
            report.rejection = Some("outside_clear_control_recipe".into());
        }
        report
    }

    /// Stationary, aligned starts isolate learning a short approach and stop.
    pub fn inspect_aligned_approach(&self) -> FlightAdmission {
        let bounds = match self.recipe.as_str() {
            "aligned-approach-v1/stratum-0" => Some((4.0, 6.0)),
            "aligned-approach-v1/stratum-1" => Some((6.0, 8.0)),
            "aligned-approach-v1/stratum-2" => Some((8.0, 10.0)),
            "aligned-approach-v1/stratum-3" => Some((10.0, 12.0)),
            _ => None,
        };
        let mut report = self.inspect_distance_range(4.0, 12.0);
        let valid = bounds.is_some_and(|(low, high)| {
            let task = &self.task;
            let distance = task.goal_offset.length();
            let forward = Vec2::Y.rotate_radians(task.start.bearing + task.start.heading_offset);
            let direction = task.goal_offset / distance;
            self.family == FlightFamily::Clear
                && (low..=high).contains(&distance)
                && (forward - direction).length() <= 0.00001
                && task.start.relative_velocity == Vec2::ZERO
                && task.start.spin == 0.0
                && (140.0..=150.0).contains(&task.start.altitude)
                && !task.start.gravity
                && task.start.planet_spin == 0.0
                && task.goal_velocity == Vec2::ZERO
                && task.obstacles.is_empty()
                && task.terrain_edits.is_empty()
                && task.arrival.position == 3.0
                && task.arrival.velocity == 1.0
                && task.arrival.spin == 0.2
                && task.arrival.dwell_ticks == 30
                && task.deadline_ticks == 1800
                && task.hold_failure.is_none()
        });
        if !valid {
            report.admitted = false;
            report.rejection = Some("outside_aligned_approach_recipe".into());
        }
        report
    }

    /// Zero-error starts isolate learning to preserve the existing arrival tolerances.
    pub fn inspect_hold_position(&self) -> FlightAdmission {
        self.inspect_hold_variant(false)
    }

    pub fn inspect_hold_first_exit(&self) -> FlightAdmission {
        self.inspect_hold_variant(true)
    }

    fn inspect_hold_variant(&self, early_failure: bool) -> FlightAdmission {
        let prefix = if early_failure {
            "hold-first-exit-v1/"
        } else {
            "hold-position-v1/"
        };
        let slot = match self.recipe.strip_prefix(prefix) {
            Some("stratum-0") => Some(0),
            Some("stratum-1") => Some(1),
            Some("stratum-2") => Some(2),
            Some("stratum-3") => Some(3),
            _ => None,
        };
        let mut report = self.inspect_distance_range(0.0, 0.0);
        let valid = slot.is_some_and(|slot| {
            let task = &self.task;
            let low = -std::f32::consts::PI + slot as f32 * FRAC_PI_2;
            self.family == FlightFamily::Clear
                && (low..=low + FRAC_PI_2).contains(&task.start.heading_offset)
                && task.goal_offset == Vec2::ZERO
                && task.start.relative_velocity == Vec2::ZERO
                && task.start.spin == 0.0
                && (140.0..=150.0).contains(&task.start.altitude)
                && !task.start.gravity
                && task.start.planet_spin == 0.0
                && task.goal_velocity == Vec2::ZERO
                && task.obstacles.is_empty()
                && task.terrain_edits.is_empty()
                && task.arrival.position == 3.0
                && task.arrival.velocity == 1.0
                && task.arrival.spin == 0.2
                && task.arrival.dwell_ticks == 30
                && task.deadline_ticks == 1800
                && task.hold_failure == early_failure.then_some(HoldFailure::FirstToleranceExitV1)
        });
        if !valid {
            report.admitted = false;
            report.rejection = Some("outside_hold_position_recipe".into());
        }
        report
    }

    /// Fixed near-goal learning families; short dwell is a matched evaluation probe.
    pub fn inspect_settle_recover(&self) -> FlightAdmission {
        let slot = match self.recipe.as_str() {
            "settle-recover-v1/stratum-0" => Some(0),
            "settle-recover-v1/stratum-1" => Some(1),
            "settle-recover-v1/stratum-2" => Some(2),
            "settle-recover-v1/stratum-3" => Some(3),
            _ => None,
        };
        let mut report = self.inspect_distance_range(0.0, 8.001);
        let t = &self.task;
        let distance = t.goal_offset.length();
        let speed = t.start.relative_velocity.length();
        let spin = t.start.spin.abs();
        let family_valid = match slot {
            Some(0) => distance == 0.0 && speed == 0.0 && spin == 0.0,
            Some(1) => (3.999..=8.001).contains(&distance) && speed == 0.0 && spin == 0.0,
            Some(2) => distance == 0.0 && (1.199..=2.001).contains(&speed) && spin == 0.0,
            Some(3) => distance == 0.0 && speed == 0.0 && (0.299..=0.501).contains(&spin),
            _ => false,
        };
        if !(family_valid
            && self.family == FlightFamily::Clear
            && (140.0..=150.0).contains(&t.start.altitude)
            && !t.start.gravity
            && t.start.planet_spin == 0.0
            && t.goal_velocity == Vec2::ZERO
            && t.obstacles.is_empty()
            && t.terrain_edits.is_empty()
            && t.arrival.position == 3.0
            && t.arrival.velocity == 1.0
            && t.arrival.spin == 0.2
            && [30, 300].contains(&t.arrival.dwell_ticks)
            && t.deadline_ticks == 900
            && t.hold_failure.is_none())
        {
            report.admitted = false;
            report.rejection = Some("outside_settle_recover_recipe".into());
        }
        report
    }

    /// Fixed-horizon robustness probes use geometric admission, not a travel-time gate.
    pub fn inspect_hold_robustness(&self) -> FlightAdmission {
        let mut report = self.inspect_distance_range(0.0, 4.001);
        let task = &self.task;
        let valid = self.recipe.starts_with("hold-robustness-v1/")
            && self.family == FlightFamily::Clear
            && task.start.relative_velocity.length() <= 1.501
            && task.start.spin.abs() <= 0.301
            && (140.0..=150.0).contains(&task.start.altitude)
            && !task.start.gravity
            && task.start.planet_spin == 0.0
            && task.goal_velocity == Vec2::ZERO
            && task.obstacles.is_empty()
            && task.terrain_edits.is_empty()
            && task.arrival.position == 3.0
            && task.arrival.velocity == 1.0
            && task.arrival.spin == 0.2
            && [30, 120, 300].contains(&task.arrival.dwell_ticks)
            && [30, 120, 300].contains(&task.deadline_ticks)
            && task.hold_failure.is_none();
        if !valid {
            report.admitted = false;
            report.rejection = Some("outside_hold_robustness_envelope".into());
        } else if report.rejection.as_deref() == Some("reference_time_exceeds_deadline") {
            // All geometric checks have passed. The old estimate includes a fixed
            // travel/settling reserve, inappropriate for a prescribed hold horizon.
            // Retain the estimate as evidence; do not claim dynamic feasibility.
            report.admitted = true;
            report.rejection = None;
        }
        report
    }

    fn inspect_distance_range(&self, minimum: f32, maximum: f32) -> FlightAdmission {
        let mut report = FlightAdmission {
            candidate: self.clone(),
            admitted: false,
            rejection: None,
            evidence: None,
            geometry: Vec::new(),
        };
        if self.generator_version != GENERATOR_VERSION {
            report.rejection = Some("unsupported_generator_version".into());
            return report;
        }
        match FlightEnvironment::new(self.task.clone()) {
            Err(e) => report.rejection = Some(format!("invalid_reset: {e}")),
            Ok(env) => {
                if let Err(e) = inspect(&env, self, &mut report, minimum, maximum) {
                    report.rejection = Some(e.into());
                } else {
                    report.admitted = true;
                }
            }
        }
        report
    }
}

pub(super) fn inspect(
    env: &FlightEnvironment,
    candidate: &FlightCandidate,
    report: &mut FlightAdmission,
    minimum_goal_distance: f32,
    maximum_goal_distance: f32,
) -> Result<(), &'static str> {
    let state = &env.state;
    let world = &state.world.physics.world;
    let body = state.world.physics.ship_body(0);
    let start = env.frame.center_of_mass;
    let goal = env.frame.target_position;
    let deadline = candidate.task.deadline_ticks as f32 / FLIGHT_HZ as f32;
    let end = goal + candidate.task.goal_velocity * deadline;
    let world_radius = state.world.config.universe_radius as f32;
    let center = crate::universe_center(world_radius);
    let com_local = (start - env.frame.origin).rotate_radians(-env.frame.angle);
    let hull = world
        .body_solid_radius_about(body, com_local)
        .ok_or("missing_hull_bound")?;
    let margin = GOAL_CLEARANCE - hull - candidate.task.arrival.position;
    let policy = env
        .policy_observation()
        .map_err(|_| "unsupported_observation")?;
    report.geometry = world.polar_geometry_outlines(engine_rapier::world::polar::PolarFilter {
        excluded: &[body.entity],
        ..Default::default()
    })?;
    let mut envelopes = Vec::new();
    // This first generator uses only the practice planet with fixed center.
    // Reject other motion models rather than treating a frozen snapshot as future truth.
    for (i, p) in state.world.planets.iter().enumerate() {
        if p.orbit_omega != 0.0 || state.planet_motion(i).linear_velocity.length() > 0.00001 {
            return Err("unsupported_moving_body_origin");
        }
        if p.wrapper_omega != 0.0 {
            let id = state.world.physics.planet_body(i);
            envelopes.push((
                world.motion(id).ok_or("missing_planet")?.position,
                world.body_solid_radius(id).ok_or("missing_planet_bound")?,
            ));
        }
    }
    for record in world.motions() {
        if record.id.entity != body.entity
            && !matches!(
                physics::classify_entity(record.id.entity),
                Some(
                    physics::MechanicalEntity::World
                        | physics::MechanicalEntity::Body(BodyId::Planet(_))
                )
            )
            && world.body_kind(record.id) != Some(BodyKind::Fixed)
        {
            return Err("unsupported_independent_body_motion");
        }
    }
    let radius = hull + ROUTE_MARGIN;
    let route_clear = |a, b| {
        segment_clear(
            world,
            body.entity,
            center,
            world_radius,
            &envelopes,
            a,
            b,
            radius,
        )
    };
    let full_hull_start_clear = state
        .world
        .physics
        .surface_preview_geometry(0, &state.world.ships[0])
        .iter()
        .all(|s| {
            world.collider_fits_at(
                s.id,
                env.frame.origin + s.local_position.rotate_radians(env.frame.angle),
                env.frame.angle + s.local_angle,
            ) == Some(true)
        });
    // Above a spinning planet the goal rides the surface, so its path over
    // the deadline is an arc, checked as a chain of segments.
    let spin = state.world.planets[0].wrapper_omega;
    let goal_path: Vec<Vec2> = if spin == 0.0 {
        vec![goal, end]
    } else {
        let planet = state.world.planets[0].position;
        (0..=12)
            .map(|step| {
                let t = deadline * step as f32 / 12.0;
                planet + (goal - planet + candidate.task.goal_velocity * t).rotate_radians(spin * t)
            })
            .collect()
    };
    let end = goal_path[goal_path.len() - 1];
    let goal_clear = goal_path.windows(2).all(|leg| {
        segment_clear(
            world,
            body.entity,
            center,
            world_radius,
            &envelopes,
            leg[0],
            leg[1],
            GOAL_CLEARANCE,
        )
    });
    let direct = route_clear(start, goal);
    report.evidence = Some(AdmissionEvidence {
        start_com: start,
        goal_start: goal,
        goal_end: end,
        world_center: center,
        world_radius,
        hull_com_radius: hull,
        goal_margin_after_hull_and_tolerance: margin,
        direct_clear: direct,
        full_hull_start_clear,
        goal_trajectory_clear: goal_clear,
        route: Vec::new(),
        route_length: 0.0,
        first_leg_moves_away: false,
        rotation_envelopes: envelopes.clone(),
        gravity_samples: Vec::new(),
        reference_time_seconds: None,
        deadline_seconds: deadline,
        object_count: policy.object_sources.iter().flatten().count(),
    });
    if !(minimum_goal_distance..=maximum_goal_distance)
        .contains(&candidate.task.goal_offset.length())
    {
        return Err("goal_distance_outside_recipe");
    }
    if margin < 1.0 {
        return Err("insufficient_goal_hull_margin");
    }
    if !full_hull_start_clear {
        return Err("full_hull_start_overlap");
    }
    if !route_clear(start, start) {
        return Err("no_orientation_independent_start_clearance");
    }
    if !goal_clear {
        return Err("unsafe_goal_trajectory");
    }
    if matches!(candidate.family, FlightFamily::Clear | FlightFamily::Moving) && !direct {
        return Err("clear_family_direct_route_blocked");
    }
    if candidate.family == FlightFamily::Obstructed && direct {
        return Err("obstructed_family_does_not_require_detour");
    }
    let path = route::find(start, goal, center, world_radius, &route_clear);
    if path.is_empty() {
        return Err("no_conservative_route_witness");
    }
    let evidence = report.evidence.as_mut().unwrap();
    evidence.route_length = path.windows(2).map(|p| p[0].distance_to(p[1])).sum();
    evidence.first_leg_moves_away = path[1].distance_to(goal) > start.distance_to(goal) + 0.01;
    evidence.gravity_samples = gravity_samples(env, &path, goal, end)?;
    // A transparent engineering screen, never a flown trajectory or upper bound.
    // Five seconds reserves acceleration/braking/settling; segment allowance
    // uses the measured ~1.9 s half-turn. Target motion consumes closure speed.
    let target_speed = candidate.task.goal_velocity.length();
    let reference = (5.0 + 1.9 * (path.len() - 1) as f32 + evidence.route_length / 35.0)
        / (1.0 - target_speed / 35.0)
        + candidate.task.arrival.dwell_ticks as f32 / 60.0;
    evidence.reference_time_seconds = Some(reference);
    evidence.route = path;
    if reference > deadline {
        return Err("reference_time_exceeds_deadline");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn segment_clear(
    world: &PhysicsWorld,
    own: PhysicsId,
    center: Vec2,
    world_radius: f32,
    envelopes: &[(Vec2, f32)],
    a: Vec2,
    b: Vec2,
    radius: f32,
) -> bool {
    if a.distance_to(center) + radius >= world_radius
        || b.distance_to(center) + radius >= world_radius
    {
        return false;
    }
    if envelopes
        .iter()
        .any(|&(p, r)| route::distance_to_segment(p, a, b) <= r + radius)
    {
        return false;
    }
    let d = b - a;
    // A capsule is the exact swept disk for this whole straight segment.
    world.capsule_is_clear_except(
        (a + b) * 0.5,
        d.y.atan2(d.x) - FRAC_PI_2,
        d.length() * 0.5,
        radius,
        CollisionGroups::ALL,
        Some(own),
    )
}

fn gravity_samples(
    env: &FlightEnvironment,
    path: &[Vec2],
    goal: Vec2,
    end: Vec2,
) -> Result<Vec<GravitySample>, &'static str> {
    // Production ship gravity currently samples its legacy sprite anchor,
    // offset from the rigid-body origin by SHIP_PIVOT. Retain that diagnostic
    // point explicitly; subsequent points sample the field along COM routes.
    let mut points = vec![env.state.world.ships[0].position];
    for (a, b) in path
        .windows(2)
        .map(|p| (p[0], p[1]))
        .chain(std::iter::once((goal, end)))
    {
        let count = (a.distance_to(b) / GRID_SPACING).ceil().max(1.0) as usize;
        for i in 0..=count {
            points.push(a + (b - a) * (i as f32 / count as f32));
        }
    }
    let mut participants: Vec<_> = env
        .state
        .world
        .planets
        .iter()
        .enumerate()
        .map(|(i, p)| {
            GravityParticipant::spherical_source(
                GravityId::new(i as u64),
                p.position,
                p.mass,
                p.radius,
            )
        })
        .collect();
    let sources = participants.len();
    participants.extend(
        points
            .iter()
            .enumerate()
            .map(|(i, p)| GravityParticipant::target(GravityId::new(100 + i as u64), *p, 1.0)),
    );
    let mut solver = GravitySolver::new();
    let outputs = solver
        .solve(
            &participants,
            GravityConfig {
                backend: GravityBackend::Exact,
                softening: crate::GRAVITY_SOFTENING,
                interaction_scale: crate::GRAVITY * 60.0,
            },
        )
        .map_err(|_| "invalid_gravity_probe")?;
    Ok(points
        .into_iter()
        .zip(&outputs[sources..])
        .map(|(position, o)| GravitySample {
            position,
            acceleration: o.velocity_delta,
        })
        .collect())
}
