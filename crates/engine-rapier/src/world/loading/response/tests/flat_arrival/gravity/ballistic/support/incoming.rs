//! A free plate arrives at a plate already supported by a fixed base.
//! Each proven new arrival owns incoming work; the old support transmits it.
use super::*;

mod native;
mod stack;
mod tests;

#[derive(Clone, Serialize, Deserialize)]
struct Witness {
    sources: Vec<ContactSource>,
    support: EpisodeId,
    incoming: Option<EpisodeId>,
    motion: Vec<(f32, f32, f32)>,
    gravity: f32,
    rounding: numerics::Ledger,
    stack: Option<stack::History>,
}

struct Verified {
    world: PhysicsWorld,
    witness: Witness,
}

fn motion(world: &PhysicsWorld, plates: &Plates) -> Vec<(f32, f32, f32)> {
    plates
        .handles
        .iter()
        .map(|h| {
            let b = &world.raw.bodies[*h];
            (b.translation().x, b.linvel().x, b.mass())
        })
        .collect()
}

impl Verified {
    fn new(world: PhysicsWorld) -> Result<Self, Rejection> {
        let plates = Plates::new(&world).map_err(Rejection::Backend)?;
        let sources = departure::bound_sources(&world, &plates.handles)?;
        if sources.len() != 3 {
            return Err(Rejection::Geometry);
        }
        let support = world
            .loading
            .as_ref()
            .unwrap()
            .episodes
            .get(&[sources[1], sources[2]])
            .ok_or(Rejection::Evidence)?
            .id;
        let witness = Witness {
            sources,
            support,
            incoming: None,
            motion: motion(&world, &plates),
            gravity: world.raw.gravity.x,
            rounding: numerics::Ledger::new(&world, f64::from(world.raw.gravity.x))?,
            stack: None,
        };
        let result = Self { world, witness };
        result.validate(&plates)?;
        Ok(result)
    }

    fn with_stack(world: PhysicsWorld, settling: bool) -> Result<Self, Rejection> {
        let mut result = Self::new(world)?;
        result.witness.stack = Some(stack::History::new(settling));
        Ok(result)
    }

    fn snapshot(&self) -> Vec<u8> {
        bincode::serialize(&(self.world.snapshot_bytes().unwrap(), &self.witness)).unwrap()
    }

    fn restore(bytes: &[u8]) -> Self {
        let (physics, witness): (Vec<u8>, Witness) = bincode::deserialize(bytes).unwrap();
        Self {
            world: PhysicsWorld::from_snapshot_bytes(&physics).unwrap(),
            witness,
        }
    }

    fn validate(&self, plates: &Plates) -> Result<(), Rejection> {
        let world = &self.world;
        let g = world.raw.gravity;
        self.witness.rounding.validate(world, f64::from(g.x))?;
        if plates.handles.len() != 3
            || plates.normal != [1.0, 0.0]
            || !g.x.is_finite()
            || g.x <= 0.0
            || g.y != 0.0
            || !world.raw.bodies[plates.handles[0]].is_dynamic()
            || !world.raw.bodies[plates.handles[1]].is_dynamic()
            || !world.raw.bodies[plates.handles[2]].is_fixed()
            || world.raw.bodies[plates.handles[1]].translation() != Vector::X
            || world.raw.bodies[plates.handles[2]].translation() != Vector::X * 2.0
            || plates.handles[1..]
                .iter()
                .any(|h| world.raw.bodies[*h].linvel() != Vector::ZERO)
            || !world.raw.impulse_joints.is_empty()
            || world.raw.multibody_joints.iter().next().is_some()
            || world.raw.bodies.iter().any(|(_, b)| {
                b.user_force() != Vector::ZERO
                    || b.user_torque() != 0.0
                    || b.linear_damping() != 0.0
                    || b.angular_damping() != 0.0
                    || b.is_ccd_enabled()
                    || b.gravity_scale() != 1.0
                    || b.locked_axes().intersects(LockedAxes::TRANSLATION_LOCKED)
                    || (b.is_dynamic() && (!b.mass().is_finite() || b.mass() <= 0.0))
                    || !b.linvel().is_finite()
                    || !b.translation().is_finite()
                    || b.linvel().y != 0.0
                    || b.translation().y != 0.0
            })
            || world.raw.colliders.iter().any(|(_, c)| c.friction() != 0.0)
        {
            return Err(Rejection::Geometry);
        }
        let sources = departure::bound_sources(world, &plates.handles)?;
        let episodes = &world.loading.as_ref().unwrap().episodes;
        if sources != self.witness.sources
            || sources.windows(2).any(|s| s[0].region >= s[1].region)
            || self.witness.motion != motion(world, plates)
            || self.witness.gravity != g.x
            || episodes.len() != 1 + usize::from(self.witness.incoming.is_some())
            || episodes.get(&[sources[1], sources[2]]).map(|e| e.id) != Some(self.witness.support)
            || episodes.get(&[sources[0], sources[1]]).map(|e| e.id) != self.witness.incoming
        {
            return Err(Rejection::Evidence);
        }
        if let Some(stack) = &self.witness.stack {
            stack.validate(self)?;
        }
        Ok(())
    }

    fn force(&self, plates: &Plates) -> f64 {
        f64::from(self.world.raw.bodies[plates.handles[1]].mass()) * f64::from(self.witness.gravity)
    }

    fn safe_support(&self, plates: &Plates, dt: f32) -> Result<(), Rejection> {
        safe_regions(
            &self.world,
            [self.witness.sources[1], self.witness.sources[2]],
            self.force(plates),
            dt,
        )
    }

    fn flight(
        &mut self,
        plates: &Plates,
        at: f64,
        dt: f32,
    ) -> Result<(Flight, Support), Rejection> {
        self.safe_support(plates, dt)?;
        reserve(&mut self.world)?;
        let acceleration = f64::from(self.witness.gravity);
        let kick = (0.5 * acceleration * f64::from(dt)) as f32;
        if !kick.is_finite() || kick <= 0.0 {
            return Err(Rejection::Energy);
        }
        let pair = [plates.handles[1], plates.handles[2]];
        // These support-only projections commute with the free body's kicks.
        // Even at arrival, they must not alter the incoming body's velocity.
        self.world.raw.bodies[pair[0]].set_linvel(Vector::X * kick, true);
        let first = project_pair(&mut self.world, plates, pair)?;
        let before_v = state(&self.world).1;
        let flight = fly(
            &mut self.world,
            plates,
            dt,
            acceleration,
            Some(&mut self.witness.rounding),
        )?;
        self.world.raw.bodies[pair[0]].set_linvel(Vector::X * kick, true);
        let second = project_pair(&mut self.world, plates, pair)?;
        let force = self.force(plates);
        let impulse = first.impulse + second.impulse;
        let rounding_impulse = first.rounding_impulse + second.rounding_impulse;
        let projection_energy = first.dissipated + second.dissipated;
        let kick_energy =
            f64::from(self.world.raw.bodies[pair[0]].mass()) * f64::from(kick) * f64::from(kick);
        if (impulse + rounding_impulse - force * f64::from(dt)).abs()
            > 1.0e-5 * force * f64::from(dt)
            || projection_energy != kick_energy
        {
            return Err(Rejection::Energy);
        }
        self.witness.motion = motion(&self.world, plates);
        let after_v = state(&self.world).1;
        if let Some(stack) = &mut self.witness.stack {
            stack.after_flight(before_v, after_v, &flight);
        }
        Ok((
            flight,
            Support {
                at,
                duration: dt,
                episode: self.witness.support,
                force,
                impulse,
                rounding_impulse,
                rounding_loss: first.rounding_loss + second.rounding_loss,
                gravity_work: 0.0,
                kick_energy,
                projection_energy,
            },
        ))
    }

    fn depart(&mut self, flight: &Flight, trace: &mut Trace, at: f64) -> Result<(), Rejection> {
        let Some(episode) = self.witness.incoming else {
            return Ok(());
        };
        // Every accepted flight extends the exact motion witness, even when
        // rounding crosses the band before the predicted endpoint does. Keep
        // the owner until both endpoints prove separation beyond uncertainty.
        let lower = (-state(&self.world).0).min(-flight.predicted_x) - flight.uncertainty;
        let loading = self.world.loading.as_ref().unwrap();
        if lower <= 2.0 * f64::from(loading.config.touch_tolerance) {
            return Ok(());
        }
        let sources = [self.witness.sources[0], self.witness.sources[1]];
        if loading.episodes.get(&sources).map(|e| e.id) != Some(episode) {
            return Err(Rejection::Evidence);
        }
        reserve(&mut self.world)?;
        self.world
            .loading
            .as_mut()
            .unwrap()
            .episodes
            .remove(&sources);
        self.witness.incoming = None;
        if let Some(stack) = &mut self.witness.stack {
            stack.depart();
        }
        trace.events.push(TimedEvent {
            at,
            event: EpisodeEvent {
                id: episode,
                sources,
                transition: EpisodeTransition::End,
                closing_speed: 0.0,
            },
        });
        Ok(())
    }

    fn impact(&mut self, plates: &Plates, trace: &mut Trace, at: f64) -> Result<(), Rejection> {
        if self.witness.incoming.is_some() {
            return Err(Rejection::Reimpact);
        }
        let start = plates.positions(&self.world);
        let (_, incoming, mass) = state(&self.world);
        let count = self.world.material_response_batches().len();
        let (velocity, _) =
            advance(&mut self.world, plates, 0.0, false, true).map_err(Rejection::Backend)?;
        if plates.positions(&self.world) != start
            || velocity[0].x > 0.0
            || (velocity[0].x == 0.0 && self.witness.stack.is_none())
            || velocity[1].y != 0.0
            || velocity[2] != Vector::ZERO
            || self.world.material_response_batches().len() != count + 1
        {
            return Err(Rejection::Energy);
        }
        let response = self.world.material_response_batches()[count]
            .result
            .as_ref()
            .unwrap()
            .clone();
        let events = self.world.loading_batches().last().unwrap().events.clone();
        if events.len() != 1
            || events[0].transition != EpisodeTransition::Impact
            || events[0].sources != [self.witness.sources[0], self.witness.sources[1]]
            || response.roots != vec![events[0].id]
            || events[0].id == self.witness.support
            || response.impacts.len() != 2
            || response.impacts[1].episode != self.witness.support
            || response.impacts[0].episode != events[0].id
            || response.impacts[0].sources != events[0].sources
            || response.impacts[1].sources != [self.witness.sources[1], self.witness.sources[2]]
            || response.prescribed_work != 0.0
            || response.budget.components.len() != 1
        {
            return Err(Rejection::Evidence);
        }
        let outgoing = velocity[0].x;
        let energy = 0.5 * mass * incoming * incoming;
        let recoil = self.correct_support_recoil(plates, trace, at, incoming)?;
        let removed =
            0.5 * mass * (incoming - f64::from(outgoing)) * (incoming + f64::from(outgoing))
                - recoil;
        let impulse = mass * (incoming - f64::from(outgoing));
        if (removed - loss(&response)).abs() > 1.0e-5 * energy
            || (response.budget.components[0].available - energy).abs() > 1.0e-5 * energy
            || response
                .impacts
                .iter()
                .any(|i| (f64::from(i.normal_impulse) - impulse).abs() > 1.0e-5 * impulse)
        {
            return Err(Rejection::Energy);
        }
        self.witness.incoming = Some(events[0].id);
        let energy = mechanical(&self.world, f64::from(self.witness.gravity));
        let x = state(&self.world).0;
        if let Some(stack) = &mut self.witness.stack {
            stack.anchor(energy, -x, outgoing == 0.0)?;
        }
        trace
            .events
            .extend(events.iter().map(|event| TimedEvent { at, event: *event }));
        trace.impacts.push(Impact {
            at,
            incoming: incoming as f32,
            outgoing,
            response,
        });
        self.witness.motion = motion(&self.world, plates);
        // Failure needs a physical consequence; this held-base fixture cannot
        // continue supporting a failed region, even on a frame-end arrival.
        self.safe_support(plates, 1.0)?;
        if outgoing == 0.0 {
            self.safe_stack(plates, 1.0)?;
        }
        self.witness.rounding.after_response(&self.world)?;
        Ok(())
    }

    fn step(&mut self, dt: f32, limit: usize) -> Result<Trace, Rejection> {
        self.step_with_projection_limit(dt, limit, stack::MAX_SOLVES)
    }

    fn step_with_projection_limit(
        &mut self,
        dt: f32,
        limit: usize,
        projection_limit: usize,
    ) -> Result<Trace, Rejection> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(Rejection::InvalidTime);
        }
        if dt == 0.0 {
            self.world.step(0.0);
            return Ok(Trace::default());
        }
        let plates = Plates::new(&self.world).map_err(Rejection::Backend)?;
        self.validate(&plates)?;
        self.safe_support(&plates, dt)?;
        let mut staged = Self {
            world: self.world.clone(),
            witness: self.witness.clone(),
        };
        staged.world.raw.gravity = Vector::ZERO;
        staged.world.loading.as_mut().unwrap().begin_step();
        let mut trace = Trace::default();
        let mut elapsed = 0.0;
        let mut solves = 0;
        loop {
            let (x, v, _) = state(&staged.world);
            if x > TOUCH {
                return Err(Rejection::Geometry);
            }
            if -x <= TOUCH && v >= 0.0 {
                if v == 0.0 {
                    if staged.witness.stack.is_none() {
                        return Err(Rejection::Support);
                    }
                    if elapsed == f64::from(dt) {
                        break;
                    }
                    if trace.flights.len() + trace.stack_supports.len() >= limit.min(MAX_ADVANCES) {
                        return Err(Rejection::Limit);
                    }
                    let remaining = f64::from(dt) - elapsed;
                    let duration = bounded_duration(remaining, remaining)?;
                    let next = exact_advance(elapsed, duration, f64::from(dt))?;
                    staged.hold_stack(
                        &plates,
                        &mut trace,
                        elapsed,
                        duration,
                        &mut solves,
                        projection_limit,
                    )?;
                    elapsed = next;
                    continue;
                }
                staged.arrive(&plates, &mut trace, elapsed, &mut solves, projection_limit)?;
                if state(&staged.world).1 == 0.0 {
                    continue;
                }
            }
            if elapsed >= f64::from(dt) {
                break;
            }
            if trace.flights.len() + trace.stack_supports.len() >= limit.min(MAX_ADVANCES) {
                return Err(Rejection::Limit);
            }
            let (x, v, _) = state(&staged.world);
            let remaining = f64::from(dt) - elapsed;
            let toi = arrival(-x, v, f64::from(self.witness.gravity));
            let mut at_arrival = toi <= remaining;
            let mut duration = bounded_duration(toi.min(remaining), remaining)?;
            if v < 0.0 {
                let apex = departure::apex_duration(v, f64::from(self.witness.gravity))?;
                if apex < duration {
                    duration = apex;
                    at_arrival = false;
                }
            }
            let next = exact_advance(elapsed, duration, f64::from(dt))?;
            let (flight, support) = staged.flight(&plates, elapsed, duration)?;
            elapsed = next;
            staged.depart(&flight, &mut trace, elapsed)?;
            trace.flights.push(flight);
            trace.supports.push(support);
            let (end_x, end_v, _) = state(&staged.world);
            if at_arrival || (-end_x <= TOUCH && end_v > 0.0) {
                if state(&staged.world).0.abs() > TOUCH {
                    return Err(Rejection::Geometry);
                }
                staged.arrive(&plates, &mut trace, elapsed, &mut solves, projection_limit)?;
            }
            if elapsed == f64::from(dt) {
                break;
            }
        }
        staged.world.raw.gravity = self.world.raw.gravity;
        staged.validate(&plates)?;
        *self = staged;
        Ok(trace)
    }
}

fn bounded_duration(preferred: f64, remaining: f64) -> Result<f32, Rejection> {
    let mut duration = preferred as f32;
    if f64::from(duration) > remaining {
        duration = f32::from_bits(duration.to_bits() - 1);
    }
    if !duration.is_finite() || duration <= 0.0 || f64::from(duration) > remaining {
        return Err(Rejection::Energy);
    }
    Ok(duration)
}

fn exact_advance(elapsed: f64, duration: f32, total: f64) -> Result<f64, Rejection> {
    let h = f64::from(duration);
    let next = elapsed + h;
    // Keep every accepted interval, including a rounded final remainder.
    let lost = if elapsed >= h {
        (elapsed - next) + h
    } else {
        (h - next) + elapsed
    };
    if lost != 0.0 || next <= elapsed || next > total {
        return Err(Rejection::Energy);
    }
    Ok(next)
}
