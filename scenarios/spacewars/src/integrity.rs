//! Material regions over Spacewars health (#330).
//!
//! Each body a weapon can harm has a region of material: a ship's hull, an
//! escape pod's hull, a pilot on foot, a piece of rock, a shell and a rover's
//! hull. The game still stores each body's health, and the region is a view of
//! it: the region's integrity is the body's health over its full health. So
//! the rules that change health, such as contacts, solar heat, repairs and
//! rebuilding, change the region with it, and play is as before. Debris breaks
//! by the game's rule once it loses a fifth of its full health, so a fifth of
//! its region's capacity breaks it, and the region itself never fails.
//!
//! A [`HealthMap`] is that view for one body. It turns the work a weapon
//! charges the region into the health the body loses, and health back into
//! work, which healing paid for in energy needs (#246). A ship or a rover built
//! with more health has more hull rather than tougher hull, so a point of its
//! health stands for the same work however it was built.
//!
//! The hulls and asteroid rock keep their reference materials' stiffness and
//! strength, with a toughness set in the match tests' fixture: a still target
//! struck every two seconds by a supplied round closing at 300. There a round
//! fails a fresh ship's hull on the fourth hit, a fresh pod's on the second and
//! a pilot on foot on the first, the survival #330 set. In open space a still
//! ship takes a fifth. A round breaks the small asteroids the game spawns, as a
//! shell did, and costs rock of radius 40 to 150 about the 15 health a shell's
//! damage did. A hit's cost follows its energy, so it rises steeply with
//! closing speed.
//!
//! Pilot tissue keeps soft tissue's stiffness and strength, tough enough that
//! a ship's laser kills a pilot on foot in half the time it takes to fail a
//! pod's hull, as the old laser did. A round closing at 300 still kills one on
//! the first hit, but one closing slower than about 236 does not.
use super::*;
use engine_material::{HULL, Material, ROCK, Region, SHELL, SOFT_TISSUE, Section};
use surface_sortie::match_rules::{PILOT_HEALTH, POD_HULL_HEALTH};

/// Depth of hull plate behind a struck face, `wu`.
const PLATE_DEPTH: f32 = 0.2;
/// Depth of rock that gives way behind a struck face, `wu`: about the width
/// of the patch a shell strikes, whatever the rock's size.
const ROCK_DEPTH: f32 = 1.0;
/// Represented depth into the screen of ships, rovers and debris, `wu`.
const THICKNESS: f32 = 2.0;
/// Represented depth of an escape pod into the screen, `wu`.
const POD_THICKNESS: f32 = 1.0;
/// Perimeter of the open-winged ship's collision hull, `wu`.
const SHIP_HULL_PERIMETER: f32 = 28.351;
/// Perimeter of the escape pod's collision hull, `wu`.
const POD_HULL_PERIMETER: f32 = 7.385;
/// Perimeter of a rover's chassis, `wu`.
const ROVER_HULL_PERIMETER: f32 = 11.2;
/// A ship built at 100 percent health has this much hull.
const SHIP_HULL_VOLUME: f32 = SHIP_HULL_PERIMETER * PLATE_DEPTH * THICKNESS;
const SHIP_HEALTH: f32 = 100.0;
const POD_HULL_VOLUME: f32 = POD_HULL_PERIMETER * PLATE_DEPTH * POD_THICKNESS;
/// A rover built with `ROVER_LIFE_MAX` health has this much hull.
const ROVER_HULL_VOLUME: f32 = ROVER_HULL_PERIMETER * PLATE_DEPTH * THICKNESS;
/// A pilot is as deep into the screen as the capsule is wide.
const PILOT_THICKNESS: f32 = 2.0 * spaceling_geometry::RADIUS;
/// Area of the pilot's capsule, `wu^2`.
const PILOT_AREA: f32 =
    core::f32::consts::PI * spaceling_geometry::RADIUS * spaceling_geometry::RADIUS
        + 4.0 * spaceling_geometry::RADIUS * spaceling_geometry::HALF_SEGMENT;
const PILOT_VOLUME: f32 = PILOT_AREA * PILOT_THICKNESS;

/// A ship's hull: the reference hull's stiffness and strength, tough enough
/// that a supplied round fails a fresh ship's hull on the fourth hit in the
/// match tests' fixture, the survival #330 set. In open space a still ship
/// takes a fifth.
pub const SHIP_HULL: Material = Material {
    name: "ship hull",
    toughness: 7_800.0,
    ..HULL
};
/// An escape pod's hull, tougher for its size, so that a supplied round fails
/// a fresh pod's hull on the second hit in that fixture, as a match's missile
/// rule did.
pub const POD_HULL: Material = Material {
    name: "pod hull",
    toughness: 12_000.0,
    ..HULL
};
/// A pilot on foot: soft tissue's stiffness and strength, tough enough that a
/// ship's laser 40 units off takes about 2.9 seconds to kill one, half as long
/// as it takes to fail a pod's hull. A supplied round kills one on the first
/// hit only when it closes at about 236 or faster.
pub const PILOT_TISSUE: Material = Material {
    name: "pilot tissue",
    toughness: 35_000.0,
    ..SOFT_TISSUE
};
/// Asteroid rock: the reference rock, tough enough that a supplied round costs
/// rock of radius 40 to 150 about the 15 health a shell's damage did.
pub const ASTEROID_ROCK: Material = Material {
    name: "asteroid rock",
    toughness: 1_750.0,
    ..ROCK
};

/// A body's material at full health, and the health that stands for it. A hull
/// region is the plate a weapon can harm, which can weigh more than its body:
/// a hit takes the body's mass from the physics, not from the region.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthMap {
    pub material: Material,
    pub section: Section,
    /// Material at full health, `wu^3`.
    pub volume: f32,
    pub full_health: f32,
}

impl HealthMap {
    /// A ship's hull: plate around the open-winged hull, more of it the more
    /// health the ship is built with.
    pub fn ship_hull(full_health: f32) -> Self {
        Self {
            material: SHIP_HULL,
            section: Section::new(THICKNESS, PLATE_DEPTH),
            volume: SHIP_HULL_VOLUME * (full_health / SHIP_HEALTH),
            full_health,
        }
    }

    /// An escape pod's hull. Only a match harms a pod; elsewhere its health
    /// tracks the rebuild instead.
    pub fn pod_hull() -> Self {
        Self {
            material: POD_HULL,
            section: Section::new(POD_THICKNESS, PLATE_DEPTH),
            volume: POD_HULL_VOLUME,
            full_health: POD_HULL_HEALTH,
        }
    }

    /// A pilot on foot: tissue filling the pilot's capsule.
    pub fn pilot() -> Self {
        Self {
            material: PILOT_TISSUE,
            section: Section::new(PILOT_THICKNESS, spaceling_geometry::RADIUS),
            volume: PILOT_VOLUME,
            full_health: PILOT_HEALTH,
        }
    }

    /// A rover's hull: plate around the chassis, more of it the more health
    /// the rover is built with.
    pub fn rover_hull(full_health: f32) -> Self {
        Self {
            material: HULL,
            section: Section::new(THICKNESS, PLATE_DEPTH),
            volume: ROVER_HULL_VOLUME * (full_health / ROVER_LIFE_MAX),
            full_health,
        }
    }

    /// Rock, or a shell's casing, weighing what the debris weighed when it
    /// was made. Rock gives way a patch's depth behind a struck face, and a
    /// shell crushes through its radius. The debris breaks below
    /// `DEBRIS_DEATH_LIFE_FACTOR` of its full health, while its region still
    /// holds that much of its capacity.
    pub fn debris(debris: &DebrisState) -> Self {
        let (material, depth) = match debris.kind {
            DebrisKind::Shell => (SHELL, debris.base_radius),
            DebrisKind::Asteroid | DebrisKind::Fragment => (ASTEROID_ROCK, ROCK_DEPTH),
        };
        let mass = debris
            .inertial_mass
            .unwrap_or_else(|| debris_mass(debris.base_radius));
        Self {
            material,
            section: Section::new(THICKNESS, depth),
            volume: mass / material.density,
            full_health: debris.life_max,
        }
    }

    /// The region's capacity, the work its full health stands for.
    pub fn capacity(self) -> f32 {
        self.material.toughness * self.volume
    }

    /// The work one point of health stands for.
    pub fn work_per_health(self) -> f32 {
        self.capacity() / self.full_health
    }

    /// The region of a body with `health` left, which counts as none below
    /// zero and as full past full. `None` if the health is not a number, or
    /// if the map's volume or full health is out of range.
    pub fn region(self, health: f32) -> Option<Region> {
        let whole = Region::new(self.material, self.volume).ok()?;
        if !(self.full_health.is_finite() && self.full_health > 0.0) {
            return None;
        }
        let share = f64::from(health.clamp(0.0, self.full_health)) / f64::from(self.full_health);
        Region::with_remaining(
            self.material,
            self.volume,
            f64::from(whole.capacity()) * share,
        )
        .ok()
    }

    /// The health that charging `work` to the region costs the body, for a
    /// map whose regions exist and work that is finite and not negative.
    pub fn health(self, work: f32) -> f32 {
        debug_assert!(work.is_finite() && work >= 0.0, "work {work}");
        (f64::from(work) * f64::from(self.full_health) / f64::from(self.capacity())) as f32
    }

    /// The work that `health` stands for, for a map whose regions exist and
    /// health that is finite and not negative.
    pub fn work(self, health: f32) -> f32 {
        debug_assert!(health.is_finite() && health >= 0.0, "health {health}");
        (f64::from(health) * f64::from(self.capacity()) / f64::from(self.full_health)) as f32
    }
}

impl ShipState {
    /// The hull of the form the ship has now. Outside a match a pod's health
    /// tracks its rebuild, not a hull, so nothing may charge a pod there.
    pub fn health_map(&self) -> HealthMap {
        match self.form {
            ShipForm::Ship => HealthMap::ship_hull(self.life_max),
            ShipForm::EscapePod => HealthMap::pod_hull(),
        }
    }
}

impl RoverState {
    pub fn health_map(self) -> HealthMap {
        HealthMap::rover_hull(self.life_max)
    }
}

impl DebrisState {
    pub fn health_map(&self) -> HealthMap {
        HealthMap::debris(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perimeter(points: &[Vec2]) -> f32 {
        (0..points.len())
            .map(|i| (points[(i + 1) % points.len()] - points[i]).length())
            .sum()
    }

    fn ship(health_percent: u32) -> ShipState {
        ShipState::new(0, Vec2::ZERO, Color::WHITE, health_percent, 1.0 / 60.0)
    }

    fn pod() -> ShipState {
        let mut pod = ship(100);
        pod.form = ShipForm::EscapePod;
        pod.life = POD_HULL_HEALTH;
        pod
    }

    fn asteroid(radius: f32) -> DebrisState {
        DebrisState::new(
            DebrisKind::Asteroid,
            Vec2::ZERO,
            Vec2::ZERO,
            radius,
            ASTEROID_DAMAGE_SCALAR,
            Color::WHITE,
        )
    }

    fn fragment() -> DebrisState {
        DebrisState::new_fragment(
            Vec2::ZERO,
            [
                Vec2::new(-1.0, 0.0),
                Vec2::new(1.5, -1.0),
                Vec2::new(0.5, 2.0),
            ],
            Vec2::ZERO,
            0.0,
            Color::WHITE,
        )
    }

    fn shell() -> DebrisState {
        DebrisState::new_shell(0, 0, Vec2::ZERO, Vec2::X * CANNON_SHELL_SPEED, 0.0)
    }

    fn round() -> DebrisState {
        let mut round = shell();
        round.inertial_mass = Some(weapons::ROUND_MASS);
        round
    }

    fn maps() -> Vec<(&'static str, HealthMap)> {
        vec![
            ("ship", ship(100).health_map()),
            ("ship at 500 percent", ship(500).health_map()),
            ("pod", pod().health_map()),
            ("pilot", HealthMap::pilot()),
            ("rover", RoverState::new(1, 0, 0, 0).health_map()),
            ("grit", asteroid(0.5).health_map()),
            ("boulder", asteroid(6.0).health_map()),
            ("fragment", fragment().health_map()),
            ("shell", shell().health_map()),
            ("round", round().health_map()),
        ]
    }

    #[test]
    fn hulls_follow_the_collision_shapes() {
        let open = physics::ship_collision_hull_for(ShipForm::Ship, 0.0);
        assert!((perimeter(&open) - SHIP_HULL_PERIMETER).abs() < 1.0e-3);
        let pod = physics::ship_collision_hull_for(ShipForm::EscapePod, 0.0);
        assert!((perimeter(&pod) - POD_HULL_PERIMETER).abs() < 1.0e-3);
        let rover = physics::spacewars_rover_spec();
        let chassis = 4.0 * (rover.chassis_half_width + rover.chassis_half_height);
        assert!((chassis - ROVER_HULL_PERIMETER).abs() < 1.0e-5);
        assert!((PILOT_AREA - 0.250_686).abs() < 1.0e-5);
        // Each section is the plate or tissue the volume counts.
        let hulls = [
            (HealthMap::ship_hull(SHIP_HEALTH), SHIP_HULL_PERIMETER),
            (HealthMap::pod_hull(), POD_HULL_PERIMETER),
            (HealthMap::rover_hull(ROVER_LIFE_MAX), ROVER_HULL_PERIMETER),
        ];
        for (map, perimeter) in hulls {
            let plate = perimeter * map.section.depth * map.section.thickness;
            assert!((plate - map.volume).abs() < 1.0e-5 * map.volume, "{map:?}");
        }
        let pilot = HealthMap::pilot();
        assert_eq!(PILOT_AREA * pilot.section.thickness, pilot.volume);
    }

    #[test]
    fn calibration_changes_only_toughness() {
        for (calibrated, reference) in [
            (SHIP_HULL, HULL),
            (POD_HULL, HULL),
            (PILOT_TISSUE, SOFT_TISSUE),
            (ASTEROID_ROCK, ROCK),
        ] {
            let restored = Material {
                name: reference.name,
                toughness: reference.toughness,
                ..calibrated
            };
            assert_eq!(restored, reference, "{}", calibrated.name);
        }
    }

    #[test]
    fn regions_hold_what_the_design_note_lists() {
        let close = |value: f32, expected: f32| (value - expected).abs() <= 1.0e-3 * expected;
        assert_eq!(ship(100).health_map(), HealthMap::ship_hull(100.0));
        assert_eq!(pod().health_map(), HealthMap::pod_hull());
        // Each section is its thickness into the screen, then its depth.
        let rows = [
            (ship(100).health_map(), 88_455.0, 884.55, (2.0, 0.2)),
            (pod().health_map(), 17_724.0, 177.24, (1.0, 0.2)),
            (HealthMap::pilot(), 2_632.2, 26.322, (0.3, 0.15)),
            (
                RoverState::new(1, 0, 0, 0).health_map(),
                8_960.0,
                358.4,
                (2.0, 0.2),
            ),
            (asteroid(3.0).health_map(), 13_195.0, 1_400.0, (2.0, 1.0)),
            (shell().health_map(), 4_712.4, 4_712.4, (2.0, 2.0)),
            (round().health_map(), 375.0, 375.0, (2.0, 2.0)),
        ];
        for (map, capacity, per_point, (thickness, depth)) in rows {
            assert!(close(map.capacity(), capacity), "{map:?}");
            assert!(close(map.work_per_health(), per_point), "{map:?}");
            assert!(close(map.section.thickness, thickness), "{map:?}");
            assert!(close(map.section.depth, depth), "{map:?}");
        }
    }

    #[test]
    fn a_region_is_its_body_health_over_full_health() {
        for (name, map) in maps() {
            let full = map.full_health;
            let whole = map.region(full).unwrap();
            assert_eq!(whole.capacity(), map.capacity(), "{name}");
            assert_eq!(
                f64::from(whole.capacity()),
                whole.remaining_precise(),
                "{name}"
            );
            assert_eq!(whole.integrity(), 1.0, "{name}");
            assert_eq!(map.region(0.5 * full).unwrap().integrity(), 0.5, "{name}");
            assert_eq!(map.region(0.25 * full).unwrap().integrity(), 0.25, "{name}");
            assert_eq!(map.region(2.0 * full), Some(whole), "{name}");
            assert!(map.region(0.0).unwrap().failed(), "{name}");
            assert!(map.region(-5.0).unwrap().failed(), "{name}");
            assert_eq!(map.region(f32::NAN), None, "{name}");
            assert_eq!(map.region(f32::INFINITY), Some(whole), "{name}");
        }
        for full_health in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let map = HealthMap {
                full_health,
                ..HealthMap::pilot()
            };
            assert_eq!(map.region(1.0), None, "{full_health}");
        }
    }

    #[test]
    fn work_and_health_convert_both_ways() {
        for (name, map) in maps() {
            let full = map.full_health;
            assert_eq!(map.work(full), map.capacity(), "{name}");
            assert_eq!(map.health(map.capacity()), full, "{name}");
            assert_eq!(map.health(map.work(0.0)), 0.0, "{name}");
            for health in [0.5, 0.25 * full, 0.8 * full] {
                let back = map.health(map.work(health));
                assert!((back - health).abs() <= 1.0e-6 * full, "{name} at {health}");
            }
            let region = map.region(0.75 * full).unwrap();
            let left = map.health(region.remaining());
            assert!((left - 0.75 * full).abs() <= 1.0e-6 * full, "{name}");
            let per_point = map.work(1.0);
            assert!((per_point - map.work_per_health()).abs() <= 1.0e-6 * per_point);
        }
    }

    #[test]
    fn health_a_ship_is_built_with_adds_hull_of_the_same_toughness() {
        let standard = ship(100).health_map();
        let durable = ship(500).health_map();
        assert_eq!(durable.full_health, 500.0);
        assert!(
            (durable.capacity() - 5.0 * standard.capacity()).abs() < 1.0e-3 * durable.capacity()
        );
        let ratio = durable.work_per_health() / standard.work_per_health();
        assert!((ratio - 1.0).abs() < 1.0e-6);
        assert_eq!(standard.capacity(), SHIP_HULL.toughness * SHIP_HULL_VOLUME);
        let rover = HealthMap::rover_hull(ROVER_LIFE_MAX);
        let sturdy = HealthMap::rover_hull(2.0 * ROVER_LIFE_MAX);
        assert_eq!(sturdy.capacity(), 2.0 * rover.capacity());
        assert_eq!(sturdy.work_per_health(), rover.work_per_health());
    }

    #[test]
    fn debris_regions_weigh_what_the_debris_did_when_made() {
        for debris in [asteroid(0.5), asteroid(3.0), fragment(), shell(), round()] {
            let map = debris.health_map();
            let region = map.region(debris.life).unwrap();
            assert!((region.mass() - debris.mass()).abs() <= 1.0e-5 * debris.mass());
        }
        assert_eq!(shell().health_map().material, SHELL);
        assert_eq!(fragment().health_map().material, ASTEROID_ROCK);
        // Rock takes the same work per point of health at any size.
        let grit = asteroid(0.5).health_map().work_per_health();
        let boulder = asteroid(6.0).health_map().work_per_health();
        assert!((grit - boulder).abs() < 1.0e-4 * boulder);
        // Chipping and shrinking change the body, not what it was made of.
        let mut chipped = asteroid(3.0);
        let map = chipped.health_map();
        chipped.apply_damage(0.1 * chipped.life_max);
        assert!(chipped.radius < chipped.base_radius);
        assert_eq!(chipped.health_map(), map);
        let mut shrunk = fragment();
        let map = shrunk.health_map();
        shrunk.translate_life(-0.1 * shrunk.life_max);
        assert!(shrunk.radius < shrunk.base_radius);
        assert_eq!(shrunk.health_map(), map);
    }

    #[test]
    fn debris_breaks_by_the_game_rule_before_its_region_fails() {
        let mut rock = asteroid(3.0);
        let left = (DEBRIS_DEATH_LIFE_FACTOR - 0.01) * rock.life_max;
        let region = rock.health_map().region(left).unwrap();
        assert!(!region.failed());
        assert!((region.integrity() - (DEBRIS_DEATH_LIFE_FACTOR - 0.01)).abs() < 1.0e-6);
        rock.apply_damage(rock.life_max - left);
        assert!(rock.dead);
    }

    #[test]
    fn the_rules_that_change_health_change_the_region() {
        let mut ship = ship(100);
        ship.translate_life_with_impulse(-25.0, Vec2::X);
        assert_eq!(
            ship.health_map().region(ship.life).unwrap().integrity(),
            0.75
        );
        ship.heal(5.0);
        assert_eq!(
            ship.health_map().region(ship.life).unwrap().integrity(),
            0.8
        );
        let mut rover = RoverState::new(1, 0, 0, 0);
        rover.apply_damage(5.0);
        assert_eq!(
            rover.health_map().region(rover.life).unwrap().integrity(),
            0.8
        );
        rover.apply_damage(25.0);
        assert!(rover.health_map().region(rover.life).unwrap().failed());
    }
}
