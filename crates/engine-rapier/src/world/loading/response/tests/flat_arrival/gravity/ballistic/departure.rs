//! Fixture-only accelerated departure evidence, separate from native snapshots.
use super::*;

mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Witness {
    sources: [ContactSource; 2],
    episode: EpisodeId,
    motion: (f64, f64, f64),
    acceleration: f64,
    energy: f64,
    gap_upper: f64,
    turned: bool,
    held: bool,
    settled_work: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct Tracker {
    pending: Option<Witness>,
    pub(super) rounding: Option<numerics::Ledger>,
}

pub(super) fn sources(
    world: &PhysicsWorld,
    plates: &Plates,
) -> Result<[ContactSource; 2], Rejection> {
    let mut result = bound_sources(world, &plates.handles)?;
    result.sort();
    if result.len() != 2 || result[0].region == result[1].region {
        return Err(Rejection::Evidence);
    }
    Ok([result[0], result[1]])
}

pub(super) fn bound_sources(
    world: &PhysicsWorld,
    handles: &[RigidBodyHandle],
) -> Result<Vec<ContactSource>, Rejection> {
    let loading = world.loading.as_ref().unwrap();
    let mut result = Vec::new();
    for handle in handles {
        let b = &world.raw.bodies[*handle];
        let mut source = None;
        for handle in b.colliders() {
            let collider = &world.raw.colliders[*handle];
            let id = decode_collider(collider.user_data).ok_or(Rejection::Evidence)?;
            let binding = *loading.bindings.get(&id).ok_or(Rejection::Evidence)?;
            if !loading.live(binding.region)
                || source.is_some_and(|previous| previous != binding)
                || b.user_data != encode_body(loading.regions[&binding.region.id].body)
            {
                return Err(Rejection::Evidence);
            }
            source = Some(binding);
        }
        result.push(source.ok_or(Rejection::Evidence)?);
    }
    Ok(result)
}

impl Tracker {
    pub(super) fn start(
        &mut self,
        world: &PhysicsWorld,
        plates: &Plates,
        acceleration: f64,
    ) -> Result<(), Rejection> {
        self.validate(world, plates, acceleration)?;
        if self.rounding.is_none() {
            self.rounding = Some(numerics::Ledger::new(world, acceleration)?);
        }
        Ok(())
    }

    pub(super) fn validate(
        &self,
        world: &PhysicsWorld,
        plates: &Plates,
        acceleration: f64,
    ) -> Result<(), Rejection> {
        if let Some(ledger) = &self.rounding {
            ledger.validate(world, acceleration)?;
        }
        let sources = sources(world, plates)?;
        let episodes = &world.loading.as_ref().unwrap().episodes;
        match &self.pending {
            None if episodes.is_empty() => Ok(()),
            Some(witness)
                if episodes.len() == 1
                    && witness.sources == sources
                    && episodes
                        .get(&sources)
                        .is_some_and(|e| e.id == witness.episode)
                    && witness.motion == state(world)
                    && witness.acceleration == acceleration =>
            {
                Ok(())
            }
            _ => Err(Rejection::Evidence),
        }
    }

    pub(super) fn anchor(
        &mut self,
        world: &PhysicsWorld,
        plates: &Plates,
        acceleration: f64,
    ) -> Result<(), Rejection> {
        if self.pending.is_some() {
            return Err(Rejection::Evidence);
        }
        let sources = sources(world, plates)?;
        let episodes = &world.loading.as_ref().unwrap().episodes;
        let episode = episodes.get(&sources).ok_or(Rejection::Evidence)?.id;
        let motion = state(world);
        if episodes.len() != 1 || motion.0.abs() > TOUCH || motion.1 > 0.0 {
            return Err(Rejection::Evidence);
        }
        self.pending = Some(Witness {
            sources,
            episode,
            motion,
            acceleration,
            energy: mechanical(world, acceleration),
            gap_upper: -motion.0,
            turned: false,
            held: false,
            settled_work: 0.0,
        });
        if let Some(ledger) = &mut self.rounding {
            ledger.after_response(world)?;
        }
        Ok(())
    }

    pub(super) fn after_flight(
        &mut self,
        world: &mut PhysicsWorld,
        plates: &Plates,
        acceleration: f64,
        flight: &Flight,
    ) -> Result<Option<EpisodeEvent>, Rejection> {
        let Some(witness) = &mut self.pending else {
            return Ok(None);
        };
        // validate() checked the exact starting endpoint, velocity, gravity and
        // owner before this drift. Keep that chain even if rounding passes the
        // exit threshold before the conservative lower bound does.
        let end = state(world);
        witness.turned |= witness.motion.1 < 0.0 && end.1 >= 0.0;
        witness.gap_upper = witness.gap_upper.max(flight.gap_upper);
        witness.motion = end;
        self.validate(world, plates, acceleration)?;
        let witness = self.pending.as_ref().unwrap();
        let lower = (-state(world).0).min(-flight.predicted_x) - flight.uncertainty;
        let loading = world.loading.as_mut().unwrap();
        if lower <= 2.0 * f64::from(loading.config.touch_tolerance) {
            return Ok(None);
        }
        if loading.records >= loading.config.max_records_per_step {
            return Err(Rejection::Backend(Refusal::Response(
                ResponseRejection::Capture(CaptureRejection::StepLimit),
            )));
        }
        let event = EpisodeEvent {
            id: witness.episode,
            sources: witness.sources,
            transition: EpisodeTransition::End,
            closing_speed: 0.0,
        };
        loading.records += 1;
        loading.episodes.remove(&witness.sources);
        self.pending = None;
        Ok(Some(event))
    }

    pub(super) fn settling_budget(
        &self,
        world: &PhysicsWorld,
    ) -> Result<(EpisodeId, f64), Rejection> {
        let w = self.pending.as_ref().ok_or(Rejection::Evidence)?;
        let exit = 2.0 * f64::from(world.loading.as_ref().unwrap().config.touch_tolerance);
        if w.held || !w.turned || w.gap_upper > exit || w.settled_work != 0.0 {
            return Err(Rejection::Reimpact);
        }
        Ok((w.episode, w.energy))
    }

    pub(super) fn held(
        &mut self,
        world: &PhysicsWorld,
        dissipated: f64,
    ) -> Result<EpisodeId, Rejection> {
        let w = self.pending.as_mut().ok_or(Rejection::Evidence)?;
        if w.held && dissipated != 0.0 {
            return Err(Rejection::Evidence);
        }
        w.motion = state(world);
        w.held = true;
        w.settled_work += dissipated;
        if let Some(ledger) = &mut self.rounding {
            ledger.after_response(world)?;
        }
        Ok(w.episode)
    }
}

pub(super) fn apex_duration(velocity: f64, acceleration: f64) -> Result<f32, Rejection> {
    let mut dt = (-velocity / acceleration) as f32;
    // Round toward/just beyond the apex so a residual negative velocity does
    // not cause a second, numerically tiny turn split. Match the compensated
    // final kick used by fly(), rather than adding two rounded increments.
    for _ in 0..3 {
        if !dt.is_finite() || dt <= 0.0 {
            return Err(Rejection::Energy);
        }
        if velocity + acceleration * f64::from(dt) >= 0.0 {
            return Ok(dt);
        }
        dt = f32::from_bits(dt.to_bits() + 1);
    }
    Err(Rejection::Energy)
}

struct Verified {
    world: PhysicsWorld,
    tracker: Tracker,
}

impl Verified {
    fn snapshot(&self) -> Vec<u8> {
        bincode::serialize(&(self.world.snapshot_bytes().unwrap(), &self.tracker)).unwrap()
    }

    fn restore(bytes: &[u8]) -> Self {
        let (physics, tracker): (Vec<u8>, Tracker) = bincode::deserialize(bytes).unwrap();
        Self {
            world: PhysicsWorld::from_snapshot_bytes(&physics).unwrap(),
            tracker,
        }
    }

    fn step(&mut self, dt: f32, limit: usize) -> Result<Trace, Rejection> {
        step_tracked(&mut self.world, dt, limit, Some(&mut self.tracker))
    }
}
