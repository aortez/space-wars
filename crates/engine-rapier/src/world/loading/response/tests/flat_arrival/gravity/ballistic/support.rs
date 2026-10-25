//! Explicit numerical settling and safe rigid support, never a damage charge.
use super::*;
use engine_material::{SustainedLoad, sustained};

mod incoming;
mod tests;

#[derive(Debug, PartialEq)]
pub(super) struct Settlement {
    pub at: f64,
    pub episode: EpisodeId,
    pub available: f64,
    pub dissipated: f64,
    pub impulse: f64,
    pub rounding_impulse: f64,
    pub rounding_loss: f64,
}

#[derive(Debug, PartialEq)]
pub(super) struct Support {
    pub at: f64,
    pub duration: f32,
    pub episode: EpisodeId,
    pub force: f64,
    pub impulse: f64,
    pub rounding_impulse: f64,
    pub rounding_loss: f64,
    pub gravity_work: f64,
    // Auxiliary kick/projection energies cancel. Neither is material work.
    pub kick_energy: f64,
    pub projection_energy: f64,
}

struct Projection {
    pair: [RigidBodyHandle; 2],
}
impl PhysicsHooks for Projection {
    fn report_applied_contact_impulses(&self) -> bool {
        true
    }

    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        if ![context.rigid_body1, context.rigid_body2]
            .iter()
            .all(|h| h.is_some_and(|h| self.pair.contains(&h)))
            || internal_face(context)
        {
            context.solver_contacts.clear();
        }
        context
            .solver_contacts
            .retain(|c| f64::from(c.dist) <= TOUCH);
        for c in context.solver_contacts.iter_mut() {
            c.restitution = 0.0;
        }
    }
}

fn safe(
    world: &PhysicsWorld,
    plates: &Plates,
    dt: f32,
    acceleration: f64,
) -> Result<f64, Rejection> {
    let sources = departure::sources(world, plates)?;
    let force = state(world).2 * acceleration;
    safe_regions(world, sources, force, dt)?;
    Ok(force)
}

fn safe_regions(
    world: &PhysicsWorld,
    sources: [ContactSource; 2],
    force: f64,
    dt: f32,
) -> Result<(), Rejection> {
    for source in sources {
        let stored = &world
            .loading
            .as_ref()
            .unwrap()
            .response
            .as_ref()
            .unwrap()
            .materials[&source.region];
        let region = stored.region().map_err(|_| Rejection::Support)?;
        let section = stored.section();
        if region.failed()
            || force > 2.0 * f64::from(section.thickness) * f64::from(region.effective_strength())
        {
            return Err(Rejection::Support);
        }
        let result = sustained(
            &region,
            SustainedLoad {
                force: force as f32,
                width: 2.0,
                section,
                duration: dt,
                measured_plastic_displacement: Some(0.0),
            },
        )
        .map_err(|_| Rejection::Support)?;
        if result.yielded != [false; 2] || result.consumed != [0.0; 2] {
            return Err(Rejection::Support);
        }
    }
    Ok(())
}

fn reserve(world: &mut PhysicsWorld) -> Result<(), Rejection> {
    let loading = world.loading.as_mut().unwrap();
    if loading.records >= loading.config.max_records_per_step {
        return Err(Rejection::Backend(Refusal::Response(
            ResponseRejection::Capture(CaptureRejection::StepLimit),
        )));
    }
    loading.records += 1;
    Ok(())
}

struct ProjectionResult {
    impulse: f64,
    rounding_impulse: f64,
    dissipated: f64,
    rounding_loss: f64,
}

fn project(world: &mut PhysicsWorld, plates: &Plates) -> Result<ProjectionResult, Rejection> {
    project_pair(world, plates, [plates.handles[0], plates.handles[1]])
}

fn project_pair(
    world: &mut PhysicsWorld,
    plates: &Plates,
    pair: [RigidBodyHandle; 2],
) -> Result<ProjectionResult, Rejection> {
    let start = plates.positions(world);
    let b = &world.raw.bodies[pair[0]];
    let before = f64::from(b.linvel().x);
    let mass = f64::from(b.mass());
    if before <= 0.0 || !world.raw.bodies[pair[1]].is_fixed() {
        return Err(Rejection::Energy);
    }
    let other_motion: Vec<_> = world
        .raw
        .bodies
        .iter()
        .filter(|(h, _)| *h != pair[0])
        .map(|(h, b)| (h, b.linvel(), b.angvel()))
        .collect();
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, f) in &flags {
        world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    world.raw.integration_parameters.dt = 0.0;
    world.raw.step_with_events(&Projection { pair }, &());
    for (h, f) in flags {
        world.raw.colliders[h].set_active_hooks(f);
    }
    let b = &world.raw.bodies[pair[0]];
    let after = f64::from(b.linvel().x);
    let after_mass = f64::from(b.mass());
    let impulse: f64 = world
        .raw
        .narrow_phase
        .contact_pairs()
        .filter(|p| {
            [p.collider1, p.collider2].iter().all(|h| {
                world.raw.colliders[*h]
                    .parent()
                    .is_some_and(|h| pair.contains(&h))
            })
        })
        .map(|p| f64::from(p.total_impulse_magnitude()))
        .sum();
    if start != plates.positions(world)
        || !after.is_finite()
        || after.abs() > 1.0e-6 * before
        || after_mass != mass
        || other_motion.iter().any(|(h, v, w)| {
            world.raw.bodies[*h].linvel() != *v || world.raw.bodies[*h].angvel() != *w
        })
        || (impulse - mass * before).abs() > 1.0e-5 * mass * before
    {
        return Err(Rejection::Energy);
    }
    // The iterative f32 solve can leave a minute velocity in either direction.
    // Make that final projection explicit: retain its signed momentum and its
    // energy loss separately, and never move the bodies to establish contact.
    world.raw.bodies[pair[0]].set_linvel(Vector::ZERO, true);
    Ok(ProjectionResult {
        impulse,
        rounding_impulse: mass * after,
        dissipated: 0.5 * mass * before * before,
        rounding_loss: 0.5 * mass * after * after,
    })
}

pub(super) fn settle(
    world: &mut PhysicsWorld,
    plates: &Plates,
    tracker: &mut departure::Tracker,
    at: f64,
    acceleration: f64,
) -> Result<Settlement, Rejection> {
    tracker.validate(world, plates, acceleration)?;
    let (episode, available) = tracker.settling_budget(world)?;
    safe(world, plates, 1.0, acceleration)?;
    let (_, velocity, mass) = state(world);
    let kinetic = 0.5 * mass * velocity * velocity;
    if !available.is_finite() || available <= 0.0 || kinetic > available * (1.0 + 1.0e-5) {
        return Err(Rejection::Energy);
    }
    reserve(world)?;
    let projection = project(world, plates)?;
    assert_eq!(tracker.held(world, projection.dissipated)?, episode);
    Ok(Settlement {
        at,
        episode,
        available,
        dissipated: projection.dissipated,
        impulse: projection.impulse,
        rounding_impulse: projection.rounding_impulse,
        rounding_loss: projection.rounding_loss,
    })
}

pub(super) fn hold(
    world: &mut PhysicsWorld,
    plates: &Plates,
    tracker: &mut departure::Tracker,
    at: f64,
    duration: f32,
    acceleration: f64,
) -> Result<(Support, Option<TimedEvent>), Rejection> {
    tracker.validate(world, plates, acceleration)?;
    let force = safe(world, plates, duration, acceleration)?;
    let mut event = None;
    if world.loading.as_ref().unwrap().episodes.is_empty() {
        let (velocity, _) = advance(world, plates, 0.0, false, true).map_err(Rejection::Backend)?;
        if velocity != vec![Vector::ZERO; 2] {
            return Err(Rejection::Energy);
        }
        let events = &world.loading_batches().last().unwrap().events;
        if events.len() != 1 || events[0].transition != EpisodeTransition::Support {
            return Err(Rejection::Evidence);
        }
        event = Some(TimedEvent {
            at,
            event: events[0],
        });
        tracker.anchor(world, plates, acceleration)?;
    }
    reserve(world)?;
    let start = plates.positions(world);
    let (x, velocity, mass) = state(world);
    if x.abs() > TOUCH || velocity != 0.0 {
        return Err(Rejection::Evidence);
    }
    let kick = (0.5 * acceleration * f64::from(duration)) as f32;
    if !kick.is_finite() || kick <= 0.0 {
        return Err(Rejection::Energy);
    }
    let mut impulse = 0.0;
    let mut rounding_impulse = 0.0;
    let mut rounding_loss = 0.0;
    let mut projection_energy = 0.0;
    for phase in 0..2 {
        world.raw.bodies[plates.handles[0]].set_linvel(Vector::X * kick, true);
        let projection = project(world, plates)?;
        impulse += projection.impulse;
        rounding_impulse += projection.rounding_impulse;
        rounding_loss += projection.rounding_loss;
        projection_energy += projection.dissipated;
        if phase == 0 {
            // No force or motion in this drift: the two projections balance
            // gravity, while geometry and the live support episode persist.
            // Suppress warm-start/bias impulses here; support was already
            // solved by the first projection at the unchanged touching pose.
            let flags: Vec<_> = world
                .raw
                .colliders
                .iter()
                .map(|(h, c)| (h, c.active_hooks()))
                .collect();
            for (h, f) in &flags {
                world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
            }
            world.raw.integration_parameters.dt = duration;
            world.raw.step_with_events(&FreeDrift, &());
            for (h, f) in flags {
                world.raw.colliders[h].set_active_hooks(f);
            }
        }
    }
    let (end, velocity, after_mass) = state(world);
    let kick_energy = mass * f64::from(kick) * f64::from(kick);
    if plates.positions(world) != start
        || velocity != 0.0
        || mass != after_mass
        || (impulse + rounding_impulse - force * f64::from(duration)).abs()
            > 1.0e-5 * force * f64::from(duration)
        || projection_energy != kick_energy
    {
        return Err(Rejection::Energy);
    }
    let episode = tracker.held(world, 0.0)?;
    Ok((
        Support {
            at,
            duration,
            episode,
            force,
            impulse,
            rounding_impulse,
            rounding_loss,
            gravity_work: mass * acceleration * (end - x),
            kick_energy,
            projection_energy,
        },
        event,
    ))
}

struct Verified {
    world: PhysicsWorld,
    tracker: departure::Tracker,
}
impl Verified {
    fn snapshot(&self) -> Vec<u8> {
        bincode::serialize(&(self.world.snapshot_bytes().unwrap(), &self.tracker)).unwrap()
    }
    fn restore(bytes: &[u8]) -> Self {
        let (physics, tracker): (Vec<u8>, departure::Tracker) =
            bincode::deserialize(bytes).unwrap();
        Self {
            world: PhysicsWorld::from_snapshot_bytes(&physics).unwrap(),
            tracker,
        }
    }
    fn step(&mut self, dt: f32, limit: usize) -> Result<Trace, Rejection> {
        step_mode(
            &mut self.world,
            dt,
            limit,
            Some(&mut self.tracker),
            SupportMode::Settling,
        )
    }
}
