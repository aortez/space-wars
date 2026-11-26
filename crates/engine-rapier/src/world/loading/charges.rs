//! Contact charges: each new impact a capture saw, and each support's load,
//! priced after the step against the surfaces the host keeps. Impacts are
//! priced from the step's pre-solve records and supports from the solver's
//! impulses, and neither changes the physics, so Rapier's restitution still
//! sets the bounce. The host keeps the regions and charges them, as it does
//! for weapons. See `docs/design/material-contact-charges.md`.

use super::*;
use engine_material::shared_impact::{self, BudgetError};
use engine_material::{
    ContactGeometry, Impact, MIN_EFFECTIVE_MASS, MaterialError, Outcome, Region, Section,
    deformation_depth, impact, patch_width_from_radii,
};

/// Normals closer than this, as a cosine, push along one patch.
const PATCH_NORMAL_COSINE: f64 = 0.999;
/// Lengths below this, in the world's length units, are none: closer patch
/// ends are one point, and narrower gaps between pieces close.
const COINCIDENT: f64 = 1.0e-3;

/// The material behind one contact source, as the host keeps it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactSurface {
    /// The region, with the capacity it has left.
    pub region: Region,
    /// Represented depth into the screen, `wu`.
    pub thickness: f32,
    pub depth: Depth,
    /// The surface's radius of curvature where it meets others, `wu`. `None`
    /// is flat. Where two surfaces meet is as wide as their points span, or
    /// as the two radii give if that is wider, so a single point between two
    /// flat surfaces is not priced.
    pub radius: Option<f32>,
}

/// How deep the material behind a surface deforms along the contact normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Depth {
    /// A fixed depth, `wu`, such as a plate's.
    Fixed(f32),
    /// As deep as the patch is wide, up to how far the material reaches
    /// behind the contact, as `deformation_depth` gives. `None` is a
    /// half-space.
    Patch(Option<f32>),
}

/// The host's side of contact charges: the surface behind each source, and
/// its policy on who may be charged.
pub trait ContactSurfaces {
    /// The surface the host keeps behind `source`, if it keeps one.
    fn surface(&self, source: ContactSource) -> Option<ContactSurface>;
    /// Charges the region behind `source` with the work an impact or a
    /// support consumed from it.
    fn absorb(&mut self, source: ContactSource, work: f32);
    /// Whether the host's policy shields `source` for now, as it might shield
    /// a ship just after it spawns. A shielded side is priced as any other,
    /// and its work is reported rather than charged.
    fn protected(&self, _source: ContactSource) -> bool {
        false
    }
}

/// Why an impact or a support was not priced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unpriced {
    /// A side's region was retired after the step.
    Retired,
    /// The host keeps no surface behind a side.
    Undeclared,
    /// The new impacts that shared its bodies in its substep could not share
    /// one budget, such as when they have more points than a budget takes.
    /// Supports have no budget.
    Budget(BudgetError),
    /// The material law refused the load, such as at a single point between
    /// two flat surfaces.
    Material(MaterialError),
}

/// What pricing one new impact did. Each impact is reported once.
#[derive(Debug, Clone, PartialEq)]
pub enum ContactCharge {
    /// An impact priced against both surfaces. The outcome's slots follow
    /// `sources`, and each side the host doesn't protect has absorbed its
    /// consumed work.
    Charged {
        episode: EpisodeId,
        sources: [ContactSource; 2],
        /// The fastest approach that started the episode, `wu / s`.
        closing_speed: f32,
        /// The patch the law priced across, and the section behind each side.
        geometry: ContactGeometry,
        /// What the law priced: the fastest approach among the impact's points
        /// in its substep, and the effective mass its share of the energy
        /// gives at that speed.
        load: Impact,
        outcome: Outcome,
        protected: [bool; 2],
    },
    /// An impact that was not priced.
    Unpriced {
        episode: EpisodeId,
        sources: [ContactSource; 2],
        closing_speed: f32,
        /// The impact's share of its substep's energy, if its budget was
        /// solved.
        energy: Option<f32>,
        reason: Unpriced,
    },
}

/// What pricing one support's load did. Each steady support that carried
/// load in the step is reported once.
#[derive(Debug, Clone, PartialEq)]
pub enum SupportCharge {
    /// A support priced against both surfaces. Each outcome is one side's, in
    /// its slot 0, and they follow `sources`. Each side the host doesn't
    /// protect has absorbed its consumed work.
    Charged {
        episode: EpisodeId,
        sources: [ContactSource; 2],
        /// The normal force the support carried over the step,
        /// `mu wu / s^2`.
        force: f32,
        /// How wide what its loaded pieces cover is, `wu`.
        width: f32,
        /// How long the load acted, seconds: the whole step.
        duration: f32,
        outcomes: [Outcome; 2],
        protected: [bool; 2],
    },
    /// A support that was not priced, because the host keeps no surface
    /// behind a side or the law refused the load.
    Unpriced {
        episode: EpisodeId,
        sources: [ContactSource; 2],
        force: f32,
        reason: Unpriced,
    },
}

/// One approaching point of a new impact, in its substep's budget.
#[derive(Clone, Copy)]
struct Row {
    impact: usize,
    bodies: [usize; 2],
    piece: Piece,
    point: Vec2,
    normal: Vec2,
}

/// A new impact's share of its substep's energy, the fastest approach among
/// its points, which carries that energy into the law, and where its points
/// meet.
#[derive(Debug, Clone, Default)]
struct Share {
    energy: f64,
    speed: f64,
    coverage: Coverage,
}

/// The colliders, or their compound children, that meet at a point. Each
/// piece of a contact is convex, so its points bound one interval of it.
type Piece = ([ColliderId; 2], [Option<u32>; 2]);

/// A point for [`patches`] to group: whose it is, its piece, where it is and
/// the normal it pushes along.
#[derive(Clone, Copy)]
struct PatchPoint {
    group: usize,
    piece: Piece,
    point: Vec2,
    normal: Vec2,
}

/// What a group's points cover: for each patch, the intervals along it that
/// its pieces meet on, apart and in order. An interval no longer than the
/// coincident length is a point.
type Coverage = Vec<Vec<[f64; 2]>>;

impl PhysicsWorld {
    /// Prices each new impact of the last step against the host's surfaces,
    /// and has the host absorb what each consumed. Call it after every step:
    /// the next step drops this one's records, and a clone or snapshot
    /// carries none. A second call after the same step prices nothing.
    ///
    /// The new impacts of one substep that share a dynamic body share its
    /// motion: their approaching points are one coupled budget, and each
    /// impact takes its own points' share. The impact law then prices that
    /// energy across what the impact's points cover.
    pub fn charge_contact_impacts(
        &mut self,
        surfaces: &mut impl ContactSurfaces,
    ) -> Result<Vec<ContactCharge>, LoadingError> {
        let coincident = COINCIDENT * f64::from(self.length_unit());
        let state = self.loading.as_mut().ok_or(LoadingError::Disabled)?;
        if !state.speculative_arrivals {
            return Err(LoadingError::ArrivalsDisabled);
        }
        if std::mem::replace(&mut state.charged, true) {
            return Ok(Vec::new());
        }
        let state = &*state;
        let mut charges = Vec::new();
        for batch in &state.batches {
            let impacts: Vec<_> = batch
                .events
                .iter()
                .filter(|e| e.transition == EpisodeTransition::Impact)
                .collect();
            let shares = state.shares(batch, &impacts, coincident);
            for (event, share) in impacts.into_iter().zip(shares) {
                charges.push(charge(state, surfaces, event, share));
            }
        }
        Ok(charges)
    }
}

impl LoadingState {
    /// Each new impact's share of its substep's energy, or why its budget
    /// failed. The approaching points of the new impacts that share a dynamic
    /// body form one budget, so a body that strikes two surfaces at once
    /// spends its motion once.
    fn shares(
        &self,
        batch: &ContactBatch,
        impacts: &[&EpisodeEvent],
        coincident: f64,
    ) -> Vec<Result<Share, BudgetError>> {
        let index: BTreeMap<_, _> = impacts.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        let mut bodies = BTreeMap::new();
        let mut motions = Vec::new();
        let mut rows = Vec::new();
        for record in &batch.contacts {
            let Some(&impact) = record.episode.and_then(|id| index.get(&id)) else {
                continue;
            };
            if !self.acting(record, batch.duration) {
                continue;
            }
            let row = Row {
                impact,
                bodies: record.bodies.map(|state| {
                    *bodies.entry(state.body).or_insert_with(|| {
                        motions.push((motion(record.point, &state), state.kind));
                        motions.len() - 1
                    })
                }),
                piece: (record.colliders, record.subshapes),
                point: record.point,
                normal: record.normal,
            };
            if approach(&row, &motions) > f64::from(self.config.min_approach_speed) {
                rows.push(row);
            }
        }
        let points: Vec<_> = rows
            .iter()
            .map(|row| PatchPoint {
                group: row.impact,
                piece: row.piece,
                point: row.point,
                normal: row.normal,
            })
            .collect();
        let (kept, coverages) = patches(&points, impacts.len(), coincident);
        let rows: Vec<_> = kept.into_iter().map(|i| rows[i]).collect();
        // Impacts that share a dynamic body join one budget. Fixed and
        // kinematic bodies give no way, so they couple nothing.
        let mut roots: Vec<_> = (0..impacts.len()).collect();
        let mut owners = vec![None; motions.len()];
        for row in &rows {
            for body in row.bodies {
                if motions[body].1 != BodyKind::Dynamic {
                    continue;
                }
                match owners[body] {
                    Some(owner) => {
                        let [a, b] = [owner, row.impact].map(|i| root(&mut roots, i));
                        roots[a.max(b)] = a.min(b);
                    }
                    None => owners[body] = Some(row.impact),
                }
            }
        }
        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for row in &rows {
            groups
                .entry(root(&mut roots, row.impact))
                .or_default()
                .push(row);
        }
        let mut shares = vec![Ok(Share::default()); impacts.len()];
        for (group, members) in groups {
            match budget(&members, &motions) {
                Ok(found) => {
                    for (impact, share) in found {
                        shares[impact] = Ok(share);
                    }
                }
                Err(error) => {
                    for (impact, share) in shares.iter_mut().enumerate() {
                        if root(&mut roots, impact) == group {
                            *share = Err(error);
                        }
                    }
                }
            }
        }
        for (share, coverage) in shares.iter_mut().zip(coverages) {
            if let Ok(share) = share {
                share.coverage = coverage;
            }
        }
        shares
    }
}

/// Each group's patches: its points that push along one normal. For rigid
/// bodies, the points between a patch's two ends add no way to push, so only
/// the ends are kept, and ends closer than `coincident` are one point. Each
/// piece's points cover one interval of their patch. Intervals closer than
/// `coincident` join, and the gaps between the rest cover nothing. Returns
/// the kept points' indices and what each group's points cover.
fn patches(points: &[PatchPoint], groups: usize, coincident: f64) -> (Vec<usize>, Vec<Coverage>) {
    struct Patch {
        group: usize,
        normal: Vec2,
        low: (f64, usize),
        high: (f64, usize),
        pieces: Vec<(Piece, [f64; 2])>,
    }
    let along = |normal: Vec2, point: Vec2| {
        f64::from(point.y) * f64::from(normal.x) - f64::from(point.x) * f64::from(normal.y)
    };
    let mut patches: Vec<Patch> = Vec::new();
    for (index, point) in points.iter().enumerate() {
        let same = |patch: &Patch| {
            patch.group == point.group
                && f64::from(patch.normal.x) * f64::from(point.normal.x)
                    + f64::from(patch.normal.y) * f64::from(point.normal.y)
                    >= PATCH_NORMAL_COSINE
        };
        let patch = match patches.iter().position(same) {
            Some(found) => &mut patches[found],
            None => {
                let at = along(point.normal, point.point);
                patches.push(Patch {
                    group: point.group,
                    normal: point.normal,
                    low: (at, index),
                    high: (at, index),
                    pieces: Vec::new(),
                });
                patches.last_mut().unwrap()
            }
        };
        let at = along(patch.normal, point.point);
        if at < patch.low.0 {
            patch.low = (at, index);
        }
        if at > patch.high.0 {
            patch.high = (at, index);
        }
        match patch
            .pieces
            .iter_mut()
            .find(|(piece, _)| *piece == point.piece)
        {
            Some((_, interval)) => {
                interval[0] = interval[0].min(at);
                interval[1] = interval[1].max(at);
            }
            None => patch.pieces.push((point.piece, [at, at])),
        }
    }
    let mut kept = Vec::new();
    let mut coverages = vec![Coverage::new(); groups];
    for patch in patches {
        kept.push(patch.low.1);
        if patch.high.0 - patch.low.0 > coincident {
            kept.push(patch.high.1);
        }
        let mut intervals: Vec<_> = patch.pieces.into_iter().map(|(_, i)| i).collect();
        intervals.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut covered: Vec<[f64; 2]> = Vec::new();
        for [low, high] in intervals {
            match covered.last_mut() {
                Some(last) if low <= last[1] + coincident => last[1] = last[1].max(high),
                _ => covered.push([low, high]),
            }
        }
        for interval in &mut covered {
            if interval[1] - interval[0] <= coincident {
                *interval = [0.5 * (interval[0] + interval[1]); 2];
            }
        }
        coverages[patch.group].push(covered);
    }
    (kept, coverages)
}

fn root(roots: &mut [usize], mut i: usize) -> usize {
    while roots[i] != i {
        roots[i] = roots[roots[i]];
        i = roots[i];
    }
    i
}

/// A body's motion as a record saw it: the point's velocity, less the spin's
/// share there.
fn motion(point: Vec2, state: &ContactBodyState) -> shared_impact::BodyMotion {
    let spin = f64::from(state.angular_velocity);
    let offset = point - state.center_of_mass;
    shared_impact::BodyMotion {
        center: state.center_of_mass,
        linear_velocity: Vec2::new(
            (f64::from(state.point_velocity.x) + spin * f64::from(offset.y)) as f32,
            (f64::from(state.point_velocity.y) - spin * f64::from(offset.x)) as f32,
        ),
        angular_velocity: state.angular_velocity,
        inverse_mass: state.inverse_mass,
        inverse_inertia: state.inverse_inertia,
    }
}

/// A row's approach along its normal, from body 0 toward body 1, as the
/// budget computes it from the bodies' motion.
fn approach(row: &Row, motions: &[(shared_impact::BodyMotion, BodyKind)]) -> f64 {
    let velocity = |body: usize| {
        let motion = &motions[body].0;
        let spin = f64::from(motion.angular_velocity);
        let x = f64::from(row.point.x) - f64::from(motion.center.x);
        let y = f64::from(row.point.y) - f64::from(motion.center.y);
        [
            f64::from(motion.linear_velocity.x) - spin * y,
            f64::from(motion.linear_velocity.y) + spin * x,
        ]
    };
    let [a, b] = row.bodies.map(velocity);
    f64::from(row.normal.x) * (a[0] - b[0]) + f64::from(row.normal.y) * (a[1] - b[1])
}

/// Solves one budget over a group's rows, and gives each impact in it its
/// points' share of the energy.
fn budget(
    rows: &[&Row],
    motions: &[(shared_impact::BodyMotion, BodyKind)],
) -> Result<BTreeMap<usize, Share>, BudgetError> {
    let mut members = BTreeMap::new();
    let mut ordered = Vec::new();
    let mut contacts = Vec::new();
    for (id, row) in rows.iter().enumerate() {
        contacts.push(shared_impact::Contact {
            id: id as u64,
            bodies: row.bodies.map(|body| {
                *members.entry(body).or_insert_with(|| {
                    ordered.push(motions[body].0);
                    ordered.len() - 1
                })
            }),
            point: row.point,
            normal: row.normal,
            stiffness: 1.0,
        });
    }
    let budget = shared_impact::allocate(&ordered, &contacts)?;
    let mut shares = BTreeMap::new();
    for contact in &budget.contacts {
        let share: &mut Share = shares.entry(rows[contact.id as usize].impact).or_default();
        share.energy += contact.energy;
        share.speed = share.speed.max(contact.closing_speed);
    }
    Ok(shares)
}

/// Prices one new impact with its share, and charges the host.
fn charge(
    state: &LoadingState,
    surfaces: &mut impl ContactSurfaces,
    event: &EpisodeEvent,
    share: Result<Share, BudgetError>,
) -> ContactCharge {
    let unpriced = |energy: Option<f64>, reason| ContactCharge::Unpriced {
        episode: event.id,
        sources: event.sources,
        closing_speed: event.closing_speed,
        energy: energy.map(|e| e as f32),
        reason,
    };
    let share = match share {
        Ok(share) => share,
        Err(error) => return unpriced(None, Unpriced::Budget(error)),
    };
    if !event.sources.iter().all(|s| state.live(s.region)) {
        return unpriced(Some(share.energy), Unpriced::Retired);
    }
    let [Some(a), Some(b)] = event.sources.map(|s| surfaces.surface(s)) else {
        return unpriced(Some(share.energy), Unpriced::Undeclared);
    };
    let (geometry, load, outcome) = match price(a, b, &share) {
        Ok(priced) => priced,
        Err(error) => return unpriced(Some(share.energy), Unpriced::Material(error)),
    };
    let protected = absorb(surfaces, event.sources, outcome.consumed);
    ContactCharge::Charged {
        episode: event.id,
        sources: event.sources,
        closing_speed: event.closing_speed,
        geometry,
        load,
        outcome,
        protected,
    }
}

/// Has the host absorb the work each side consumed, except on the sides its
/// policy shields, and returns which those are.
fn absorb(
    surfaces: &mut impl ContactSurfaces,
    sources: [ContactSource; 2],
    consumed: [f32; 2],
) -> [bool; 2] {
    let protected = sources.map(|s| surfaces.protected(s));
    for ((source, work), shielded) in sources.into_iter().zip(consumed).zip(protected) {
        if !shielded && work > 0.0 {
            surfaces.absorb(source, work);
        }
    }
    protected
}

/// How wide what a group's points cover is, between two surfaces. Each
/// interval is widened about its middle to the patch the surfaces' radii give,
/// if it is narrower, and each patch is as wide as the union of its intervals.
/// A point between two flat surfaces is as wide as nothing, so points alone
/// there are not priced.
fn width(
    a: &ContactSurface,
    b: &ContactSurface,
    coverage: &Coverage,
) -> Result<f32, MaterialError> {
    let curved = match (a.radius, b.radius) {
        (Some(radius), other) => patch_width_from_radii(radius, other)?,
        (None, Some(radius)) => patch_width_from_radii(radius, None)?,
        (None, None) => 0.0,
    };
    let curved = f64::from(curved);
    let mut width = 0.0;
    for patch in coverage {
        let mut widened: Vec<_> = patch
            .iter()
            .map(|&[low, high]| {
                let pad = 0.5 * (curved - (high - low)).max(0.0);
                [low - pad, high + pad]
            })
            .collect();
        widened.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut end = f64::NEG_INFINITY;
        for [low, high] in widened {
            width += (high - low.max(end)).max(0.0);
            end = end.max(high);
        }
    }
    // Points that cover nothing, such as an impact whose points all took none
    // of the motion, are as wide as the radii give.
    let width = width.max(curved) as f32;
    if width > 0.0 {
        Ok(width)
    } else {
        Err(MaterialError::InvalidGeometry)
    }
}

/// The section behind a surface, under a patch `width` wide.
fn section(surface: &ContactSurface, width: f32) -> Result<Section, MaterialError> {
    Ok(Section::new(
        surface.thickness,
        match surface.depth {
            Depth::Fixed(depth) => depth,
            Depth::Patch(extent) => deformation_depth(width, extent)?,
        },
    ))
}

/// The impact law on a share of energy, across what the impact's points
/// cover. Returns the geometry and load it priced with, and the outcome.
fn price(
    a: ContactSurface,
    b: ContactSurface,
    share: &Share,
) -> Result<(ContactGeometry, Impact, Outcome), MaterialError> {
    let width = width(&a, &b, &share.coverage)?;
    let contact = ContactGeometry::new(width, section(&a, width)?, section(&b, width)?);
    // No energy is no effective mass, even at no speed.
    let effective_mass = if share.energy == 0.0 {
        0.0
    } else {
        (2.0 * share.energy / (share.speed * share.speed)) as f32
    };
    let load = Impact {
        closing_speed: share.speed as f32,
        effective_mass,
    };
    if share.energy == 0.0 || effective_mass < MIN_EFFECTIVE_MASS {
        // The impact's points took none of the motion, or too little for the
        // law to price, such as when another impact on the same body pushed
        // them clear.
        contact.validate()?;
        return Ok((contact, load, Outcome::default()));
    }
    Ok((contact, load, impact(&a.region, &b.region, contact, load)?))
}

mod supports;

#[cfg(test)]
mod tests;
