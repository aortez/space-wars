//! Support charges: the load each steady support carried in the last step,
//! priced as a sustained load. The force is the solver's own: the normal
//! impulse the step's solve applied at each contact, over the step. Joints and
//! other contacts reach a support through that impulse, so a chassis pressing
//! on a wheel through its suspension loads the wheel's support.

use super::*;
use engine_material::{SustainedLoad, sustained};

/// Points that move against each other no faster than this, in the world's
/// length units a second, hold a support steady.
const STEADY_SPEED: f64 = 0.1;

/// The load one support's points carried in the step.
#[derive(Default)]
struct Carried {
    /// The normal impulse the solve applied, summed over the points.
    impulse: f64,
    /// The touching points of each piece the solve loaded, from source 0
    /// toward source 1.
    points: Vec<PatchPoint>,
    /// Whether the solve stopped a point that was apart before it, which so
    /// landed within the step.
    landed: bool,
}

impl PhysicsWorld {
    /// Prices the load each steady support carried in the last step against
    /// the host's surfaces, as a sustained load on each side, and has the host
    /// absorb what each consumed. A load within a side's strength consumes
    /// nothing however long it rests. Call it after each step's
    /// [`Self::charge_contact_impacts`]; a second call after the same step
    /// prices nothing.
    ///
    /// A support is an open episode whose pair the step's solve acted on and
    /// loaded, and whose points held steady: each the solve acted on moved
    /// against the other surface no faster than a tenth of the length unit a
    /// second, along the normal or across it. Other steps are not priced as
    /// supports:
    ///
    /// - A step that starts the episode with an impact is the impact's.
    /// - A step in which a point moves faster along the normal is a hit's or a
    ///   bounce's. Capture folds a hit into the open episode, such as a hull
    ///   rocking onto its other corner, and nothing prices it. Points
    ///   separating that fast carry an impulse only when something drives them
    ///   back within the step, as when a deep overlap is pushed apart on one
    ///   point.
    /// - A step in which a point slides faster is a grind's. Its load moves
    ///   across both surfaces rather than resting on one patch.
    /// - A step in which the solve stops a point that was apart, such as a
    ///   corner that gravity lands within the step, is a landing's.
    /// - In a step that CCD splits, the last substep's impulse can carry a hit
    ///   delivered in a small part of the step.
    /// - A pair the step's solve did not act on, such as one with a sleeping
    ///   body, keeps an earlier solve's impulses.
    /// - In a step with a batch that capture refused, nothing shows how the
    ///   points moved.
    ///
    /// The impulses are the ones the solve left in Rapier's contacts, which
    /// the next step replaces. A collider the host removes after the step
    /// takes its supports with it.
    pub fn charge_contact_supports(
        &mut self,
        surfaces: &mut impl ContactSurfaces,
    ) -> Result<Vec<SupportCharge>, LoadingError> {
        let unit = f64::from(self.length_unit());
        let state = self.loading.as_mut().ok_or(LoadingError::Disabled)?;
        if !state.speculative_arrivals {
            return Err(LoadingError::ArrivalsDisabled);
        }
        if std::mem::replace(&mut state.supported, true)
            || state.diagnostics.batches_seen != 1
            || state.diagnostics.rejected_batches > 0
        {
            return Ok(Vec::new());
        }
        let state = self.loading.as_ref().unwrap();
        let batch = &state.batches[0];
        let coincident = COINCIDENT * unit;
        let mut unsteady: BTreeSet<_> = batch
            .events
            .iter()
            .filter(|event| event.transition == EpisodeTransition::Impact)
            .map(|event| event.id)
            .collect();
        unsteady.extend(
            batch
                .contacts
                .iter()
                .filter(|record| {
                    let speed =
                        f64::from(record.closing_speed).hypot(f64::from(record.tangent_speed));
                    state.acting(record, batch.duration) && speed > STEADY_SPEED * unit
                })
                .filter_map(|record| record.episode),
        );
        // Rapier solves a pair when it has a dynamic body and each of its
        // dynamic bodies is awake.
        let solved: BTreeSet<_> = batch
            .contacts
            .iter()
            .filter(|record| {
                let dynamic = record.bodies.map(|body| body.kind == BodyKind::Dynamic);
                dynamic.contains(&true)
                    && (0..2).all(|side| !dynamic[side] || !record.bodies[side].sleeping)
            })
            .map(|record| record.colliders)
            .collect();
        let mut charges = Vec::new();
        for (sources, carried) in state.carried(&self.raw, &solved, coincident) {
            let Some(episode) = state.episodes.get(&sources).map(|e| e.id) else {
                continue;
            };
            if carried.landed || unsteady.contains(&episode) {
                continue;
            }
            let support = Support {
                episode,
                sources,
                force: (carried.impulse / f64::from(batch.duration)) as f32,
                duration: batch.duration,
                coverage: patches(&carried.points, 1, coincident).1.swap_remove(0),
            };
            charges.push(support.charge(surfaces));
        }
        Ok(charges)
    }
}

impl LoadingState {
    /// What each pair of bound sources that the step's solve acted on carried.
    /// Rapier's report of a contact's impulse also counts the warm start its
    /// solve began from, which the previous solve applied. The report less
    /// that warm start is what this solve applied. Nothing rebuilds the
    /// contacts after the step's solve, so both are still there.
    ///
    /// A face resting on its pieces' points is indeterminate, so the solve can
    /// load any of them. Each loaded piece therefore covers all its points
    /// within `coincident` of touching, loaded or not.
    fn carried(
        &self,
        raw: &RapierWorld,
        solved: &BTreeSet<[ColliderId; 2]>,
        coincident: f64,
    ) -> BTreeMap<[ContactSource; 2], Carried> {
        let mut carried = BTreeMap::<_, Carried>::new();
        for pair in raw.narrow_phase.contact_pairs() {
            if pair
                .manifolds
                .iter()
                .all(|m| m.data.solver_contacts.is_empty())
            {
                continue;
            }
            // A collider removed since the step keeps its pairs until the next.
            let (Some(first), Some(second)) = (
                raw.colliders.get(pair.collider1),
                raw.colliders.get(pair.collider2),
            ) else {
                continue;
            };
            let Some(ids) = decode_collider(first.user_data).zip(decode_collider(second.user_data))
            else {
                continue;
            };
            let Some((a, b)) = self.bindings.get(&ids.0).zip(self.bindings.get(&ids.1)) else {
                continue;
            };
            if a == b {
                continue;
            }
            // As capture orders a record: by source.
            let order = if a < b { [0, 1] } else { [1, 0] };
            let sources = if a < b { [*a, *b] } else { [*b, *a] };
            let colliders = order.map(|i| [ids.0, ids.1][i]);
            if !solved.contains(&colliders) {
                continue;
            }
            let sign = if a < b { 1.0 } else { -1.0 };
            let mut loaded = Carried::default();
            for manifold in &pair.manifolds {
                let subshapes = [
                    first.shape().as_compound().map(|_| manifold.subshape1),
                    second.shape().as_compound().map(|_| manifold.subshape2),
                ];
                let piece = (colliders, order.map(|i| subshapes[i]));
                let normal = from_rapier(manifold.data.normal) * sign;
                let mut impulse = 0.0;
                let mut touching = Vec::new();
                for contact in &manifold.data.solver_contacts {
                    let Some(point) = manifold.points.get(contact.contact_id[0] as usize) else {
                        continue;
                    };
                    let applied = point.data.impulse - contact.warmstart_impulse;
                    let apart = f64::from(contact.dist) > coincident;
                    if applied > 0.0 {
                        impulse += f64::from(applied);
                        loaded.landed |= apart;
                    }
                    if !apart {
                        touching.push(PatchPoint {
                            group: 0,
                            piece,
                            point: from_rapier(contact.point),
                            normal,
                        });
                    }
                }
                if impulse > 0.0 {
                    loaded.impulse += impulse;
                    loaded.points.append(&mut touching);
                }
            }
            if loaded.impulse > 0.0 {
                let entry = carried.entry(sources).or_default();
                entry.impulse += loaded.impulse;
                entry.landed |= loaded.landed;
                entry.points.append(&mut loaded.points);
            }
        }
        carried
    }
}

/// One support's load in the step.
struct Support {
    episode: EpisodeId,
    sources: [ContactSource; 2],
    force: f32,
    duration: f32,
    coverage: Coverage,
}

impl Support {
    /// Prices the load on each side, and charges the host. Its regions are
    /// live: retiring a region drops its bindings and episodes.
    fn charge(&self, surfaces: &mut impl ContactSurfaces) -> SupportCharge {
        let unpriced = |reason| SupportCharge::Unpriced {
            episode: self.episode,
            sources: self.sources,
            force: self.force,
            reason,
        };
        let [Some(a), Some(b)] = self.sources.map(|s| surfaces.surface(s)) else {
            return unpriced(Unpriced::Undeclared);
        };
        let (width, outcomes) = match self.price(a, b) {
            Ok(priced) => priced,
            Err(error) => return unpriced(Unpriced::Material(error)),
        };
        let protected = absorb(surfaces, self.sources, outcomes.map(|o| o.consumed[0]));
        SupportCharge::Charged {
            episode: self.episode,
            sources: self.sources,
            force: self.force,
            width,
            duration: self.duration,
            outcomes,
            protected,
        }
    }

    /// The sustained law on each side, across what the support's loaded
    /// points cover. No plastic displacement is measured, so an overload
    /// follows the law's fallback.
    fn price(
        &self,
        a: ContactSurface,
        b: ContactSurface,
    ) -> Result<(f32, [Outcome; 2]), MaterialError> {
        let width = width(&a, &b, &self.coverage)?;
        let side = |surface: &ContactSurface| {
            sustained(
                &surface.region,
                SustainedLoad {
                    force: self.force,
                    width,
                    section: section(surface, width)?,
                    duration: self.duration,
                    measured_plastic_displacement: None,
                },
            )
        };
        Ok((width, [side(&a)?, side(&b)?]))
    }
}
