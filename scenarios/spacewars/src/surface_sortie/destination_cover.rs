//! Bounded remote landing/cover evidence. This does not authorize a landing or
//! establish a route to a flag and back. Samples describe their measurement tick.
use super::*;
use combat::{CombatTarget, LandingCover};
use pilot::{LandingSiteId, PilotLandingSite, PilotMotion};

pub const MAX_COVER_CANDIDATES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DestinationCoverRequest {
    pub generation: u64,
    pub candidates: [Option<LandingSiteId>; MAX_COVER_CANDIDATES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverStatus {
    Pending,
    Deferred,
    Incomplete,
    NoLanding,
    Measured,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverFinding {
    Incomplete,
    NoLanding,
    Measured,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CoverOpponent {
    pub owner: PlayerId,
    pub motion: PilotMotion,
    pub armed_ship: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoverMeasurement {
    pub tick: u64,
    pub revision: u64,
    pub planet: PilotMotion,
    pub ship_form: ShipForm,
    pub opponent: Option<CoverOpponent>,
    pub queries: u32,
    pub finding: CoverFinding,
    pub site: Option<PilotLandingSite>,
    pub cover: Option<LandingCover>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoverCandidate {
    pub id: LandingSiteId,
    pub status: CoverStatus,
    pub reason: Option<&'static str>,
    /// Historical evidence remains visible when stale or deferred. Only a
    /// sample from the current physics tick claims current material/cover.
    pub measurement: Option<CoverMeasurement>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DestinationCoverObservation {
    pub generation: u64,
    pub candidates: Vec<CoverCandidate>,
}
impl DestinationCoverObservation {
    pub fn pending(request: DestinationCoverRequest) -> Self {
        Self {
            generation: request.generation,
            candidates: request
                .candidates
                .into_iter()
                .flatten()
                .map(|id| CoverCandidate {
                    id,
                    status: CoverStatus::Pending,
                    reason: None,
                    measurement: None,
                })
                .collect(),
        }
    }
    pub(super) fn at_tick(&self, tick: u64) -> Self {
        let mut result = self.clone();
        for candidate in &mut result.candidates {
            if candidate
                .measurement
                .as_ref()
                .is_some_and(|m| m.tick != tick)
                && !matches!(candidate.status, CoverStatus::Deferred)
            {
                candidate.status = CoverStatus::Stale;
                candidate.reason = Some("physics advanced; remeasurement required");
            }
        }
        result
    }
}

impl SurfaceSortieState {
    /// Shared with local landing observations. Charge before each first-solid
    /// ray; failed fuel must be handled as incomplete by the caller.
    pub(super) fn landing_cover_with_queries(
        &self,
        site: &PilotLandingSite,
        enemy: Option<CombatTarget>,
        charge: impl Fn() -> bool,
    ) -> LandingCover {
        let occluded = |height| {
            let Some(enemy) = enemy else { return true };
            let pilot = &self.pilots[enemy.owner.index()];
            if pilot.body.is_some() || self.world.ships[pilot.vehicle.0].form != ShipForm::Ship {
                return true;
            }
            let delta = site.vehicle_position + site.normal * height - enemy.motion.position;
            charge()
                && self
                    .world
                    .physics
                    .cast_laser(
                        pilot.vehicle.0,
                        enemy.motion.position,
                        delta.normalized(),
                        delta.length(),
                    )
                    .is_some_and(|hit| {
                        hit.target == Some(MechanicalEntity::Body(BodyId::Planet(site.id.planet)))
                    })
        };
        LandingCover {
            site: site.id,
            grounded: occluded(7.0),
            approach: occluded(30.0),
            departure: occluded(60.0),
        }
    }
}
