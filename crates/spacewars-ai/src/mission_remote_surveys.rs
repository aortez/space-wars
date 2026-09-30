//! Bounded historical survey geometry for opt-in comparisons. This memory never
//! supplies a live controller or grants landing/boarding permission.
use super::{BrainReset, MissionObservationV1, transfer_forecast};
use crate::mission_evaluation::{MAX_EVIDENCE_AGE, MAX_PLANETS};
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        SpacelingId, VehicleId,
        destination_cover::{CoverStatus, DestinationCoverObservation, MAX_COVER_CANDIDATES},
        pilot::{LANDING_SITE_COUNT, PilotPlanetObservation},
    },
};
use serde::Serialize;

const MAX_RETAINED_PLANETS: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize)]
struct NeutralIdentity {
    planet: usize,
    revision: u64,
    radius: f32,
    stage_required_seconds: f32,
    flag_interaction_range: f32,
    captures: u64,
    neutralizations: u64,
}
impl NeutralIdentity {
    fn read(p: &PilotPlanetObservation) -> Option<Self> {
        let c = p.claim.as_ref()?;
        (c.planet == p.index
            && c.owner.is_none()
            && c.flag.is_none()
            && p.radius.is_finite()
            && p.radius > 0.0
            && c.stage_required_seconds.is_finite()
            && c.stage_required_seconds > 0.0
            && c.flag_interaction_range.is_finite()
            && c.flag_interaction_range > 0.0)
            .then_some(Self {
                planet: p.index,
                revision: p.revision,
                radius: p.radius,
                stage_required_seconds: c.stage_required_seconds,
                flag_interaction_range: c.flag_interaction_range,
                captures: c.captures,
                neutralizations: c.neutralizations,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Binding {
    actor: PlayerId,
    episode_seed: u64,
    vehicle: VehicleId,
    spaceling: SpacelingId,
}

#[derive(Debug, Clone, PartialEq)]
struct Frame {
    binding: Binding,
    tick: u64,
    planets: Vec<PilotPlanetObservation>,
}
impl Frame {
    fn read(context: BrainReset, o: &MissionObservationV1) -> Option<Self> {
        let p = &o.local.combat.recovery.flight.pilot;
        (transfer_forecast::source_schema_matches(o, context.actor)
            && p.ship_available
            && p.ship_form == ShipForm::Ship
            && !o
                .match_context
                .as_ref()
                .is_some_and(|m| m.finished || !m.pilots_alive[context.actor.index()])
            && o.planets.len() <= MAX_PLANETS
            && o.planets.iter().enumerate().all(|(i, planet)| {
                !o.planets[..i]
                    .iter()
                    .any(|other| other.index == planet.index)
            }))
        .then(|| Self {
            binding: Binding {
                actor: context.actor,
                episode_seed: context.episode_seed,
                vehicle: p.vehicle,
                spaceling: p.spaceling,
            },
            tick: p.tick,
            planets: o.planets.clone(),
        })
    }
    fn planet(&self, index: usize) -> Option<&PilotPlanetObservation> {
        self.planets.iter().find(|p| p.index == index)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SurveyGroup {
    identity: NeutralIdentity,
    observed_tick: u64,
    survey: DestinationCoverObservation,
}

/// An immutable snapshot at one actual observation. Private fields prevent
/// callers from rebasing old evidence or mixing actor/episode identities.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteSurveySnapshot {
    #[serde(flatten)]
    binding: Binding,
    tick: u64,
    groups: Vec<SurveyGroup>,
    #[serde(skip)]
    frame: Frame,
}
impl RemoteSurveySnapshot {
    pub(super) fn matches(&self, context: BrainReset, o: &MissionObservationV1) -> bool {
        Frame::read(context, o).as_ref() == Some(&self.frame)
    }
    pub(super) fn cover(&self, destination: usize) -> Option<&DestinationCoverObservation> {
        self.groups
            .iter()
            .find(|g| g.identity.planet == destination)
            .map(|g| &g.survey)
    }
}

/// Two planet groups, at most four slots per original survey generation.
/// Hosts must reset this memory whenever they reset the associated pilot.
pub struct RemoteSurveyMemory {
    context: BrainReset,
    previous: Option<Frame>,
    groups: Vec<SurveyGroup>,
    generation: Option<u64>,
}
impl RemoteSurveyMemory {
    pub fn new(context: BrainReset) -> Self {
        Self {
            context,
            previous: None,
            groups: Vec::new(),
            generation: None,
        }
    }
    pub fn reset(&mut self, context: BrainReset) {
        *self = Self::new(context);
    }

    /// Host collection guard before `observe`: no missed frame, actor/vehicle
    /// change or neutral material identity change since the preceding frame.
    /// This neither admits evidence nor authorizes a landing.
    pub fn continues_neutral_material(&self, o: &MissionObservationV1, planet: usize) -> bool {
        let Some(frame) = Frame::read(self.context, o) else {
            return false;
        };
        self.previous.as_ref().is_some_and(|previous| {
            previous.binding == frame.binding
                && previous.tick.checked_add(1) == Some(frame.tick)
                && previous
                    .planet(planet)
                    .and_then(NeutralIdentity::read)
                    .is_some_and(|identity| {
                        frame
                            .planet(planet)
                            .and_then(NeutralIdentity::read)
                            .as_ref()
                            == Some(&identity)
                    })
        })
    }

    /// Observe once before controls. The active planner dispatches after that
    /// observation, so a new sample can belong to this or the previous tick.
    /// Older first-seen samples are never imported, even after a reset or gap.
    pub fn observe(
        &mut self,
        o: &MissionObservationV1,
        cover: Option<&DestinationCoverObservation>,
    ) {
        let Some(frame) = Frame::read(self.context, o) else {
            self.reset(self.context);
            return;
        };
        if let Some(previous) = &self.previous {
            if frame.tick == previous.tick && frame.binding == previous.binding {
                // Same-tick calls cannot overwrite history. Conflicting world
                // observations invalidate it without admitting replacement data.
                if frame != *previous {
                    self.groups.clear();
                    self.previous = Some(frame);
                }
                return;
            }
            if frame.binding != previous.binding || previous.tick.checked_add(1) != Some(frame.tick)
            {
                self.reset(self.context);
            }
        }
        self.groups.retain_mut(|g| {
            if frame
                .planet(g.identity.planet)
                .and_then(NeutralIdentity::read)
                .as_ref()
                != Some(&g.identity)
            {
                return false;
            }
            for c in &mut g.survey.candidates {
                if c.measurement
                    .as_ref()
                    .is_some_and(|m| m.tick > frame.tick || frame.tick - m.tick > MAX_EVIDENCE_AGE)
                {
                    c.measurement = None;
                    c.status = CoverStatus::Stale;
                    c.reason = Some("retained measurement expired");
                }
            }
            g.survey.candidates.iter().any(|c| c.measurement.is_some())
        });
        if let Some(cover) = cover.filter(|c| {
            c.generation <= frame.tick
                && c.candidates.len() <= MAX_COVER_CANDIDATES
                && c.candidates.iter().enumerate().all(|(i, candidate)| {
                    candidate.id.bearing < LANDING_SITE_COUNT
                        && !c.candidates[..i].iter().any(|old| old.id == candidate.id)
                })
        }) {
            self.admit(&frame, cover);
            self.generation = Some(
                self.generation
                    .map_or(cover.generation, |old| old.max(cover.generation)),
            );
        }
        self.previous = Some(frame);
    }

    fn admit(&mut self, frame: &Frame, cover: &DestinationCoverObservation) {
        for planet in &frame.planets {
            let Some(identity) = NeutralIdentity::read(planet) else {
                continue;
            };
            let mut incoming: Vec<_> = cover
                .candidates
                .iter()
                .filter(|c| c.id.planet == planet.index)
                .cloned()
                .collect();
            for c in &mut incoming {
                let admitted = c.measurement.as_ref().is_some_and(|m| {
                    let measured_frame = if m.tick == frame.tick {
                        Some(frame)
                    } else {
                        self.previous.as_ref().filter(|p| p.tick == m.tick)
                    };
                    m.tick >= cover.generation
                        && m.ship_form == ShipForm::Ship
                        && measured_frame
                            .and_then(|f| f.planet(planet.index))
                            .is_some_and(|p| {
                                m.planet == p.motion
                                    && m.revision == p.revision
                                    && NeutralIdentity::read(p).as_ref() == Some(&identity)
                            })
                });
                if !admitted {
                    c.measurement = None;
                    c.status = CoverStatus::Pending;
                    c.reason = Some("sample not observed at measurement tick");
                }
            }
            if !incoming.iter().any(|c| c.measurement.is_some()) {
                continue;
            }
            let existing = self
                .groups
                .iter()
                .position(|g| g.identity.planet == planet.index);
            if self.generation.is_some_and(|g| cover.generation < g)
                && existing.is_none_or(|i| self.groups[i].survey.generation != cover.generation)
            {
                // A resumed retained generation can refresh its own slots.
                // Otherwise a regressed generation cannot replace a newer one;
                // witnessed conflicting evidence also prevents keeping its old
                // positive geometry as though nothing had been measured.
                if let Some(index) = existing {
                    self.groups.remove(index);
                }
                continue;
            }
            if let Some(index) = existing {
                let group = &mut self.groups[index];
                if group.survey.generation == cover.generation {
                    // A generation describes a fixed set of candidate IDs.
                    if incoming.len() != group.survey.candidates.len()
                        || incoming
                            .iter()
                            .any(|c| !group.survey.candidates.iter().any(|old| old.id == c.id))
                    {
                        continue;
                    }
                    for candidate in incoming.into_iter().filter(|c| c.measurement.is_some()) {
                        let old = group
                            .survey
                            .candidates
                            .iter_mut()
                            .find(|c| c.id == candidate.id)
                            .unwrap();
                        if old
                            .measurement
                            .as_ref()
                            .is_none_or(|m| m.tick < candidate.measurement.as_ref().unwrap().tick)
                        {
                            // Negative/incomplete findings replace positives too.
                            *old = candidate;
                            group.observed_tick = frame.tick;
                        }
                    }
                    continue;
                }
                self.groups.remove(index);
            }
            // Pending-only generations do not erase history. Once one actual
            // measurement arrives, replace the entire older generation.
            self.groups.push(SurveyGroup {
                identity,
                observed_tick: frame.tick,
                survey: DestinationCoverObservation {
                    generation: cover.generation,
                    candidates: incoming,
                },
            });
        }
        self.groups.sort_by_key(|g| {
            (
                g.survey
                    .candidates
                    .iter()
                    .filter_map(|c| c.measurement.as_ref().map(|m| m.tick))
                    .max()
                    .unwrap(),
                g.identity.planet,
            )
        });
        if self.groups.len() > MAX_RETAINED_PLANETS {
            self.groups
                .drain(..self.groups.len() - MAX_RETAINED_PLANETS);
        }
        self.groups.sort_by_key(|g| g.identity.planet);
    }

    pub fn snapshot(&self, o: &MissionObservationV1) -> Option<RemoteSurveySnapshot> {
        let frame = Frame::read(self.context, o)?;
        (self.previous.as_ref() == Some(&frame)).then(|| RemoteSurveySnapshot {
            binding: frame.binding.clone(),
            tick: frame.tick,
            groups: self.groups.clone(),
            frame,
        })
    }
}

#[cfg(test)]
#[path = "mission_remote_surveys_tests.rs"]
mod tests;
