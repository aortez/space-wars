//! Shared movement actuator. Command adapters (#282/#283) supply intent;
//! contacts, one dynamic hull and the visit's existing world own the motion.
use super::*;
use engine_common::{
    ClockDuckLedgePhase as Phase, ClockDuckLedgeReason as Reason, ClockDuckLedgeState,
};

mod automatic;
#[cfg(test)]
mod automatic_tests;
mod geometry;
use geometry::{Edge, clear_path, edges};

const REACH_RADII: f32 = 2.5;
const DRIVE_GAIN: f32 = 16.0;
const MAX_SPEED_RADII: f32 = 8.0;
const MAX_ACCELERATION_RADII: f32 = 240.0;
const DISRUPTION_SPEED_RADII: f32 = 32.0;
const MAX_ERROR_RADII: f32 = 1.25;
const CAPTURE_TICKS: u32 = 30;
const CLIMB_TICKS: u32 = 120;
const REARM_TICKS: u32 = 12;

/// Each command lasts one simulation tick. Idle leaves a valid hang intact.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LedgeCommand {
    #[default]
    Idle,
    Catch,
    Climb,
    Release,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Catching,
    Hanging,
    Rise,
    Across,
    Settle,
}

#[derive(Clone, Copy)]
struct Attachment {
    edge: Edge,
    stage: Stage,
    ticks: u32,
}

#[derive(Default)]
pub(super) struct Ledge {
    command: LedgeCommand,
    catch_target: Option<Edge>,
    attachment: Option<Attachment>,
    pub stats: ClockDuckLedgeState,
    pub automatic: automatic::Automatic,
}

impl DuckEvent {
    /// Shared seam for command adapters and replay fixtures.
    pub fn set_ledge_command(&mut self, command: LedgeCommand) {
        if matches!(self.phase, EventPhase::Running | EventPhase::Exiting) {
            self.ledge.command = command;
            self.ledge.catch_target = None;
        }
    }

    pub(super) fn clear_ledge_command(&mut self) {
        self.ledge.command = LedgeCommand::Idle;
        self.ledge.catch_target = None;
    }

    pub(super) fn decide_player_ledge(&mut self) {
        let Some(player) = &self.player else { return };
        // An explicit actuator command already owns this tick.
        if self.ledge.command != LedgeCommand::Idle {
            return;
        }
        let command = if self.ledge_attached() {
            if player.dive_held {
                LedgeCommand::Release
            } else if player.jump_pending {
                LedgeCommand::Climb
            } else {
                LedgeCommand::Idle
            }
        } else if player.jump_held {
            // Keep requesting Catch after a drop so the shared rearm gate stays
            // closed until Jump is actually released. Idle alone rearms it.
            LedgeCommand::Catch
        } else {
            LedgeCommand::Idle
        };
        self.set_ledge_command(command);
    }

    pub(super) fn ledge_attached(&self) -> bool {
        self.ledge.attachment.is_some()
    }

    pub(super) fn reset_ledge(&mut self) {
        self.release_flap_attachment();
        self.ledge = Ledge::default();
    }

    pub fn ledge_facing(&self) -> Option<f32> {
        self.ledge
            .attachment
            .map(|a| -a.edge.outward * self.direction)
    }

    pub fn ledge_state(&self) -> ClockDuckLedgeState {
        self.ledge.stats
    }

    pub fn ledge_anchor(&self) -> Option<Vec2> {
        self.ledge.attachment?.edge.anchor(self.world.as_ref()?)
    }

    fn drop_ledge(&mut self, reason: Reason) {
        self.ledge.attachment = None;
        self.release_flap_attachment();
        let stats = &mut self.ledge.stats;
        stats.phase = Phase::Dropping;
        stats.reason = Some(reason);
        stats.support_body = None;
        stats.anchor_milli = None;
        stats.progress_milli = 0;
        stats.rearm_required = true;
        stats.cooldown_ticks = REARM_TICKS;
        stats.releases = stats.releases.saturating_add(1);
        // The dynamic hull retains its actual velocity, forces and contacts.
    }

    /// True consumes this tick's movement action, including a release tick.
    /// Buoyancy has already been applied; the caller performs the single solve.
    pub(super) fn drive_ledge(&mut self) -> bool {
        let command = std::mem::take(&mut self.ledge.command);
        let catch_target = self.ledge.catch_target.take();
        let stats = &mut self.ledge.stats;
        stats.candidates_checked = 0;
        stats.clearance_queries = 0;
        stats.cooldown_ticks = stats.cooldown_ticks.saturating_sub(1);
        if command == LedgeCommand::Idle {
            stats.rearm_required = false;
        }
        if self.ledge.attachment.is_none() {
            stats.phase = if stats.cooldown_ticks > 0 && stats.phase == Phase::Dropping {
                Phase::Dropping
            } else {
                Phase::Free
            };
            if command != LedgeCommand::Catch
                || stats.rearm_required
                || stats.cooldown_ticks > 0
                || self.grounded()
            {
                return false;
            }
            self.ledge.stats.phase = Phase::Reaching;
            self.try_catch(catch_target);
        }
        let Some(mut attachment) = self.ledge.attachment else {
            return false;
        };
        if command == LedgeCommand::Release {
            self.drop_ledge(Reason::Released);
            return true;
        }
        if !attachment.edge.valid(self) {
            self.drop_ledge(Reason::SupportLost);
            return true;
        }
        let world = self.world.as_ref().unwrap();
        let motion = world.motion(DUCK_BODY).unwrap();
        let edge = attachment.edge;
        let anchor = edge.anchor(world).unwrap();
        let support = edge.velocity(world).unwrap();
        let r = self.radius;
        let hang = anchor + Vec2::new(edge.outward * 1.12 * r, -0.9 * r);
        let up = anchor + Vec2::new(edge.outward * 1.12 * r, 1.15 * r);
        let across = anchor + Vec2::new(-edge.outward * 1.5 * r, 1.15 * r);
        let landing = anchor + Vec2::new(-edge.outward * 1.5 * r, 0.98 * r);
        let queries = &mut self.ledge.stats.clearance_queries;
        if (motion.position - anchor).length() > r * REACH_RADII
            || (motion.linear_velocity - support).length() > r * DISRUPTION_SPEED_RADII
            || !clear_path(
                world,
                motion.position,
                motion.position,
                r * 0.99,
                None,
                queries,
            )
        {
            self.drop_ledge(Reason::Disrupted);
            return true;
        }
        if command == LedgeCommand::Climb && attachment.stage == Stage::Hanging {
            if clear_path(world, motion.position, up, r, None, queries)
                && clear_path(world, up, across, r, None, queries)
                && clear_path(world, across, landing, r, Some(edge.body), queries)
            {
                attachment.stage = Stage::Rise;
                attachment.ticks = 0;
                self.ledge.stats.reason = None;
            } else {
                self.ledge.stats.reason = Some(Reason::ClimbBlocked);
            }
        }
        let target = match attachment.stage {
            Stage::Catching | Stage::Hanging => hang,
            Stage::Rise => up,
            Stage::Across => across,
            Stage::Settle => landing,
        };
        let error = (target - motion.position).length();
        if attachment.stage == Stage::Hanging && error > r * MAX_ERROR_RADII {
            self.drop_ledge(Reason::Disrupted);
            return true;
        }
        if !clear_path(
            world,
            motion.position,
            target,
            r,
            (attachment.stage == Stage::Settle).then_some(edge.body),
            queries,
        ) {
            self.drop_ledge(Reason::Obstructed);
            return true;
        }
        attachment.ticks = attachment.ticks.saturating_add(1);
        if attachment.stage == Stage::Catching && attachment.ticks > CAPTURE_TICKS {
            self.drop_ledge(Reason::CaptureFailed);
            return true;
        }
        if matches!(
            attachment.stage,
            Stage::Rise | Stage::Across | Stage::Settle
        ) && attachment.ticks > CLIMB_TICKS
        {
            self.drop_ledge(Reason::ClimbTimeout);
            return true;
        }
        if error < r * 0.1 && (motion.linear_velocity - support).length() < r {
            attachment.stage = match attachment.stage {
                Stage::Catching => Stage::Hanging,
                Stage::Rise => Stage::Across,
                Stage::Across => Stage::Settle,
                stage => stage,
            };
        }
        let desired =
            limited((target - motion.position) * DRIVE_GAIN, r * MAX_SPEED_RADII) + support;
        let delta = desired + Vec2::new(0.0, self.movement.gravity * DT) - motion.linear_velocity;
        self.world.as_mut().unwrap().apply_velocity_delta(
            DUCK_BODY,
            limited(delta, r * MAX_ACCELERATION_RADII * DT),
            true,
        );
        self.ledge.attachment = Some(attachment);
        let stats = &mut self.ledge.stats;
        stats.phase = match attachment.stage {
            Stage::Catching => Phase::Reaching,
            Stage::Hanging => Phase::Hanging,
            _ => Phase::Climbing,
        };
        stats.anchor_milli = Some([
            (anchor.x * 1000.0).round() as i32,
            (anchor.y * 1000.0).round() as i32,
        ]);
        stats.progress_milli = match attachment.stage {
            Stage::Rise => 250,
            Stage::Across => 600,
            Stage::Settle => 900,
            _ => 0,
        };
        true
    }

    fn try_catch(&mut self, target: Option<Edge>) {
        let world = self.world.as_ref().unwrap();
        let motion = world.motion(DUCK_BODY).unwrap();
        let r = self.radius;
        self.ledge.stats.reason = Some(Reason::NoEligibleEdge);
        let candidates = if let Some(target) = target {
            let mut candidates = [None; geometry::MAX_EDGES];
            candidates[0] = Some(target);
            candidates
        } else {
            edges(self)
        };
        for edge in candidates.into_iter().flatten() {
            self.ledge.stats.candidates_checked += 1;
            let anchor = edge.anchor(world).unwrap();
            if !edge.in_reach(self) {
                continue;
            }
            let hang = anchor + Vec2::new(edge.outward * 1.12 * r, -0.9 * r);
            let landing = anchor + Vec2::new(-edge.outward * 1.5 * r, 1.05 * r);
            let queries = &mut self.ledge.stats.clearance_queries;
            if !clear_path(world, motion.position, hang, r, None, queries)
                || !clear_path(world, landing, landing, r, None, queries)
                || !geometry::hand_clear(world, motion.position, anchor, edge.body, r, queries)
            {
                self.ledge.stats.reason = Some(Reason::Obstructed);
                continue;
            }
            // Registration validates the same local anchor and visit generation.
            if let Some((_, handle)) = edge.flap
                && !self.attach_to_flap(handle, edge.local_anchor)
            {
                return;
            }
            self.ledge.attachment = Some(Attachment {
                edge,
                stage: Stage::Catching,
                ticks: 0,
            });
            let stats = &mut self.ledge.stats;
            stats.catches = stats.catches.saturating_add(1);
            stats.support_body = Some(edge.body.entity.value());
            stats.reason = None;
            return;
        }
    }

    pub(super) fn finish_ledge_step(&mut self) {
        let Some(attachment) = self.ledge.attachment else {
            return;
        };
        let world = self.world.as_ref().unwrap();
        self.ledge.stats.anchor_milli = attachment.edge.anchor(world).map(|anchor| {
            [
                (anchor.x * 1000.0).round() as i32,
                (anchor.y * 1000.0).round() as i32,
            ]
        });
        if attachment.stage == Stage::Settle
            && world.surface_contacts(DUCK_COLLIDER).any(|c| {
                c.collider.entity == attachment.edge.body.entity
                    && c.normal.y > 0.7
                    && c.separation <= self.radius * 0.05
            })
        {
            self.ledge.attachment = None;
            self.release_flap_attachment();
            let stats = &mut self.ledge.stats;
            stats.climbs = stats.climbs.saturating_add(1);
            stats.phase = Phase::Free;
            stats.reason = None;
            stats.support_body = None;
            stats.anchor_milli = None;
            stats.progress_milli = 1000;
            stats.rearm_required = true;
            stats.cooldown_ticks = REARM_TICKS;
        }
    }
}

fn limited(vector: Vec2, limit: f32) -> Vec2 {
    let length = vector.length();
    if length > limit {
        vector * (limit / length)
    } else {
        vector
    }
}
