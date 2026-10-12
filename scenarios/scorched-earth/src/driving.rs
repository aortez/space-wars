use super::*;
use engine_rapier::rover::RoverControl;
use engine_rapier::world::SurfaceDrive;
mod footing;
mod support;
pub use footing::FootingTarget;
mod goal;
pub use goal::{GoalDecision, TravelGoal};

const DRIVE_SPEED: f32 = 2.5;
const DRIVE_ACCELERATION: f32 = 5.0;
const PLAN_TICKS: u64 = 30;
pub(super) const ROUTE_SAMPLES: usize = 9;
const ROUTE_SPACING: f32 = CELL_SIZE;
// One terrain cell is a demonstrated obstacle, not permission to descend into
// a two-unit trench. The small tolerance covers contact/interpolation noise.
const MAX_STEP: f32 = CELL_SIZE + 0.08;
const PROFILE_TOLERANCE: f32 = 0.10;

pub(super) type GroundCheckFn = fn(&ScorchedState, usize, f32) -> bool;
pub(super) type SupportCheckFn = fn(&ScorchedState, usize) -> bool;
pub(super) const CURRENT_GROUND_CHECKS: [GroundCheckFn; 2] =
    [ScorchedState::safe_drive_direction; 2];
pub(super) const CURRENT_SUPPORT_CHECKS: [SupportCheckFn; 2] =
    [ScorchedState::has_track_support; 2];

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DrivingDiagnostics {
    /// Track command, not a promise of chassis speed on loose/sloping ground.
    pub track_speed: f32,
    pub powered_ticks: u64,
    pub planning_checks: u64,
    pub distance_while_driven: f32,
    pub mode: &'static str,
}

#[derive(Clone, Serialize)]
pub(super) struct TankDrive {
    pub diagnostics: DrivingDiagnostics,
    last_position: Vec2,
    planned_x: f32,
    next_plan: u64,
    retry_at: u64,
    cpu_direction: f32,
    stalled_plans: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    footing: Option<footing::FootingDriver>,
}

impl TankDrive {
    pub fn new(position: Vec2) -> Self {
        Self {
            diagnostics: DrivingDiagnostics {
                track_speed: 0.0,
                powered_ticks: 0,
                planning_checks: 0,
                distance_while_driven: 0.0,
                mode: "idle",
            },
            last_position: position,
            planned_x: position.x,
            next_plan: 0,
            retry_at: 0,
            cpu_direction: 0.0,
            stalled_plans: 0,
            footing: None,
        }
    }

    pub fn has_history(&self) -> bool {
        self.diagnostics.powered_ticks > 0 || self.diagnostics.planning_checks > 0
    }
}

impl ScorchedState {
    pub fn driving_diagnostics(&self) -> [DrivingDiagnostics; 2] {
        self.driving.each_ref().map(|drive| drive.diagnostics)
    }

    pub(super) fn clear_driving_input(&mut self) {
        self.manual_drive_armed = false;
        self.pointer_drive = 0.0;
        self.nudge_drive = 0.0;
        self.nudge_until = 0;
        for drive in &mut self.driving {
            drive.diagnostics.track_speed = 0.0;
            drive.cpu_direction = 0.0;
            drive.next_plan = 0;
            if let Some(footing) = &mut drive.footing {
                footing.suspend();
            }
        }
    }

    pub(super) fn safe_drive_direction(&self, player: usize, direction: f32) -> bool {
        self.check_drive_direction(player, direction)
            .reason
            .is_none()
    }

    pub(super) fn check_drive_direction(
        &self,
        player: usize,
        direction: f32,
    ) -> driving_lab::GroundCheck {
        use driving_lab::{GroundCheck, GroundProbe};
        let mut result = GroundCheck {
            direction,
            reason: None,
            probes: [None; ROUTE_SAMPLES],
        };
        let tank = self.tanks[player];
        let motion = self.physics.motion(tank.body).unwrap();
        let clearance = if self.running_gear[player].is_some() {
            1.2 + (self.config.wheel_tuning.radius - WheelTuning::default().radius)
        } else {
            0.8
        };
        let foot = motion.position + Vec2::new(0.0, -clearance).rotate_radians(motion.angle);
        let support_span = if self.running_gear[player].is_some() {
            suspension::spec(self.config.wheel_tuning).wheel_offset * 2.0
        } else {
            3.0 // Width of the rigid hull's lower contact face.
        };
        // Sample actual collision surfaces at cell-sized intervals. Small 45°
        // interpolation faces need not make the whole route a 45° hill.
        for index in 0..ROUTE_SAMPLES {
            let x = motion.position.x + direction * index as f32 * ROUTE_SPACING;
            result.probes[index] = Some(GroundProbe {
                x,
                point: None,
                normal: None,
            });
            if x.abs() > 46.0 {
                result.reason = Some("world edge");
                return result;
            }
            let Some(hit) = self.physics.cast_ray(
                Vec2::new(x, foot.y + 4.0),
                -Vec2::Y,
                RayCastOptions {
                    max_distance: 8.0,
                    exclude_entity: Some(tank.body.entity),
                    ..Default::default()
                },
            ) else {
                result.reason = Some("no ground ahead");
                return result;
            };
            let probe = result.probes[index].as_mut().unwrap();
            probe.point = Some([hit.point.x, hit.point.y]);
            probe.normal = Some([hit.normal.x, hit.normal.y]);
            result.reason = if self
                .tanks
                .iter()
                .any(|other| other.body.entity == hit.collider.entity)
            {
                Some("tank ahead")
            } else if hit.normal.y <= 0.0 {
                Some("no upward ground")
            } else {
                None
            };
            if result.reason.is_some() {
                return result;
            }
            if index > 0 {
                let previous = result.probes[index - 1].unwrap().point.unwrap();
                let rise = hit.point.y - previous[1];
                if rise > MAX_STEP {
                    result.reason = Some("step too high");
                    return result;
                }
                if rise < -MAX_STEP {
                    result.reason = Some("drop ahead");
                    return result;
                }
            }
            // Compare every available span at least as long as the supports.
            // This retains a slope limit even for a staircase of small steps.
            for earlier in result.probes[..index].iter().flatten() {
                let previous = earlier.point.unwrap();
                let run = (hit.point.x - previous[0]).abs();
                if run >= support_span
                    && (hit.point.y - previous[1]).abs()
                        > run * 40.0_f32.to_radians().tan() + PROFILE_TOLERANCE
                {
                    result.reason = Some("steep profile");
                    return result;
                }
            }
        }
        result
    }

    fn cpu_drive(
        &mut self,
        player: usize,
        ground_check: GroundCheckFn,
        support_check: SupportCheckFn,
    ) -> f32 {
        let tank = self.tanks[player];
        let motion = self.physics.motion(tank.body).unwrap();
        if !support_check(self, player) {
            self.driving[player].diagnostics.mode = "no support";
            self.driving[player].cpu_direction = 0.0;
            return 0.0;
        }
        if tank.next_shot <= self.tick + 60 && self.tick < tank.next_shot.saturating_add(180) {
            self.driving[player].diagnostics.mode = "aim and fire";
            self.driving[player].cpu_direction = 0.0;
            self.driving[player].next_plan = self.tick;
            return 0.0;
        }
        if self.tick >= tank.next_shot.saturating_add(180) {
            // A moving/unstable support must not trap the CPU in an unlimited
            // wait for a perfect firing pose. Skip this shot and reposition.
            self.tanks[player].next_shot = self.tick + 120;
        }
        if self.tick < self.driving[player].retry_at {
            self.driving[player].diagnostics.mode = "cooldown";
            return 0.0;
        }
        if self.tick >= self.driving[player].next_plan {
            let drive = &mut self.driving[player];
            drive.diagnostics.planning_checks += 1;
            drive.next_plan = self.tick + PLAN_TICKS;
            if drive.cpu_direction != 0.0 && (motion.position.x - drive.planned_x).abs() < 0.15 {
                drive.stalled_plans += 1;
            } else {
                drive.stalled_plans = 0;
            }
            drive.planned_x = motion.position.x;
            if drive.stalled_plans >= 3 {
                drive.stalled_plans = 0;
                drive.cpu_direction = 0.0;
                drive.retry_at = self.tick + 180;
                drive.diagnostics.mode = "cooldown";
                return 0.0;
            }
            let target = self
                .physics
                .motion(self.tanks[1 - player].body)
                .unwrap()
                .position;
            let toward = if target.x >= motion.position.x {
                1.0
            } else {
                -1.0
            };
            let distance = (target.x - motion.position.x).abs();
            let (pivot, muzzle) = self.muzzle(player);
            let blocked_barrel = self
                .physics
                .cast_ray(
                    pivot,
                    muzzle - pivot,
                    RayCastOptions {
                        max_distance: (muzzle - pivot).length(),
                        exclude_entity: Some(tank.body.entity),
                        ..Default::default()
                    },
                )
                .is_some();
            let (direction, mode) = if distance > 32.0 {
                (toward, "approach")
            } else if distance < 18.0 {
                (-toward, "make room")
            } else if blocked_barrel {
                (-toward, "clear barrel")
            } else {
                (0.0, "hold range")
            };
            self.driving[player].cpu_direction = direction;
            self.driving[player].diagnostics.mode = mode;
        }
        let direction = self.driving[player].cpu_direction;
        // Recheck live ground while moving: a crater can invalidate the plan
        // between its half-second decisions. A refusal brakes immediately.
        if direction != 0.0 && !ground_check(self, player, direction) {
            self.driving[player].cpu_direction = 0.0;
            self.driving[player].diagnostics.mode = "blocked ground";
            self.driving[player].next_plan = self.tick + 60;
            return 0.0;
        }
        direction
    }

    pub(super) fn ready_to_shoot(&self, player: usize, support_check: SupportCheckFn) -> bool {
        let motion = self.physics.motion(self.tanks[player].body).unwrap();
        support_check(self, player)
            && motion.linear_velocity.length() < 0.6
            && motion.angular_velocity.abs() < 0.35
            && self.driving[player].diagnostics.track_speed.abs() < 0.2
    }

    pub(super) fn prepare_driving(
        &mut self,
        manual: f32,
        override_axis: Option<(usize, f32)>,
        ground_checks: [GroundCheckFn; 2],
        support_checks: [SupportCheckFn; 2],
    ) -> Vec<SurfaceDrive> {
        let playable = self.fighting()
            && !self.config.undercut
            && self.config.drop_height.is_none()
            && self.config.bombardment_seconds == 0;
        let cpu_enabled =
            playable && self.config.cpu_driving && !self.config.hillside && !self.config.roof;
        let mut drives = Vec::new();
        for (player, ground_check) in ground_checks.into_iter().enumerate() {
            let is_cpu = self.config.demo || player != self.selected;
            let axis = if self.config.course.is_some() && player == 1 {
                self.driving[player].diagnostics.mode = "parked";
                0.0
            } else if let Some((_, axis)) = override_axis.filter(|(i, _)| *i == player) {
                axis
            } else if is_cpu && cpu_enabled {
                let axis = self.cpu_drive(player, ground_check, support_checks[player]);
                if self.config.cpu_footing[player] && self.config.course.is_none() {
                    self.seek_firing_footing(player, axis)
                } else {
                    axis
                }
            } else if playable && !is_cpu {
                self.driving[player].diagnostics.mode = "manual";
                manual
            } else {
                0.0
            };
            let supported = (axis != 0.0
                || self.driving[player].diagnostics.track_speed != 0.0
                || self.running_gear[player].is_some())
                && support_checks[player](self, player);
            let motion = self.physics.motion(self.tanks[player].body).unwrap();
            let drive = &mut self.driving[player];
            if drive.diagnostics.track_speed != 0.0 {
                drive.diagnostics.distance_while_driven +=
                    motion.position.distance_to(drive.last_position);
            }
            drive.last_position = motion.position;
            if !playable || !supported {
                drive.diagnostics.track_speed = 0.0;
            } else {
                let target = axis * DRIVE_SPEED;
                drive.diagnostics.track_speed += (target - drive.diagnostics.track_speed)
                    .clamp(-DRIVE_ACCELERATION * DT, DRIVE_ACCELERATION * DT);
            }
            if let Some(gear) = &self.running_gear[player] {
                let moving = drive.diagnostics.track_speed.abs() > 0.001;
                if moving {
                    drive.diagnostics.powered_ticks += 1;
                }
                gear.set_control(
                    &mut self.physics,
                    RoverControl {
                        throttle: drive.diagnostics.track_speed / DRIVE_SPEED,
                        brake: playable && supported && !moving,
                    },
                );
                continue;
            }
            if drive.diagnostics.track_speed.abs() > 0.001 {
                drive.diagnostics.powered_ticks += 1;
                drives.push(SurfaceDrive {
                    collider: ColliderId::new(
                        self.tanks[player].body.entity,
                        ColliderRole::PRIMARY,
                        0,
                    ),
                    local_velocity: Vec2::new(-drive.diagnostics.track_speed, 0.0),
                    local_normal: -Vec2::Y,
                    min_normal_dot: 0.5,
                    world_up: Vec2::Y,
                    // The lower bevel wraps the track around its nose. It can
                    // touch a flat tile corner with a steep manifold normal;
                    // the separate live support gate enforces the slope limit.
                    min_up_dot: 0.0,
                });
            }
        }
        drives
    }
}

#[cfg(test)]
pub(super) mod tests;
