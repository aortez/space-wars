//! Contact charges: each new impact a capture saw, priced after the step
//! against the surfaces the host keeps. Pricing reads the step's pre-solve
//! records and leaves the physics alone, so Rapier's restitution still sets
//! the bounce. The host keeps the regions and charges them, as it does for
//! weapons. See `docs/design/material-contact-charges.md`.

use super::*;
use engine_material::shared_impact::{self, BudgetError};
use engine_material::{
    ContactGeometry, Impact, MIN_EFFECTIVE_MASS, MaterialError, Outcome, Region, Section, impact,
    patch_width_from_radii,
};

/// The material behind one contact source, as the host keeps it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactSurface {
    /// The region, with the capacity it has left.
    pub region: Region,
    pub section: Section,
    /// The surface's radius of curvature where it meets others, `wu`. `None`
    /// is flat. The patch width comes from the two radii, so an impact
    /// between two flat surfaces is not priced.
    pub radius: Option<f32>,
}

/// The host's side of contact charges: the surface behind each source, and
/// its policy on who may be charged.
pub trait ContactSurfaces {
    /// The surface the host keeps behind `source`, if it keeps one.
    fn surface(&self, source: ContactSource) -> Option<ContactSurface>;
    /// Charges the region behind `source` with the work an impact consumed
    /// from it.
    fn absorb(&mut self, source: ContactSource, work: f32);
    /// Whether the host's policy shields `source` for now, as it might shield
    /// a ship just after it spawns. A shielded side is priced as any other,
    /// and its work is reported rather than charged.
    fn protected(&self, _source: ContactSource) -> bool {
        false
    }
}

/// Why an impact was not priced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unpriced {
    /// A side's region was retired after the step.
    Retired,
    /// The host keeps no surface behind a side.
    Undeclared,
    /// The new impacts that shared its bodies in its substep could not share
    /// one budget, such as when they have more points than a budget takes.
    Budget(BudgetError),
    /// The material law refused the impact, such as two flat surfaces.
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

/// One approaching point of a new impact, in its substep's budget.
struct Row {
    impact: usize,
    bodies: [usize; 2],
    point: Vec2,
    normal: Vec2,
}

/// A new impact's share of its substep's energy, and the fastest approach
/// among its points, which carries that energy into the law.
#[derive(Debug, Clone, Copy, Default)]
struct Share {
    energy: f64,
    speed: f64,
}

impl PhysicsWorld {
    /// Prices each new impact of the last step against the host's surfaces,
    /// and has the host absorb what each consumed. Call it after every step:
    /// the next step drops this one's records, and a clone or snapshot
    /// carries none. A second call after the same step prices nothing.
    ///
    /// The new impacts of one substep that share a dynamic body share its
    /// motion: their approaching points are one coupled budget, and each
    /// impact takes its own points' share. The impact law then prices that
    /// energy across the patch the two surfaces' radii give.
    pub fn charge_contact_impacts(
        &mut self,
        surfaces: &mut impl ContactSurfaces,
    ) -> Result<Vec<ContactCharge>, LoadingError> {
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
            let shares = state.shares(batch, &impacts);
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
                point: record.point,
                normal: record.normal,
            };
            if approach(&row, &motions) > f64::from(self.config.min_approach_speed) {
                rows.push(row);
            }
        }
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
        shares
    }
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
    let outcome = match price(a, b, share) {
        Ok(outcome) => outcome,
        Err(error) => return unpriced(Some(share.energy), Unpriced::Material(error)),
    };
    let protected = event.sources.map(|s| surfaces.protected(s));
    let sides = event
        .sources
        .into_iter()
        .zip(outcome.consumed)
        .zip(protected);
    for ((source, work), shielded) in sides {
        if !shielded && work > 0.0 {
            surfaces.absorb(source, work);
        }
    }
    ContactCharge::Charged {
        episode: event.id,
        sources: event.sources,
        closing_speed: event.closing_speed,
        outcome,
        protected,
    }
}

/// The impact law on a share of energy, across the patch the radii give.
fn price(a: ContactSurface, b: ContactSurface, share: Share) -> Result<Outcome, MaterialError> {
    let width = match (a.radius, b.radius) {
        (Some(radius), other) => patch_width_from_radii(radius, other)?,
        (None, Some(radius)) => patch_width_from_radii(radius, None)?,
        (None, None) => return Err(MaterialError::InvalidGeometry),
    };
    let contact = ContactGeometry::new(width, a.section, b.section);
    let effective_mass = (2.0 * share.energy / (share.speed * share.speed)) as f32;
    if share.energy == 0.0 || effective_mass < MIN_EFFECTIVE_MASS {
        // The impact's points took none of the motion, or too little for the
        // law to price, such as when another impact on the same body pushed
        // them clear.
        contact.validate()?;
        return Ok(Outcome::default());
    }
    let load = Impact {
        closing_speed: share.speed as f32,
        effective_mass,
    };
    impact(&a.region, &b.region, contact, load)
}

#[cfg(test)]
mod tests;
