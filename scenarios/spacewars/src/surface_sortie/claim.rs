//! Planet ownership without infrastructure. Flags are visual, surface-local markers.

use super::*;

const FLAG_STAGE_TIME: Duration = Duration::from_secs(3);
const FLAG_INTERACTION_RANGE: f32 = 3.0;
const CLAIM_MAX_SPEED: f32 = 1.0;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanetClaimPhase {
    #[default]
    Idle,
    Lowering,
    Raising,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanetClaimStatus {
    Aboard,
    Elsewhere,
    NeedSupport,
    NeedBalance,
    NeedSettle,
    ApproachFlag,
    Ready,
    Lowering,
    Raising,
    Contested,
    Secured,
}

impl PlanetClaimStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Aboard => "land and disembark to claim",
            Self::Elsewhere => "stand on this planet to claim",
            Self::NeedSupport => "stand on the planet's surface",
            Self::NeedBalance => "recover your balance",
            Self::NeedSettle | Self::Ready => "stand still to claim",
            Self::ApproachFlag => "walk to the flag",
            Self::Lowering => "lowering enemy flag; stay nearby",
            Self::Raising => "raising your flag; stand still",
            Self::Contested => "contested; flag progress paused",
            Self::Secured => "your planet",
        }
    }
}

/// The contact location and normal in the completed terrain body's frame.
/// Never reproject onto an assumed radius: future noncircular ground can use
/// the same attachment contract without adding a rigid body for the flag.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FlagAnchor {
    position: Vec2,
    normal: Vec2,
    footing: Option<engine_terrain::CellCoord>,
}

impl FlagAnchor {
    fn world_position(self, frame: motion::SurfaceFrame) -> Vec2 {
        frame.position + self.position.rotate_radians(frame.angle)
    }
}

#[derive(Debug, Clone, Copy)]
struct PlanetFlag {
    player: PlayerId,
    anchor: FlagAnchor,
}

#[derive(Debug, Clone, Copy)]
struct ClaimProgress {
    player: PlayerId,
    phase: PlanetClaimPhase,
    elapsed: Duration,
}

#[derive(Debug, Clone, Copy)]
struct Claimant {
    player: PlayerId,
    planet: Option<usize>,
    status: PlanetClaimStatus,
    anchor: Option<FlagAnchor>,
}

#[derive(Clone)]
pub(super) struct SurfacePlanetClaim {
    pub planet: usize,
    flag: Option<PlanetFlag>,
    progress: Option<ClaimProgress>,
    statuses: [PlanetClaimStatus; SPACEWARS_PLAYER_COUNT],
    captures: u64,
    neutralizations: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PlanetFlagObservation {
    pub player: PlayerId,
    pub position: Vec2,
    pub normal: Vec2,
    pub raised_fraction: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanetClaimObservation {
    pub planet: usize,
    pub owner: Option<PlayerId>,
    pub claimant: Option<PlayerId>,
    pub phase: PlanetClaimPhase,
    pub progress: f32,
    pub stage_required_seconds: f32,
    pub flag_interaction_range: f32,
    pub status: PlanetClaimStatus,
    pub flag: Option<PlanetFlagObservation>,
    pub captures: u64,
    pub neutralizations: u64,
}

impl SurfacePlanetClaim {
    pub(super) fn new(planet: usize) -> Self {
        Self {
            planet,
            flag: None,
            progress: None,
            statuses: [PlanetClaimStatus::Aboard; SPACEWARS_PLAYER_COUNT],
            captures: 0,
            neutralizations: 0,
        }
    }

    pub(super) fn observation(
        &self,
        owner: Option<usize>,
        frame: motion::SurfaceFrame,
        player: usize,
    ) -> PlanetClaimObservation {
        let phase = self.progress.map_or(PlanetClaimPhase::Idle, |p| p.phase);
        let progress = self.progress.map_or(0.0, |p| {
            p.elapsed.as_secs_f32() / FLAG_STAGE_TIME.as_secs_f32()
        });
        PlanetClaimObservation {
            planet: self.planet,
            owner: owner.and_then(PlayerId::from_index),
            claimant: self.progress.map(|p| p.player),
            phase,
            progress,
            stage_required_seconds: FLAG_STAGE_TIME.as_secs_f32(),
            flag_interaction_range: FLAG_INTERACTION_RANGE,
            status: self.statuses[player],
            flag: self.flag.map(|flag| PlanetFlagObservation {
                player: flag.player,
                position: flag.anchor.world_position(frame),
                normal: flag.anchor.normal.rotate_radians(frame.angle),
                raised_fraction: match phase {
                    PlanetClaimPhase::Idle => 1.0,
                    PlanetClaimPhase::Lowering => 1.0 - progress,
                    PlanetClaimPhase::Raising => progress,
                },
            }),
            captures: self.captures,
            neutralizations: self.neutralizations,
        }
    }

    fn cancel_progress(&mut self) {
        if self
            .progress
            .take()
            .is_some_and(|p| p.phase == PlanetClaimPhase::Raising)
        {
            self.flag = None;
        }
        // An interrupted lowering restores the still-owned flag to full height.
        // A completed lowering has already removed both the flag and owner.
    }

    fn update(
        &mut self,
        owner: &mut Option<usize>,
        frame: motion::SurfaceFrame,
        claimants: &[Claimant],
        dt: Duration,
    ) {
        let mut eligible = [None; SPACEWARS_PLAYER_COUNT];
        let mut count = 0;
        for candidate in claimants {
            let status = if candidate.planet.is_some_and(|planet| planet != self.planet) {
                PlanetClaimStatus::Elsewhere
            } else if candidate.status == PlanetClaimStatus::Ready
                && self.flag.is_some_and(|flag| {
                    candidate.anchor.is_some_and(|anchor| {
                        anchor
                            .world_position(frame)
                            .distance_to(flag.anchor.world_position(frame))
                            > FLAG_INTERACTION_RANGE
                    })
                })
            {
                PlanetClaimStatus::ApproachFlag
            } else {
                candidate.status
            };
            self.statuses[candidate.player.index()] = if *owner == Some(candidate.player.index()) {
                PlanetClaimStatus::Secured
            } else {
                status
            };
            if status == PlanetClaimStatus::Ready {
                eligible[count] = Some(*candidate);
                count += 1;
            }
        }
        // Resolve the complete set first; seat iteration cannot win a capture race.
        if count > 1 {
            if self
                .progress
                .is_some_and(|p| !eligible.iter().flatten().any(|c| c.player == p.player))
            {
                self.cancel_progress();
            }
            for candidate in eligible.into_iter().flatten() {
                self.statuses[candidate.player.index()] = PlanetClaimStatus::Contested;
            }
            return;
        }
        let Some(candidate) = eligible[0] else {
            self.cancel_progress();
            return;
        };
        if *owner == Some(candidate.player.index()) {
            self.cancel_progress();
            return;
        }
        if self.progress.is_none_or(|p| p.player != candidate.player) {
            self.cancel_progress();
            let phase = if owner.is_some() {
                PlanetClaimPhase::Lowering
            } else {
                self.flag = Some(PlanetFlag {
                    player: candidate.player,
                    anchor: candidate
                        .anchor
                        .expect("eligible claimant has real support"),
                });
                PlanetClaimPhase::Raising
            };
            self.progress = Some(ClaimProgress {
                player: candidate.player,
                phase,
                elapsed: Duration::ZERO,
            });
        }
        let progress = self.progress.as_mut().expect("active claim");
        self.statuses[candidate.player.index()] = match progress.phase {
            PlanetClaimPhase::Lowering => PlanetClaimStatus::Lowering,
            PlanetClaimPhase::Raising => PlanetClaimStatus::Raising,
            PlanetClaimPhase::Idle => unreachable!("idle is not an active claim"),
        };
        progress.elapsed = progress.elapsed.saturating_add(dt).min(FLAG_STAGE_TIME);
        if progress.elapsed < FLAG_STAGE_TIME {
            return;
        }
        match progress.phase {
            PlanetClaimPhase::Lowering => {
                *owner = None;
                self.flag = Some(PlanetFlag {
                    player: candidate.player,
                    anchor: candidate
                        .anchor
                        .expect("eligible claimant has real support"),
                });
                self.neutralizations += 1;
                // The old flag is gone. Reserve the new flag at height zero;
                // raising receives no elapsed time from the lowering tick.
                self.progress = Some(ClaimProgress {
                    player: candidate.player,
                    phase: PlanetClaimPhase::Raising,
                    elapsed: Duration::ZERO,
                });
                self.statuses[candidate.player.index()] = PlanetClaimStatus::Raising;
            }
            PlanetClaimPhase::Raising => {
                *owner = Some(candidate.player.index());
                self.captures += 1;
                self.statuses[candidate.player.index()] = PlanetClaimStatus::Secured;
                self.progress = None;
            }
            PlanetClaimPhase::Idle => unreachable!("idle is not an active claim"),
        }
        // Ownership changes and both seat views must agree in this same observation.
        for other in claimants.iter().filter(|c| c.player != candidate.player) {
            if self.statuses[other.player.index()] == PlanetClaimStatus::Secured {
                self.statuses[other.player.index()] = other.status;
            }
        }
    }
}

impl SurfaceSortieState {
    pub(super) fn invalidate_flag_footings(&mut self) {
        for claim in &mut self.claims {
            let Some(terrain) = self.world.terrain.planets.get(&claim.planet) else {
                continue;
            };
            let lost = claim.flag.is_some_and(|flag| {
                !flag.anchor.footing.is_some_and(|cell| {
                    terrain
                        .field
                        .cell(cell)
                        .is_some_and(|cell| cell.material != engine_terrain::MaterialId::VOID)
                })
            });
            if lost {
                // Flags are currently the only owned planetary object. Destroying
                // their material footing neutralizes, never transfers ownership.
                claim.neutralizations +=
                    u64::from(self.world.planets[claim.planet].owner_id.take().is_some());
                claim.flag = None;
                claim.progress = None;
                claim.statuses = [PlanetClaimStatus::NeedSupport; SPACEWARS_PLAYER_COUNT];
            }
        }
    }

    fn claim_candidate(&self, player: usize) -> Claimant {
        let mut candidate = Claimant {
            player: self.pilots[player].owner,
            planet: None,
            status: PlanetClaimStatus::Aboard,
            anchor: None,
        };
        let Some(snapshot) = self.spaceling_snapshot(player) else {
            return candidate;
        };
        candidate.status = PlanetClaimStatus::NeedSupport;
        let Some((support, planet)) = snapshot.support.and_then(|support| {
            physics::planet_surface_support_index(support.collider)
                .filter(|&index| index < self.world.planets.len())
                .map(|planet| (support, planet))
        }) else {
            return candidate;
        };
        candidate.planet = Some(planet);
        if snapshot.balance != SpacelingBalance::Balanced {
            candidate.status = PlanetClaimStatus::NeedBalance;
            return candidate;
        }
        let offset = support.position - snapshot.motion.position;
        let relative = snapshot.motion.linear_velocity
            + Vec2::new(-offset.y, offset.x) * snapshot.motion.angular_velocity
            - support.velocity;
        if relative.length() > CLAIM_MAX_SPEED {
            candidate.status = PlanetClaimStatus::NeedSettle;
            return candidate;
        }
        let frame = motion::SurfaceFrame::read(&self.world.physics, planet);
        let position = (support.position - frame.position).rotate_radians(-frame.angle);
        let normal = support.normal.rotate_radians(-frame.angle);
        let footing = self.world.terrain.planets.get(&planet).and_then(|terrain| {
            terrain
                .field
                .local_to_cell(position - normal * 0.08)
                .filter(|cell| {
                    terrain
                        .field
                        .cell(*cell)
                        .is_some_and(|cell| cell.material != engine_terrain::MaterialId::VOID)
                })
        });
        if self.world.terrain.planets.contains_key(&planet) && footing.is_none() {
            return candidate;
        }
        candidate.anchor = Some(FlagAnchor {
            position,
            normal,
            footing,
        });
        candidate.status = PlanetClaimStatus::Ready;
        candidate
    }

    pub(super) fn update_planet_claims(&mut self, dt: Duration) {
        if self.claims.is_empty() {
            return;
        }
        // Sample each pilot once from the completed step, not once per planet.
        let candidates: [_; SPACEWARS_PLAYER_COUNT] = std::array::from_fn(|player| {
            if player < self.player_count() {
                self.claim_candidate(player)
            } else {
                Claimant {
                    player: PlayerId::from_index(player).expect("bounded player seat"),
                    planet: None,
                    status: PlanetClaimStatus::Aboard,
                    anchor: None,
                }
            }
        });
        let count = self.player_count();
        for claim in &mut self.claims {
            let frame = motion::SurfaceFrame::read(&self.world.physics, claim.planet);
            claim.update(
                &mut self.world.planets[claim.planet].owner_id,
                frame,
                &candidates[..count],
                dt,
            );
        }
    }

    pub(super) fn claim_observation(
        &self,
        planet: usize,
        player: usize,
    ) -> Option<PlanetClaimObservation> {
        let claim = self.claims.get(planet)?;
        Some(claim.observation(
            self.world.planets[planet].owner_id,
            motion::SurfaceFrame::read(&self.world.physics, planet),
            player,
        ))
    }

    pub(super) fn enable_planet_claims(&mut self) {
        self.claims = (0..self.world.planets.len())
            .map(SurfacePlanetClaim::new)
            .collect();
        for pilot in &mut self.pilots {
            pilot.travel_enabled = true;
        }
    }
}

#[cfg(test)]
mod tests;
