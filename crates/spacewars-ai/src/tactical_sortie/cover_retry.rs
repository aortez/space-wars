//! Temporary memory of observed cover failures, never a landing permission.
use super::*;

pub const COVER_RETRY_PROFILE: &str = "cover_retry_cooldown_v1";
pub const COVER_RETRY_TICKS: u64 = 30 * 60;
const MAX_REJECTIONS: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoverRejection {
    pub site: LandingSiteId,
    pub revision: u64,
    pub rejected_tick: u64,
    pub until_tick: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CoverRetryCooldown {
    pub rejected: Vec<CoverRejection>,
    pub blocked_selections: u64,
    pub first_blocked_tick: Option<u64>,
    pub last_blocked_tick: Option<u64>,
}

impl CoverRetryCooldown {
    fn reject(&mut self, site: PilotLandingSite, tick: u64) {
        self.rejected.retain(|old| {
            old.site != site.id && old.rejected_tick <= tick && tick < old.until_tick
        });
        if self.rejected.len() == MAX_REJECTIONS {
            self.rejected.remove(0);
        }
        self.rejected.push(CoverRejection {
            site: site.id,
            revision: site.revision,
            rejected_tick: tick,
            until_tick: tick.saturating_add(COVER_RETRY_TICKS),
        });
    }
}

impl TacticalSortiePilot {
    pub(crate) fn enable_cover_retry_cooldown(&mut self, enabled: bool) {
        self.telemetry.cover_retry_cooldown = enabled.then(CoverRetryCooldown::default);
    }

    pub(super) fn remember_cover_rejection(&mut self, tick: u64) {
        if self.commit_descent
            && let Some(site) = self.site
            && let Some(memory) = &mut self.telemetry.cover_retry_cooldown
        {
            memory.reject(site, tick);
        }
    }

    pub(super) fn cover_retry_blocked(
        &self,
        o: &TacticalSortieObservationV1,
        site: &PilotLandingSite,
        exposed: bool,
    ) -> bool {
        let Some(memory) = &self.telemetry.cover_retry_cooldown else {
            return false;
        };
        let p = &o.combat.recovery.flight.pilot;
        // Match the native transition out of SeekCover. Becoming sheltered
        // releases this filter; all route, material and solar gates still apply.
        let usable_cover = o.cover.iter().any(|cover| {
            cover.site == site.id
                && cover.grounded
                && (cover.approach
                    || (p.ship.position - site.vehicle_position).dot(site.normal) < 40.0)
        });
        self.commit_descent
            && exposed
            && !usable_cover
            && memory.rejected.iter().any(|old| {
                old.site == site.id
                    && old.revision == site.revision
                    && old.rejected_tick <= p.tick
                    && p.tick < old.until_tick
            })
    }

    pub(super) fn record_cover_exclusions(&mut self, tick: u64, count: usize) {
        if count > 0
            && let Some(memory) = &mut self.telemetry.cover_retry_cooldown
        {
            memory.blocked_selections += 1;
            memory.first_blocked_tick.get_or_insert(tick);
            memory.last_blocked_tick = Some(tick);
        }
    }
}

#[cfg(test)]
mod tests;
