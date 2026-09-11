//! Optional physical lift and a finite charge supply. Disabled in historical fixtures.
use super::*;

pub const THRUST: f32 = 40.0;
pub const BURN_SECONDS: f32 = 3.0;
pub const RECHARGE_SECONDS: f32 = 4.0;
pub const RISE_SPEED: f32 = 9.0;
pub const AIR_SPEED: f32 = 10.0;
pub const AIR_ACCELERATION: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JetpackSnapshot {
    pub charge: f32,
    pub active: bool,
    /// Inertial velocity saved at takeoff; airborne steering uses this frame.
    pub reference_velocity: Vec2,
    /// Full-thrust equivalent time; partial thrust consumes proportionally.
    pub burn_seconds: f32,
}

#[derive(Clone)]
pub(super) struct Jetpack {
    pub charge: f32,
    pub active: bool,
    pub armed: bool,
    pub reference_velocity: Vec2,
    pub burn_seconds: f32,
}

impl SpacelingAssembly {
    /// Scenario-owned equipment setup; preserves charge through boarding/exiting.
    pub fn equip_jetpack(&mut self, charge: f32) -> bool {
        if !charge.is_finite() || !(0.0..=1.0).contains(&charge) {
            return false;
        }
        self.jetpack = Some(Jetpack {
            charge,
            active: false,
            armed: false,
            reference_velocity: Vec2::ZERO,
            burn_seconds: 0.0,
        });
        true
    }

    pub fn jetpack(&self) -> Option<JetpackSnapshot> {
        self.jetpack.as_ref().map(|pack| JetpackSnapshot {
            charge: pack.charge,
            active: pack.active,
            reference_velocity: pack.reference_velocity,
            burn_seconds: pack.burn_seconds,
        })
    }
}
