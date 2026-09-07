//! Single-pilot travel policy. Raw/profile probes remain pinned positive controls.
use super::*;

// Avoid switching approach frames back and forth at a free-space bisector.
// Contact always wins; this tolerance never grants or extends physical support.
const APPROACH_HYSTERESIS: f32 = 2.0;

impl SurfacePilot {
    pub(super) fn select_approach_planet(
        &mut self,
        physics: &physics::SpacewarsPhysics,
        planets: &[PlanetState],
    ) {
        if !self.travel_enabled {
            return;
        }
        let Some(ship) = physics.world.motion(physics.ship_body(self.vehicle.0)) else {
            return;
        };
        let sample = |index: usize| {
            let frame = motion::SurfaceFrame::read(physics, index);
            let offset = ship.position - frame.position;
            (
                physics.landing_feet_supported(self.vehicle.0, index, offset.normalized()),
                offset.length() - planets[index].radius * BODY_BOUNDS_RADIUS_SCALE,
            )
        };
        let mut best = self.planet;
        let (mut feet, mut distance) = sample(best);
        for index in 0..planets.len() {
            let (candidate_feet, candidate_distance) = sample(index);
            if candidate_feet > feet
                || (candidate_feet == feet && candidate_distance + APPROACH_HYSTERESIS < distance)
            {
                best = index;
                feet = candidate_feet;
                distance = candidate_distance;
            }
        }
        self.planet = best;
    }
}

impl SurfaceSortieState {
    pub fn travel_enabled(&self) -> bool {
        self.pilot.travel_enabled
    }

    pub(super) fn ship_support_planet(&self) -> Option<usize> {
        (self.landing.supported_feet > 0)
            .then_some(self.landing.planet)
            .flatten()
    }

    pub(super) fn pilot_support_planet(&self) -> Option<usize> {
        let support = self.spaceling_snapshot()?.support?;
        physics::planet_surface_support_index(support.collider)
            .filter(|&index| index < self.world.planets.len())
    }

    pub(super) fn motion_planet_index(&self) -> usize {
        if !self.travel_enabled() {
            return self.pilot.planet;
        }
        if let Some(planet) = self.pilot_support_planet() {
            return planet;
        }
        if let Some(snapshot) = self.spaceling_snapshot() {
            // An airborne pilot's diagnostics/site focus must not follow the
            // unoccupied ship if it has independently reached another planet.
            return self
                .world
                .planets
                .iter()
                .enumerate()
                .min_by(|(a, pa), (b, pb)| {
                    let distance = |index, radius| {
                        snapshot.motion.position.distance_to(
                            motion::SurfaceFrame::read(&self.world.physics, index).position,
                        ) - radius * BODY_BOUNDS_RADIUS_SCALE
                    };
                    distance(*a, pa.radius).total_cmp(&distance(*b, pb.radius))
                })
                .map_or(self.pilot.planet, |(index, _)| index);
        }
        self.pilot.planet
    }

    pub(super) fn focused_outpost(&self) -> &outpost::SurfaceOutpost {
        let planet = self.motion_planet_index();
        self.outposts
            .iter()
            .find(|post| post.planet == planet)
            .unwrap_or(&self.outposts[0])
    }

    /// One intact site per generated body for this experiment, not a universal
    /// planet-claim rule. Identity/ownership survive changes of the active frame.
    pub(super) fn enable_travel(&mut self) {
        for (index, planet) in self.world.planets.iter().enumerate() {
            if self.outposts.iter().any(|post| post.planet == index) {
                continue;
            }
            let away = self
                .world
                .sun
                .map_or(Vec2::Y, |sun| (planet.position - sun.position).normalized());
            let angle = away.y.atan2(away.x) - planet.wrapper_angle - 20.4 / planet.radius;
            assert!(
                self.world
                    .physics
                    .insert_surface_terminal(index, planet.radius, angle)
            );
            self.outposts.push(outpost::SurfaceOutpost::new(
                OutpostId(index as u64 + 1),
                index,
                angle,
            ));
        }
        self.outposts.sort_by_key(|post| post.planet);
        self.pilot.travel_enabled = true;
    }
}

impl SurfaceSortieScenario {
    /// Opt-in travel on the named Surface V1 world. Compatibility cases still
    /// use their original single-site, pinned-planet setup for paired evidence.
    pub fn init_expedition(seed: u64) -> SurfaceSortieState {
        let mut state = Self::init(SurfaceMotionPreset::GeneratedSurfaceV1, seed);
        state.enable_travel();
        state
    }
}
