//! Test-only, post-physics observations. The lab retains only the last two
//! seconds before departure; there is no production sampling or logging cost.
use super::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct Contact {
    entity: u64,
    normal: [f32; 2],
    velocity: [f32; 2],
    separation: f32,
}

#[derive(Debug, Serialize)]
pub(crate) struct Sample {
    pub tick: u64,
    position: [f32; 2],
    velocity: [f32; 2],
    support: Option<usize>,
    grounded: bool,
    contacts: Vec<Contact>,
    behavior: engine_common::ClockDuckBehavior,
    direction: f32,
    target: Option<usize>,
    landing: Option<[f32; 2]>,
    flight_tick: Option<u32>,
    submerged: f64,
    recovery: engine_common::ClockDuckRecoveryState,
}

impl DuckEvent {
    pub(crate) fn trace_sample(&self) -> Option<Sample> {
        let world = self.world.as_ref()?;
        let motion = world.motion(DUCK_BODY)?;
        Some(Sample {
            tick: self.tick,
            // Entrance-relative X, world Y, like the controller. Public
            // navigation positions remain screen-space (see entrance side).
            position: [self.position()?.x, motion.position.y],
            velocity: [
                motion.linear_velocity.x * self.direction,
                motion.linear_velocity.y,
            ],
            support: self.supported_surface(),
            grounded: self.grounded(),
            contacts: world
                .surface_contacts(DUCK_COLLIDER)
                .take(8)
                .map(|c| Contact {
                    entity: c.collider.entity.value(),
                    normal: [c.normal.x * self.direction, c.normal.y],
                    velocity: [c.velocity.x * self.direction, c.velocity.y],
                    separation: c.separation,
                })
                .collect(),
            behavior: self.controller.behavior,
            direction: self.controller.direction,
            target: self.controller.navigator.plan.map(|p| p.target),
            landing: self
                .controller
                .navigator
                .plan
                .map(|p| [p.landing.x, p.landing.y + self.layout.floor_y]),
            flight_tick: self.controller.navigator.flight_tick,
            submerged: self.water_report.submerged_fraction,
            recovery: self.controller.recovery.stats,
        })
    }
}
