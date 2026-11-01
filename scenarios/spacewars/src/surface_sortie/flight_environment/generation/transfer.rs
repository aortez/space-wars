//! The match-transfer family: starts above one planet of a generated arena,
//! the way a mission's launch leaves a ship, and goals at the mission's entry
//! point above another planet, carried by that planet's orbit and spin under
//! the arena's gravity, sun and boundary. A separate batch from the practice
//! families, so their candidate sequences stay as archived.
use super::*;
use recipe::Rng;

pub const TRANSFER_GENERATOR_VERSION: u32 = 1;
/// The mission's transfer entry altitude above a planet's radius.
pub const ENTRY_ALTITUDE: f32 = 85.0;
/// The mission hands off within this altitude; an entry point above it is no transfer goal.
pub const HANDOFF_ALTITUDE: f32 = 105.0;
/// Clearance of the entry point above the planet's highest terrain.
pub const TERRAIN_CLEARANCE: f32 = 25.0;
pub const MIN_DISTANCE: f32 = 100.0;
pub const MAX_DISTANCE: f32 = 600.0;
/// The long range's farthest start, beyond any leg of the standard arena.
pub const LONG_MAX_DISTANCE: f32 = 3000.0;
const BODY_MARGIN: f32 = 30.0;

/// The start distances a transfer batch admits. Each candidate index draws
/// the same task in either range, deadline included: a long leg gets the
/// 1,800 ticks of a mission's learned transfer attempt, which the observation's
/// remaining time is read against.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferRange {
    /// The archived family: 100 to 600 units.
    #[default]
    Standard,
    /// Beyond 600 units: the legs a mission flies across the arena, between
    /// planets on opposite sides of the sun.
    Long,
}

impl TransferRange {
    fn admits(self, distance: f32) -> bool {
        match self {
            Self::Standard => (MIN_DISTANCE..=MAX_DISTANCE).contains(&distance),
            Self::Long => distance > MAX_DISTANCE && distance <= LONG_MAX_DISTANCE,
        }
    }

    fn is_standard(&self) -> bool {
        *self == Self::Standard
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransferCandidate {
    pub generator_version: u32,
    pub seed: u64,
    pub index: u64,
    /// Absent from the standard range's records, which stay as archived.
    #[serde(default, skip_serializing_if = "TransferRange::is_standard")]
    pub range: TransferRange,
    pub task: FlightTask,
}

#[derive(Debug, Serialize)]
pub struct TransferAdmission {
    pub candidate: TransferCandidate,
    /// Admitted to the family's declared conditions, not proven flyable.
    pub admitted: bool,
    pub rejection: Option<String>,
    pub distance: Option<f32>,
    /// An engineering screen, distance over 30 plus ten seconds.
    pub reference_time_seconds: Option<f32>,
}

impl TransferCandidate {
    /// Every index produces exactly one candidate on its own generated arena.
    pub fn generate(seed: u64, index: u64, range: TransferRange) -> Self {
        let mut rng = Rng(seed ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let layout = mission::ArenaLayout::STANDARD;
        let start_planet = (rng.next() % layout.planets as u64) as usize;
        let goal_planet = (start_planet + 1 + (rng.next() % (layout.planets as u64 - 1)) as usize)
            % layout.planets;
        let world_seed = rng.next();
        let bearing = rng.range(-PI, PI);
        let altitude = rng.range(60.0, 100.0);
        let heading_offset = rng.range(-PI, PI);
        let climb = rng.range(5.0, 18.0);
        let drift = rng.range(-5.0, 5.0);
        let spin = rng.range(-0.3, 0.3);
        let state = SurfaceSortieScenario::init_material_arena_layout(world_seed, layout);
        let from = state.world.planets[start_planet];
        let to = state.world.planets[goal_planet];
        // The entry point sits at the mission's altitude above the base radius,
        // or above the highest terrain if that reaches higher.
        let solid = state
            .world
            .physics
            .world
            .body_solid_radius(state.world.physics.planet_body(goal_planet))
            .unwrap_or(to.radius);
        let entry = (to.radius + ENTRY_ALTITUDE).max(solid + TERRAIN_CLEARANCE);
        let up = Vec2::Y.rotate_radians(bearing);
        let toward_start = (from.position - to.position).normalized();
        let task = FlightTask {
            start: FlightStart {
                seed: world_seed,
                bearing,
                altitude,
                heading_offset,
                relative_velocity: up * climb + Vec2::new(up.y, -up.x) * drift,
                spin,
                gravity: true,
                planet_spin: 0.0,
            },
            goal_offset: toward_start * entry,
            goal_velocity: Vec2::ZERO,
            arrival: ArrivalCriteria::default(),
            deadline_ticks: 1800,
            hold_failure: None,
            obstacles: Vec::new(),
            terrain_edits: Vec::new(),
            transfer: Some(TransferWorld {
                layout,
                start_planet,
                goal_planet,
            }),
        };
        Self {
            generator_version: TRANSFER_GENERATOR_VERSION,
            seed,
            index,
            range,
            task,
        }
    }

    pub fn inspect(&self) -> TransferAdmission {
        let rejected = |reason: &str| TransferAdmission {
            candidate: self.clone(),
            admitted: false,
            rejection: Some(reason.to_owned()),
            distance: None,
            reference_time_seconds: None,
        };
        let env = match FlightEnvironment::new(self.task.clone()) {
            Ok(env) => env,
            Err(error) => return rejected(error),
        };
        let goal_planet = self.task.transfer.map(|t| t.goal_planet);
        let state = env.state();
        let frame = env.frame();
        if let Some(planet) = goal_planet
            && self.task.goal_offset.length()
                > state.world.planets[planet].radius + HANDOFF_ALTITUDE
        {
            return rejected("entry point above the handoff altitude");
        }
        // Every planet rides its orbit for the whole deadline, the goal with
        // its own; the goal's sweep must stay clear of the other planets'
        // sweeps and of the heat, since a planet can orbit into a hovering
        // ship as surely as the ship can fly into it.
        let goal_planet = goal_planet.expect("transfer task");
        let sun = state.solar_hazard();
        let orbit_center = sun.map_or(Vec2::ZERO, |sun| sun.position);
        let seconds = self.task.deadline_ticks as f32 / FLIGHT_HZ as f32;
        let at = |planet: &PlanetState, t: f32| {
            if planet.orbit_omega == 0.0 && planet.orbit_radius == 0.0 {
                planet.position
            } else {
                orbit_center
                    + Vec2::from_radians(planet.orbit_angle + planet.orbit_omega * t)
                        * planet.orbit_radius
            }
        };
        let solid = |index: usize| {
            state
                .world
                .physics
                .world
                .body_solid_radius(state.world.physics.planet_body(index))
                .unwrap_or(state.world.planets[index].radius)
        };
        let spin = state.world.planets[goal_planet].wrapper_omega;
        for step in 0..=60 {
            let t = seconds * step as f32 / 60.0;
            let goal_at = at(&state.world.planets[goal_planet], t)
                + self.task.goal_offset.rotate_radians(spin * t);
            for (index, planet) in state.world.planets.iter().enumerate() {
                if index != goal_planet
                    && goal_at.distance_to(at(planet, t)) < solid(index) + BODY_MARGIN
                {
                    return rejected("goal sweeps through another planet's margin");
                }
            }
            if let Some(sun) = sun
                && goal_at.distance_to(sun.position) < sun.heat_radius + BODY_MARGIN
            {
                return rejected("goal sweeps through the solar hazard");
            }
        }
        if let Some(sun) = sun
            && frame.center_of_mass.distance_to(sun.position) < sun.heat_radius + BODY_MARGIN
        {
            return rejected("start inside the solar hazard");
        }
        let distance = frame.errors.position;
        if !self.range.admits(distance) {
            return rejected("transfer distance outside the family's range");
        }
        TransferAdmission {
            candidate: self.clone(),
            admitted: true,
            rejection: None,
            distance: Some(distance),
            reference_time_seconds: Some(distance / 30.0 + 10.0),
        }
    }
}

/// Candidates in index order until `count` are admitted; every rejection is
/// retained. Fails when the attempt budget runs out first.
pub fn generate_transfers(
    seed: u64,
    count: usize,
    max_attempts: u64,
    range: TransferRange,
) -> Result<Vec<TransferAdmission>, &'static str> {
    if count == 0 || count > 1024 || max_attempts == 0 || max_attempts > 8192 {
        return Err("invalid transfer batch budget");
    }
    let mut attempts = Vec::new();
    let mut admitted = 0;
    for index in 0..max_attempts {
        if admitted == count {
            break;
        }
        let report = TransferCandidate::generate(seed, index, range).inspect();
        admitted += usize::from(report.admitted);
        attempts.push(report);
    }
    if admitted < count {
        return Err("transfer quota incomplete");
    }
    Ok(attempts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_goals_ride_their_planet_and_tasks_stay_reproducible() {
        let batch = generate_transfers(11, 2, 40, TransferRange::Standard).unwrap();
        let again = generate_transfers(11, 2, 40, TransferRange::Standard).unwrap();
        assert_eq!(
            batch.iter().map(|a| &a.candidate).collect::<Vec<_>>(),
            again.iter().map(|a| &a.candidate).collect::<Vec<_>>()
        );
        let admitted: Vec<_> = batch.iter().filter(|a| a.admitted).collect();
        assert_eq!(admitted.len(), 2);
        let task = admitted[0].candidate.task.clone();
        let transfer = task.transfer.unwrap();
        assert_ne!(transfer.start_planet, transfer.goal_planet);
        assert_eq!(task.trace_version(), FLIGHT_TRANSFER_ENVIRONMENT_VERSION);
        let value = serde_json::to_value(&task).unwrap();
        assert!(value.get("transfer").is_some());
        assert!(
            serde_json::to_value(FlightTask::default())
                .unwrap()
                .get("transfer")
                .is_none()
        );
        let mut env = FlightEnvironment::new(task.clone()).unwrap();
        let initial = env.frame().clone();
        assert!(
            initial.target_velocity.length() > 5.0,
            "the goal orbits with its planet"
        );
        assert!(initial.gravity.length() > 0.5, "the arena has gravity");
        assert!((MIN_DISTANCE..=MAX_DISTANCE).contains(&initial.errors.position));
        for _ in 0..60 {
            env.step(FlightAction::default()).unwrap();
        }
        let later = env.frame();
        assert_eq!(env.progress().outcome, None);
        assert!(later.target_position.distance_to(initial.target_position) > 3.0);
        let planet = motion::SurfaceFrame::read(&env.state().world.physics, transfer.goal_planet);
        let offset = later.target_position - planet.position;
        assert!((offset.length() - task.goal_offset.length()).abs() < 1e-3);
        let (a, b) = (task.goal_offset, offset);
        let turned = (a.x * b.y - a.y * b.x).atan2(a.x * b.x + a.y * b.y);
        assert!(
            (turned - planet.angular_velocity).abs() < 2e-3,
            "the goal turns with the spin"
        );
        let expected =
            planet.linear_velocity + Vec2::new(-offset.y, offset.x) * planet.angular_velocity;
        assert!((later.target_velocity - expected).length() < 1e-4);
        assert!(env.policy_observation().is_ok());
        let replay = FlightEnvironment::replay(env.trace()).unwrap();
        assert_eq!(replay.frame(), later);
        let mut bad = task;
        bad.start.gravity = false;
        assert!(FlightEnvironment::new(bad).is_err());
    }

    #[test]
    fn long_transfers_draw_the_same_tasks_beyond_the_standard_range() {
        let standard = generate_transfers(11, 2, 40, TransferRange::Standard).unwrap();
        let record = serde_json::to_value(&standard[0].candidate).unwrap();
        assert!(
            record.get("range").is_none(),
            "standard records stay as archived"
        );
        let long = generate_transfers(11, 2, 400, TransferRange::Long).unwrap();
        let admitted: Vec<_> = long.iter().filter(|a| a.admitted).collect();
        assert_eq!(admitted.len(), 2);
        for admission in admitted {
            let candidate = &admission.candidate;
            assert!(admission.distance.unwrap() > MAX_DISTANCE);
            let same = TransferCandidate::generate(11, candidate.index, TransferRange::Standard);
            assert_eq!(same.task, candidate.task, "the same draw in either range");
            assert!(!same.inspect().admitted);
            let record = serde_json::to_value(candidate).unwrap();
            assert_eq!(record["range"], "long");
            let back: TransferCandidate = serde_json::from_value(record).unwrap();
            assert_eq!(&back, candidate);
        }
    }
}
