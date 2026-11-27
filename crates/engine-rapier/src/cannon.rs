//! A tank's cannon, as tanks fire it wherever they fight.
//!
//! Scorched Earth's duel, which the Clock's Tanks event hosts, fires by these
//! rules and is their testbed. A cannon's barrel turns on a pivot above its
//! hull. It aims `elevation` above the horizontal toward one side, its
//! `facing`, where the horizontal is square to the world's up rather than to
//! the hull, so a tilted tank aims as a level one does. A round leaves the
//! barrel at its power, carried by the hull's velocity. Lengths and speeds are
//! in the firing game's own units, which a host such as the Clock may scale,
//! and ticks are fixed steps of a sixtieth of a second.

use engine_core::Vec2;

/// The lowest a barrel aims above the horizontal, in radians.
pub const MIN_ELEVATION: f32 = 5.0_f32.to_radians();
/// The highest a barrel aims above the horizontal, in radians.
pub const MAX_ELEVATION: f32 = 85.0_f32.to_radians();
/// The slowest a round leaves the barrel, relative to its hull.
pub const MIN_POWER: f32 = 12.0;
/// The fastest a round leaves the barrel, relative to its hull, across a field
/// as wide as Scorched's own. A host may raise it for a wider field.
pub const MAX_POWER: f32 = 48.0;
/// The ticks a cannon takes to reload after it fires.
pub const RELOAD_TICKS: u64 = 75;
/// The most rounds in flight in one world, every cannon's together.
pub const MAX_ROUNDS: usize = 8;
/// The most rounds in flight from one cannon, so that no cannon holds every
/// round its world has room for.
pub const MAX_ROUNDS_PER_CANNON: usize = 4;
/// The ticks a round flies before it expires.
pub const ROUND_LIFETIME_TICKS: u32 = 600;
/// The ticks a round passes through the tank that fired it, which carry it
/// clear of its own hull.
pub const CLEARANCE_TICKS: u32 = 8;
/// How far the barrel's pivot stands from the hull's origin, along the hull's
/// up.
pub const PIVOT_HEIGHT: f32 = 0.85;
/// How far the muzzle stands from the pivot.
pub const BARREL_LENGTH: f32 = 2.8;

/// The way a barrel points when it aims `elevation` above the horizontal
/// toward `facing`: 1 for the side clockwise of the unit vector `up`, which is
/// +x when up is +y, and -1 for the other side.
pub fn direction(elevation: f32, facing: f32, up: Vec2) -> Vec2 {
    let across = Vec2::new(up.y, -up.x);
    across * (facing * elevation.cos()) + up * elevation.sin()
}

/// A cannon's barrel: the pivot it turns on and the muzzle its rounds leave
/// by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Barrel {
    pub pivot: Vec2,
    pub muzzle: Vec2,
}

impl Barrel {
    /// The barrel of a hull whose origin is at `position`, turned `angle`,
    /// pointing along the unit `direction`.
    pub fn new(position: Vec2, angle: f32, direction: Vec2) -> Self {
        let pivot = position + Vec2::Y.rotate_radians(angle) * PIVOT_HEIGHT;
        Self {
            pivot,
            muzzle: pivot + direction * BARREL_LENGTH,
        }
    }

    /// Where a round leaves the barrel. A barrel can't fire through the ground
    /// or another body, so a round leaves where something stands in the
    /// barrel, or else at the muzzle. `cast` traces from the pivot along
    /// `muzzle - pivot`, no farther than the muzzle and past the firing tank's
    /// own bodies, and answers the first point it meets.
    pub fn exit(self, cast: impl FnOnce(Vec2, Vec2) -> Option<Vec2>) -> Vec2 {
        cast(self.pivot, self.muzzle - self.pivot).unwrap_or(self.muzzle)
    }
}

/// Why a cannon holds its fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// It is still reloading from its last shot.
    Reloading,
    /// [`MAX_ROUNDS_PER_CANNON`] of its rounds are in flight.
    OwnRounds,
    /// [`MAX_ROUNDS`] rounds are in flight in its world.
    Rounds,
}

/// Whether a cannon that has reloaded by tick `reloaded` may fire at `tick`,
/// with `own` of its rounds and `all` rounds in its world in flight.
pub fn may_fire(tick: u64, reloaded: u64, own: usize, all: usize) -> Result<(), Hold> {
    if tick < reloaded {
        return Err(Hold::Reloading);
    }
    room(own, all)
}

/// Whether a world with `all` rounds in flight, `own` of them a cannon's, has
/// room for another round from that cannon.
pub fn room(own: usize, all: usize) -> Result<(), Hold> {
    if all >= MAX_ROUNDS {
        Err(Hold::Rounds)
    } else if own >= MAX_ROUNDS_PER_CANNON {
        Err(Hold::OwnRounds)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn a_barrel_aims_its_elevation_above_the_horizontal_of_its_up() {
        let elevation = 30.0_f32.to_radians();
        let level = direction(elevation, 1.0, Vec2::Y);
        assert_eq!(level, Vec2::new(elevation.cos(), elevation.sin()));
        assert_eq!(
            direction(elevation, -1.0, Vec2::Y),
            Vec2::new(-elevation.cos(), elevation.sin())
        );
        // On a planet's left flank up is -x, and the side clockwise of it is
        // +y.
        let flank = direction(elevation, 1.0, Vec2::new(-1.0, 0.0));
        assert!(near(flank, Vec2::new(-elevation.sin(), elevation.cos())));
    }

    #[test]
    fn the_pivot_tilts_with_the_hull_and_the_barrel_aims_from_the_world() {
        let aim = direction(50.0_f32.to_radians(), 1.0, Vec2::Y);
        let angle = 0.4;
        let barrel = Barrel::new(Vec2::new(3.0, 1.0), angle, aim);
        let up = Vec2::Y.rotate_radians(angle);
        assert!(near(barrel.pivot, Vec2::new(3.0, 1.0) + up * PIVOT_HEIGHT));
        assert!(near(barrel.muzzle - barrel.pivot, aim * BARREL_LENGTH));
    }

    #[test]
    fn a_round_leaves_where_something_stands_in_the_barrel() {
        let barrel = Barrel::new(Vec2::ZERO, 0.0, Vec2::X);
        let block = Vec2::new(1.0, PIVOT_HEIGHT);
        let exit = barrel.exit(|from, along| {
            assert_eq!((from, along), (barrel.pivot, Vec2::X * BARREL_LENGTH));
            Some(block)
        });
        assert_eq!(exit, block);
        assert_eq!(barrel.exit(|_, _| None), barrel.muzzle);
    }

    #[test]
    fn a_cannon_holds_fire_while_it_reloads_or_its_rounds_fill_the_room() {
        assert_eq!(may_fire(74, 75, 0, 0), Err(Hold::Reloading));
        assert_eq!(may_fire(75, 75, 0, 0), Ok(()));
        assert_eq!(
            may_fire(75, 75, MAX_ROUNDS_PER_CANNON, MAX_ROUNDS_PER_CANNON),
            Err(Hold::OwnRounds)
        );
        assert_eq!(may_fire(75, 75, 0, MAX_ROUNDS), Err(Hold::Rounds));
        assert_eq!(may_fire(74, 75, 0, MAX_ROUNDS), Err(Hold::Reloading));
        assert_eq!(room(MAX_ROUNDS_PER_CANNON - 1, MAX_ROUNDS - 1), Ok(()));
    }
}
