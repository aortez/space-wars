//! Opportunistic remote checks after local jobs have spent their allowance.
use super::query_budget::{QueryBudget, QueryFuel};
use super::*;
use crate::surface_sortie::destination_cover::*;
use pilot::PilotMotion;

const REFRESH_TICKS: u64 = 30;

#[derive(Debug, Clone, Default, Serialize)]
pub struct DestinationCoverTelemetry {
    pub submitted: u64,
    pub cancelled: u64,
    pub checks: u64,
    pub measured: u64,
    pub no_landing: u64,
    pub incomplete: u64,
    pub physics_queries: u64,
    pub deferred: BTreeMap<&'static str, u64>,
    pub max_retained: usize,
}

#[derive(Clone)]
struct DestinationRequest {
    request: DestinationCoverRequest,
    token: RequestToken,
    seen: u64,
    submitted: u64,
    opponent: Option<combat::CombatTarget>,
    result: DestinationCoverObservation,
}

#[derive(Clone, Default)]
pub(super) struct Destinations {
    requests: BTreeMap<usize, DestinationRequest>,
    pub(super) telemetry: DestinationCoverTelemetry,
}
impl Destinations {
    pub(super) fn owns_token(&self, token: RequestToken) -> bool {
        self.requests.values().any(|r| r.token == token)
    }
    pub(super) fn remove(&mut self, player: usize) {
        self.telemetry.cancelled += u64::from(self.requests.remove(&player).is_some());
    }
    pub(super) fn retain_seen(&mut self, tick: u64) {
        let old = self.requests.len();
        self.requests.retain(|_, r| r.seen == tick);
        self.telemetry.cancelled += (old - self.requests.len()) as u64;
    }
    pub(super) fn snapshots(&self, tick: u64) -> Vec<(usize, DestinationCoverObservation)> {
        self.requests
            .iter()
            .map(|(&actor, r)| (actor, r.result.at_tick(tick)))
            .collect()
    }
    pub(super) fn observe(
        &mut self,
        state: &SurfaceSortieState,
        player: usize,
        o: &mut mission::MissionObservationV1,
        request: Option<DestinationCoverRequest>,
        queue: &mut PlanningQueue<u64, ObjectiveSurveyJob>,
        capacity: usize,
    ) {
        let Some(request) = request else {
            self.remove(player);
            o.destination_cover = None;
            return;
        };
        let p = &o.local.combat.recovery.flight.pilot;
        assert_eq!(p.tick, state.world.tick);
        assert_eq!(p.owner, state.pilots[player].owner);
        let candidates: Vec<_> = request.candidates.into_iter().flatten().collect();
        let planets: std::collections::BTreeSet<_> =
            candidates.iter().map(|id| id.planet).collect();
        let invalid = candidates.is_empty()
            || planets.len() > 2
            || candidates.iter().enumerate().any(|(i, id)| {
                id.planet >= state.world.planets.len()
                    || id.bearing >= pilot::LANDING_SITE_COUNT
                    || candidates[..i].contains(id)
            });
        let reason = if invalid {
            Some("invalid request")
        } else if !p.ship_available
            || p.ship_form != ShipForm::Ship
            || !matches!(p.location, PilotLocation::Aboard(_))
            || p.landing.supported_feet > 0
            || p.site_query != LandingSiteQuery::NotRequested
        {
            Some("local task takes priority")
        } else if self.requests.len() == capacity && !self.requests.contains_key(&player) {
            Some("capacity")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.remove(player);
            let mut result = DestinationCoverObservation::pending(request);
            for candidate in &mut result.candidates {
                candidate.status = CoverStatus::Deferred;
                candidate.reason = Some(reason);
            }
            o.destination_cover = Some(result);
            return;
        }
        if self
            .requests
            .get(&player)
            .is_some_and(|r| r.request != request)
        {
            self.remove(player);
        }
        let r = self.requests.entry(player).or_insert_with(|| {
            self.telemetry.submitted += 1;
            DestinationRequest {
                request,
                token: queue.reserve_token(player as u64),
                seen: p.tick,
                submitted: p.tick,
                opponent: o.local.combat.target,
                result: DestinationCoverObservation::pending(request),
            }
        });
        r.seen = p.tick;
        r.opponent = o.local.combat.target;
        let mut result = r.result.at_tick(p.tick);
        if state.world.physics.material_queries_dirty {
            for candidate in &mut result.candidates {
                candidate.status = if candidate.measurement.is_some() {
                    CoverStatus::Stale
                } else {
                    CoverStatus::Deferred
                };
                candidate.reason = Some("material queries unavailable");
            }
        }
        o.destination_cover = Some(result);
        self.telemetry.max_retained = self.telemetry.max_retained.max(self.requests.len());
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance(
        &mut self,
        state: &SurfaceSortieState,
        queries: &mut QueryBudget,
        mut remaining: u32,
        allowance: Work,
        capacity: usize,
        local: &BTreeMap<usize, Request>,
        parked: &BTreeMap<usize, Parked>,
    ) {
        let tick = state.world.tick;
        self.retain_seen(tick);
        for (&player, request) in &mut self.requests {
            // Oldest eligible sample first, with stable shortlist order for ties.
            let Some(index) = request
                .result
                .candidates
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    c.measurement
                        .as_ref()
                        .is_none_or(|m| tick >= m.tick + REFRESH_TICKS)
                })
                .min_by_key(|(i, c)| (c.measurement.as_ref().map(|m| m.tick), *i))
                .map(|(i, _)| i)
            else {
                continue;
            };
            let candidate = &mut request.result.candidates[index];
            let reason = if local.contains_key(&player) || parked.contains_key(&player) {
                Some("local planning takes priority")
            } else if state.world.physics.material_queries_dirty {
                Some("material queries unavailable")
            } else if !queries.available(player, capacity, allowance, remaining) {
                Some("query budget")
            } else {
                None
            };
            if let Some(reason) = reason {
                candidate.status = CoverStatus::Deferred;
                candidate.reason = Some(reason);
                *self.telemetry.deferred.entry(reason).or_default() += 1;
                continue;
            }
            let fuel = QueryFuel::default();
            let measurement = measure(
                state,
                player,
                candidate.id,
                request.opponent,
                request.request.sample_climb,
                &fuel,
            );
            let finding = measurement.finding;
            candidate.measurement = Some(measurement);
            candidate.status = match finding {
                CoverFinding::Incomplete => CoverStatus::Incomplete,
                CoverFinding::NoLanding => CoverStatus::NoLanding,
                CoverFinding::Measured => CoverStatus::Measured,
            };
            candidate.reason = None;
            queries.record(request.token, tick - request.submitted + 1, &fuel);
            remaining -= fuel.used();
            self.telemetry.checks += 1;
            self.telemetry.physics_queries += u64::from(fuel.used());
            self.telemetry.measured += u64::from(finding == CoverFinding::Measured);
            self.telemetry.no_landing += u64::from(finding == CoverFinding::NoLanding);
            self.telemetry.incomplete += u64::from(finding == CoverFinding::Incomplete);
        }
    }
}

// A candidate is atomic at this layer: any denied query discards partial site
// and cover evidence. Old evidence is never silently completed with new rays.
pub(super) fn measure(
    state: &SurfaceSortieState,
    player: usize,
    id: pilot::LandingSiteId,
    enemy: Option<combat::CombatTarget>,
    sample_climb: bool,
    fuel: &QueryFuel,
) -> CoverMeasurement {
    let site = state.vehicle_landing_site_with_queries(player, id, false, || fuel.charge());
    let armed = enemy.is_some_and(|e| {
        let p = &state.pilots[e.owner.index()];
        p.body.is_none() && state.world.ships[p.vehicle.0].form == ShipForm::Ship
    });
    let cover = site
        .as_ref()
        .filter(|_| !fuel.exhausted() && armed)
        .map(|site| state.landing_cover_with_queries(site, enemy, || fuel.charge()));
    let climb_clear = site
        .as_ref()
        .filter(|_| sample_climb && !fuel.exhausted())
        .map(|site| {
            [7.0, 30.0, 60.0].into_iter().all(|height| {
                fuel.charge()
                    && state.world.physics.surface_hull_fits_at(
                        state.pilots[player].vehicle.0,
                        site.vehicle_position + site.normal * height,
                        rotation_for_direction(site.normal),
                    )
            })
        });
    let finding = if fuel.exhausted() {
        CoverFinding::Incomplete
    } else if site.is_none() {
        CoverFinding::NoLanding
    } else {
        CoverFinding::Measured
    };
    let frame = motion::SurfaceFrame::read(&state.world.physics, id.planet);
    let opponent = enemy.map(|enemy| {
        let pilot = &state.pilots[enemy.owner.index()];
        CoverOpponent {
            owner: enemy.owner,
            motion: enemy.motion,
            armed_ship: pilot.body.is_none()
                && state.world.ships[pilot.vehicle.0].form == ShipForm::Ship,
        }
    });
    CoverMeasurement {
        tick: state.world.tick,
        revision: state
            .world
            .terrain
            .planets
            .get(&id.planet)
            .map_or(0, |p| p.field.revision()),
        planet: PilotMotion {
            position: frame.position,
            velocity: frame.linear_velocity,
            angle: frame.angle,
            spin: frame.angular_velocity,
        },
        ship_form: state.world.ships[state.pilots[player].vehicle.0].form,
        opponent,
        queries: fuel.used(),
        finding,
        site: (!fuel.exhausted()).then_some(site).flatten(),
        cover: (!fuel.exhausted()).then_some(cover).flatten(),
        climb_clear: (!fuel.exhausted()).then_some(climb_clear).flatten(),
    }
}
