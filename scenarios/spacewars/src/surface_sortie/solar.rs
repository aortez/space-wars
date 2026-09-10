//! Solar heat in material combat scenes; collision response stays in the solver.
use super::*;

pub(super) const CORONA_WIDTH: f32 = 24.0;
const MAX_DAMAGE_FRACTION_PER_SECOND: f32 = 0.20;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SolarExposure {
    /// Linear falloff across the corona, measured at the full ship's pivot.
    pub intensity: f32,
    pub damage_percent_per_second: f32,
}

fn exposure(sun: SunState, ship: &ShipState) -> SolarExposure {
    let altitude =
        (ship.position + physics::ship_pivot(ship.form)).distance_to(sun.position) - sun.radius;
    let intensity = if !ship.dead && ship.form == ShipForm::Ship {
        (1.0 - altitude / CORONA_WIDTH).clamp(0.0, 1.0)
    } else {
        0.0
    };
    SolarExposure {
        intensity,
        damage_percent_per_second: intensity * MAX_DAMAGE_FRACTION_PER_SECOND * 100.0,
    }
}

impl SurfaceSortieState {
    pub fn solar_exposure(&self, player: usize) -> Option<SolarExposure> {
        let pilot = self.pilots.get(player)?;
        pilot.combat.as_ref()?;
        Some(exposure(
            self.world.sun?,
            &self.world.ships[pilot.vehicle.0],
        ))
    }
}

pub(crate) fn apply_damage(world: &mut SpacewarsState, pilots: &mut [SurfacePilot], dt: f32) {
    let Some(sun) = world.sun else { return };
    for pilot in pilots {
        if pilot.combat.is_none() {
            continue;
        }
        let ship = &mut world.ships[pilot.vehicle.0];
        let heat = exposure(sun, ship);
        let damage = ship.life_max * heat.damage_percent_per_second * 0.01 * dt;
        if damage > 0.0 {
            // Heat changes health only. The normal shared death path produces
            // wreckage and a pod, or preserves an already external spaceling.
            ship.translate_life_with_impulse(-damage, Vec2::ZERO);
            pilot.damage.last_solar_damage_tick = Some(world.tick + 1);
        }
    }
}
