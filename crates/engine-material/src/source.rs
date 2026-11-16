//! Weapon damage sources (#329). A source is one shot or charge whose finite
//! energy comes in up to three separately attributed parts: a projectile's
//! motion, an explosive payload released once, and a beam's pulses. Each part
//! delivers energy to recipients through the material laws, so recipients
//! respond as they do to ordinary impacts, and its ledger records where the
//! energy went. A part never delivers more than its budget, so a crowd or an
//! extra collider divides a share rather than copying it; the one exception is
//! a hard hit, which is the ordinary impact the host measures. Like [`impact`],
//! these calls do not mutate recipients; the host absorbs each outcome's
//! `consumed` work into its recipient before that recipient's next event. See
//! `docs/design/material-sources.md`.

use crate::{
    ContactBody, ContactGeometry, Face, Impact, MaterialError, Outcome, Region, deposit,
    effective_mass, impact,
};
use serde::Serialize;
use std::collections::BTreeSet;

/// Host-assigned identity of one shot or charge. Hosts never reuse one.
pub type SourceId = u64;

/// The separately attributed parts of a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Part {
    /// The projectile's motion.
    Kinetic,
    /// Explosive energy, released once.
    Payload,
    /// A beam's pulses.
    Beam,
}

/// A projectile: its own material region, its whole mass and its launch
/// speed relative to recipients at rest.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Projectile {
    pub region: Region,
    /// `mu`. The region never weighs more than the projectile.
    pub mass: f32,
    /// `wu / s`.
    pub speed: f32,
}

impl Projectile {
    /// Kinetic energy at the launch speed, `mu wu^2 / s^2`: the budget its
    /// passes share.
    pub fn energy(&self) -> f64 {
        0.5 * f64::from(self.mass) * f64::from(self.speed).powi(2)
    }
}

/// A beam's pulses: the energy each carries along its path, and how many the
/// source fires.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Pulses {
    pub energy: f32,
    pub count: u32,
}

/// What a source starts with.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SourceSpec {
    pub id: SourceId,
    /// The firing actor as the host identifies it. The source keeps it after
    /// that actor dies.
    pub owner: Option<u64>,
    pub projectile: Option<Projectile>,
    /// Stored explosive energy, `mu wu^2 / s^2`.
    pub payload: Option<f32>,
    pub beam: Option<Pulses>,
}

/// One recipient's share of a payload release or of a beam pulse.
#[derive(Debug, Clone, Copy)]
pub struct Share<'a> {
    /// The recipient's identity to the host, such as its region or tissue. A
    /// release or a pulse names each key once.
    pub key: u64,
    pub region: &'a Region,
    pub face: Face,
    /// Fraction of the payload or of the pulse that reaches this recipient.
    pub coupling: f32,
}

/// Where one part's energy went, `mu wu^2 / s^2`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct PartLedger {
    /// Energy the part started with: the projectile's kinetic energy at
    /// launch, the payload, or every pulse's energy together.
    pub budget: f64,
    /// Energy recipients received: each share of a release or a pulse, the
    /// work passes left in their recipients, and a hard hit's available
    /// energy. Everything but the hard hit stays within the budget.
    pub delivered: f64,
    /// Irreversible work charged to recipients.
    pub absorbed: f64,
    /// Work that exhausted regions could not take, for the host to resolve:
    /// recipients', and on a hard hit the projectile's own as well.
    pub excess: f64,
}

impl PartLedger {
    fn add(&mut self, delivered: f64, outcome: &Outcome, slot: usize) {
        self.delivered += delivered;
        self.absorbed += f64::from(outcome.consumed[slot]);
        self.excess += f64::from(outcome.ledger.excess);
    }
}

/// Where each part's energy went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct SourceLedger {
    pub kinetic: PartLedger,
    pub payload: PartLedger,
    pub beam: PartLedger,
}

/// Why a call was refused. A refused call changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    /// The spec, a load or a face is invalid.
    Material(MaterialError),
    /// The source has no such part, or the part is spent: the projectile has
    /// made its hard hit, the payload has been released, or every pulse has
    /// fired.
    Spent(Part),
    /// The source was retired.
    Retired,
    /// The projectile already struck this recipient, or a release or pulse
    /// names it twice.
    Duplicate(u64),
    /// A coupling outside 0 to 1, or couplings that sum past 1.
    Coupling,
}

impl From<MaterialError> for SourceError {
    fn from(error: MaterialError) -> Self {
        Self::Material(error)
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Material(error) => error.fmt(f),
            Self::Spent(part) => write!(f, "the source's {part:?} part is absent or spent"),
            Self::Retired => f.write_str("the source was retired"),
            Self::Duplicate(key) => write!(f, "recipient {key} was already reached"),
            Self::Coupling => f.write_str("couplings must lie in 0 to 1 and not sum past 1"),
        }
    }
}

impl std::error::Error for SourceError {}

/// Couplings may sum past 1 by this much for rounding; the shares still never
/// take more than the energy.
const COUPLING_TOLERANCE: f64 = 1.0e-6;

/// A projectile's region may outweigh it by this fraction, for a host that
/// sizes the region from the projectile's mass and rounds.
const MASS_TOLERANCE: f64 = 1.0e-6;

fn coupling_ok(coupling: f32) -> bool {
    coupling.is_finite() && (0.0..=1.0).contains(&coupling)
}

/// `energy`, or the largest `f32` within `left` if it would take more, so that
/// rounding to `f32` never takes more than is left.
fn at_most(energy: f32, left: f64) -> f32 {
    if f64::from(energy) <= left {
        return energy;
    }
    let rounded = left as f32;
    if f64::from(rounded) > left {
        rounded.next_down()
    } else {
        rounded
    }
}

/// Split `energy` over `shares`, each taking its coupling's fraction of the
/// whole, and evaluate each share's [`deposit`]. Each key may appear once, and
/// the couplings may not sum past 1.
fn spread(energy: f32, shares: &[Share]) -> Result<Vec<Outcome>, SourceError> {
    let mut keys = BTreeSet::new();
    let mut total = 0.0;
    for share in shares {
        if !coupling_ok(share.coupling) {
            return Err(SourceError::Coupling);
        }
        if !keys.insert(share.key) {
            return Err(SourceError::Duplicate(share.key));
        }
        total += f64::from(share.coupling);
    }
    if total > 1.0 + COUPLING_TOLERANCE {
        return Err(SourceError::Coupling);
    }
    // What is left, in `f64`, so that no share rounded to `f32` takes more
    // than the exact remainder.
    let mut left = f64::from(energy);
    shares
        .iter()
        .map(|share| {
            let share_energy = at_most(energy * share.coupling, left);
            left -= f64::from(share_energy);
            deposit(share.region, share.face, share_energy).map_err(SourceError::from)
        })
        .collect()
}

/// One shot or charge and the energy it has left.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Source {
    id: SourceId,
    owner: Option<u64>,
    projectile: Option<Projectile>,
    hit: bool,
    payload: Option<f32>,
    released: bool,
    beam: Option<Pulses>,
    fired: u32,
    struck: BTreeSet<u64>,
    ledger: SourceLedger,
    retired: bool,
}

impl Source {
    pub fn new(spec: SourceSpec) -> Result<Self, SourceError> {
        let invalid = SourceError::Material(MaterialError::InvalidLoad);
        if let Some(projectile) = spec.projectile {
            let valid =
                projectile.mass > 0.0 && projectile.speed >= 0.0 && projectile.energy().is_finite();
            if !valid {
                return Err(invalid);
            }
            // The region is part of the projectile.
            let mass = f64::from(projectile.mass) * (1.0 + MASS_TOLERANCE);
            if f64::from(projectile.region.mass()) > mass {
                return Err(SourceError::Material(MaterialError::InvalidRegion));
            }
        }
        if spec
            .payload
            .is_some_and(|energy| !(energy.is_finite() && energy >= 0.0))
        {
            return Err(invalid);
        }
        if spec
            .beam
            .is_some_and(|beam| !(beam.energy.is_finite() && beam.energy >= 0.0 && beam.count > 0))
        {
            return Err(invalid);
        }
        let ledger = SourceLedger {
            kinetic: PartLedger {
                budget: spec
                    .projectile
                    .map_or(0.0, |projectile| projectile.energy()),
                ..PartLedger::default()
            },
            payload: PartLedger {
                budget: spec.payload.map_or(0.0, f64::from),
                ..PartLedger::default()
            },
            beam: PartLedger {
                budget: spec
                    .beam
                    .map_or(0.0, |beam| f64::from(beam.energy) * f64::from(beam.count)),
                ..PartLedger::default()
            },
        };
        Ok(Self {
            id: spec.id,
            owner: spec.owner,
            projectile: spec.projectile,
            hit: false,
            payload: spec.payload,
            released: false,
            beam: spec.beam,
            fired: 0,
            struck: BTreeSet::new(),
            ledger,
            retired: false,
        })
    }

    pub fn id(&self) -> SourceId {
        self.id
    }

    pub fn owner(&self) -> Option<u64> {
        self.owner
    }

    /// The projectile with any damage its hard hit did to its region.
    pub fn projectile(&self) -> Option<&Projectile> {
        self.projectile.as_ref()
    }

    pub fn ledger(&self) -> SourceLedger {
        self.ledger
    }

    pub fn pulses_left(&self) -> u32 {
        self.beam.map_or(0, |beam| beam.count - self.fired)
    }

    /// Whether nothing is left to deliver: the source was retired, or every
    /// part it has is spent.
    pub fn spent(&self) -> bool {
        self.retired
            || ((self.projectile.is_none() || self.hit)
                && (self.payload.is_none() || self.released)
                && self.pulses_left() == 0)
    }

    /// End the source. Later deliveries are refused, and its owner and ledger
    /// stay readable.
    pub fn retire(&mut self) {
        self.retired = true;
    }

    fn flying(&self, key: u64) -> Result<Projectile, SourceError> {
        if self.retired {
            return Err(SourceError::Retired);
        }
        let projectile = self
            .projectile
            .filter(|_| !self.hit)
            .ok_or(SourceError::Spent(Part::Kinetic))?;
        if self.struck.contains(&key) {
            return Err(SourceError::Duplicate(key));
        }
        Ok(projectile)
    }

    /// The projectile sweeps through a nonblocking recipient at rest, such as
    /// a grass blade, at the actual `closing_speed` of the sweep. This is a
    /// weak-coupling, ballistic approximation: the recipient's face takes a
    /// [`deposit`] of what a perfectly inelastic collision with its own mass
    /// would make available, and the projectile flies on unchanged, so its
    /// hard hit is the one it would have made without the recipient. The
    /// recipient keeps its irreversible and damped work, and one the
    /// projectile cuts through keeps its remaining capacity. Its recoverable
    /// energy stays with the projectile, and so does the work it could not
    /// take, as the outcome's already resolved failure motion. The work passes
    /// keep comes out of the kinetic budget, and a pass gets no more energy
    /// than the budget has left. The recipient is slot 0 of the outcome. The
    /// projectile reaches each `key` once, by a pass or by its hard hit.
    pub fn pass(
        &mut self,
        key: u64,
        recipient: &Region,
        face: Face,
        closing_speed: f32,
    ) -> Result<Outcome, SourceError> {
        let projectile = self.flying(key)?;
        let load = Impact {
            closing_speed,
            effective_mass: effective_mass(
                ContactBody::translating(projectile.mass),
                ContactBody::translating(recipient.mass()),
            )?,
        };
        load.validate()?;
        let left = (self.ledger.kinetic.budget - self.ledger.kinetic.delivered).max(0.0);
        let mut outcome = deposit(recipient, face, at_most(load.energy(), left))?;
        let ledger = &mut outcome.ledger;
        ledger.resolve_excess(ledger.excess)?;
        let kept = (f64::from(ledger.irreversible[0]) + f64::from(ledger.dissipated[0])).min(left);
        self.struck.insert(key);
        self.ledger.kinetic.add(kept, &outcome, 0);
        Ok(outcome)
    }

    /// The projectile hits a hard recipient, with the impact the host measured
    /// from both bodies' actual motion, so the recipient responds as to any
    /// impact. The kinetic part then ends: later passes and hits are refused,
    /// whatever the physics does next. The projectile is slot `a` of the
    /// outcome and the recipient slot `b`, and the host resolves any excess,
    /// the projectile's included. A recipient already passed is refused.
    pub fn hit(
        &mut self,
        key: u64,
        recipient: &Region,
        contact: ContactGeometry,
        load: Impact,
    ) -> Result<Outcome, SourceError> {
        let mut projectile = self.flying(key)?;
        let outcome = impact(&projectile.region, recipient, contact, load)?;
        projectile.region.absorb(outcome.consumed[0])?;
        self.projectile = Some(projectile);
        self.hit = true;
        self.struck.insert(key);
        self.ledger
            .kinetic
            .add(f64::from(outcome.ledger.available), &outcome, 1);
        Ok(outcome)
    }

    /// Release the payload over `shares`, which a source does once. Each share
    /// receives its coupling's fraction of the payload, and what the couplings
    /// leave reaches no recipient. The outcomes are [`deposit`]s on each
    /// share's face, in share order, with the recipient in slot 0.
    pub fn release(&mut self, shares: &[Share]) -> Result<Vec<Outcome>, SourceError> {
        if self.retired {
            return Err(SourceError::Retired);
        }
        let energy = self
            .payload
            .filter(|_| !self.released)
            .ok_or(SourceError::Spent(Part::Payload))?;
        let outcomes = spread(energy, shares)?;
        self.released = true;
        record(&mut self.ledger.payload, &outcomes);
        Ok(outcomes)
    }

    /// Fire one beam pulse over `shares`, as a release spreads a payload: each
    /// share receives its coupling's fraction of the pulse, and what the
    /// couplings leave goes on past the path. The outcomes are [`deposit`]s on
    /// each share's face, in share order, with the recipient in slot 0.
    pub fn pulse(&mut self, shares: &[Share]) -> Result<Vec<Outcome>, SourceError> {
        if self.retired {
            return Err(SourceError::Retired);
        }
        let beam = self
            .beam
            .filter(|beam| self.fired < beam.count)
            .ok_or(SourceError::Spent(Part::Beam))?;
        let outcomes = spread(beam.energy, shares)?;
        self.fired += 1;
        record(&mut self.ledger.beam, &outcomes);
        Ok(outcomes)
    }
}

/// Each share received the energy its deposit had available.
fn record(ledger: &mut PartLedger, outcomes: &[Outcome]) {
    for outcome in outcomes {
        ledger.add(f64::from(outcome.ledger.available), outcome, 0);
    }
}
