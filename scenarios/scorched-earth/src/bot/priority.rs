//! Give one untried observed power correction a bounded chance before evading.
//! This does not change the shot schedule or predict the result of the shot.
use super::*;

const FIRE_WAIT: u64 = 240;
const TOTAL_WAIT: u64 = 360;
const REUSE_DELAY: u64 = 600;

#[derive(Clone, Serialize)]
pub(super) struct ShotPriority {
    evidence: u64,
    geometry: Geometry,
    pub started: u64,
    pub fire_deadline: u64,
    pub deadline: u64,
    pub shot: Option<u64>,
    pub ended: Option<u64>,
    pub reason: &'static str,
}

impl Bot {
    pub(super) fn try_shot_priority(&mut self, state: &ScorchedState, player: usize) -> bool {
        let Some(observation) = self.last_shot else {
            return false;
        };
        if self.pending.is_some()
            || !self.aim.has_power_correction()
            || !observation.launch.calibrated
            || self.aim.correction != observation.assessment
            || state.tick.saturating_sub(observation.tick) > 180
            || (self.aim.power - observation.launch.power).abs() < 0.01
            || state.tanks[player].next_shot > state.tick + FIRE_WAIT
            || !state.has_track_support(player)
            || !state.correction_barrel_clear(player, self.aim.elevation)
            || self.shot_priority.as_ref().is_some_and(|previous| {
                previous.ended.is_none()
                    || previous.evidence == observation.launch.id
                    || state.tick < previous.started + REUSE_DELAY
            })
        {
            return false;
        }
        self.shot_priority = Some(ShotPriority {
            evidence: observation.launch.id,
            geometry: Geometry::read(state, player),
            started: state.tick,
            fire_deadline: state.tick + FIRE_WAIT,
            deadline: state.tick + TOTAL_WAIT,
            shot: None,
            ended: None,
            reason: "try correction",
        });
        true
    }

    pub(super) fn protect_correction(&mut self, state: &ScorchedState, player: usize) -> bool {
        let Some(priority) = self.shot_priority.as_mut().filter(|p| p.ended.is_none()) else {
            return false;
        };
        // Consume incoming hits throughout this one opportunity. They cannot
        // renew either deadline or queue an automatic move after the wait.
        self.considered_hit = self.hit_serial;
        let reason = if state.tick >= priority.deadline {
            Some("observation deadline")
        } else if priority.shot.is_none() && state.tick >= priority.fire_deadline {
            Some("firing deadline")
        } else if !priority.geometry.matches(Geometry::read(state, player)) {
            Some("geometry changed")
        } else if !state.has_track_support(player) {
            Some("no support")
        } else if !state.correction_barrel_clear(player, self.aim.elevation) {
            Some("blocked barrel")
        } else if priority.shot.is_none() && !self.aim.has_power_correction() {
            Some("correction changed")
        } else {
            None
        };
        if let Some(reason) = reason {
            priority.ended = Some(state.tick);
            priority.reason = reason;
            return false;
        }
        self.mode = if priority.shot.is_some() {
            "watching correction"
        } else {
            "try correction"
        };
        true
    }

    pub(super) fn priority_launched(&mut self, id: u64, tick: u64) {
        if let Some(priority) = self.shot_priority.as_mut().filter(|p| p.ended.is_none()) {
            if priority.shot.is_none() && tick >= priority.fire_deadline {
                priority.ended = Some(tick);
                priority.reason = "firing deadline";
            } else if priority.shot.is_none() {
                priority.shot = Some(id);
                self.mode = "watching correction";
            }
        }
    }

    pub(super) fn priority_observed(&mut self, observation: &ShotObservation) {
        if let Some(priority) = self
            .shot_priority
            .as_mut()
            .filter(|p| p.ended.is_none() && p.shot == Some(observation.launch.id))
        {
            priority.ended = Some(observation.tick);
            priority.reason = if observation.tick >= priority.deadline {
                "observation deadline"
            } else {
                observation.assessment
            };
        }
    }
}

impl ScorchedState {
    fn correction_barrel_clear(&self, player: usize, elevation: f32) -> bool {
        let geometry = Geometry::read(self, player);
        let pivot = geometry.shooter + Vec2::Y.rotate_radians(geometry.pitch) * 0.85;
        let facing = (geometry.target.x - geometry.shooter.x).signum();
        let barrel = Vec2::new(facing * elevation.cos(), elevation.sin()) * 2.8;
        self.physics
            .cast_ray(
                pivot,
                barrel,
                RayCastOptions {
                    max_distance: barrel.length(),
                    exclude_entity: Some(self.tanks[player].body.entity),
                    ..Default::default()
                },
            )
            .is_none()
    }
}
