use super::*;
use engine_rapier::world::polar::{
    PolarSensor,
    policy::{CreatureBody, CreatureContext, CreatureObservation, ObjectKind},
};

impl crate::ClockState {
    /// Prepare once for this visit, whose diameter depends on the case's aspect ratio.
    pub fn duck_polar_sensor(&self) -> Result<PolarSensor, &'static str> {
        let duck = self.duck_visit.as_ref().ok_or("no duck visit")?;
        PolarSensor::for_body_length(2. * duck.radius)
    }

    /// Read the live actor and completed-step arena in world coordinates. A
    /// visit only has an observable body between spawn and the terminal outcome.
    pub fn duck_polar_observation(
        &self,
        sensor: &PolarSensor,
        context: CreatureContext,
    ) -> Result<CreatureObservation, &'static str> {
        let duck = self.duck_visit.as_ref().ok_or("no duck visit")?;
        let world = self
            .arena
            .held(Owner::Visit)
            .ok_or("duck arena unavailable")?;
        let diameter = 2. * duck.radius;
        let support = world.surface_contacts(DUCK_COLLIDER).find(|contact| {
            !duck.ledge_attached()
                && contact.normal.y > 0.7
                && contact.separation <= duck.radius * 0.05
        });
        let body = CreatureBody {
            id: DUCK_BODY,
            body_length: diameter,
            width: diameter,
            height: diameter,
            top_speed: duck.movement.speed(Gait::Sprint(1.0)),
            jump_height: duck.movement.jump_height,
            gravity: world.gravity()
                * world
                    .gravity_scale(DUCK_BODY)
                    .ok_or("duck has not spawned")?,
            health_fraction: 1.0,
            support,
            can_run: true,
            can_dive: true,
        };
        body.observe(world, sensor, context, |id| {
            if id == flaps::body(0) {
                ObjectKind::ClockEntrance
            } else if id == flaps::body(1) && duck.exit_visible() {
                ObjectKind::ClockExit
            } else {
                ObjectKind::OtherSolid
            }
        })
    }
}
