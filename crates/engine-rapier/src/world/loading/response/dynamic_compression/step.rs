//! Atomic midpoint/drift solves with contact ownership across unilateral motion.
use super::*;
struct CompliantPair;
impl PhysicsHooks for CompliantPair {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        context.solver_contacts.clear();
    }
}
pub(super) fn advance(world: &mut PhysicsWorld, dt: f32) -> Result<DynamicCompressionStep, R> {
    if !dt.is_finite() || !(0.0..=1.0 / 30.0).contains(&dt) {
        return Err(R::InvalidCommand);
    }
    if dt == 0.0 {
        world.step(0.0);
        return Ok(DynamicCompressionStep {
            summary: world.material_dynamic_compression_summary(),
            ..Default::default()
        });
    }
    let saved = world
        .loading
        .as_ref()
        .and_then(|l| l.dynamic_compression.as_ref());
    if saved.is_some_and(|c| c.active().is_none() || !c.valid_for(world)) {
        return Err(R::StaleContinuation);
    }
    let scene = Scene::read(world)?;
    if scene.plates.material.remaining <= 0.0 {
        return Err(R::FailedRegion);
    }
    if saved.is_none() && !world.loading.as_ref().unwrap().episodes.is_empty() {
        return Err(R::UnsupportedScene);
    }
    let mut history = if let Some(s) = saved {
        s.history.clone()
    } else {
        History::new(&scene)?
    };
    let (count, h) = history.subdivision(dt)?;
    let loading = world.loading.as_ref().unwrap();
    if count + usize::from(saved.is_none() && scene.plates.x >= 0.0)
        > loading.config.max_records_per_step
    {
        return Err(R::Limit);
    }
    let mut staged =
        PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().map_err(|_| R::Snapshot)?)
            .map_err(|_| R::Snapshot)?;
    let loading = staged.loading.as_mut().unwrap();
    loading.begin_step();
    let mut events = Vec::new();
    let revision = history.revision();
    let mut pair = scene.plates.sources;
    pair.sort();
    let mut owner = if let Some(c) = saved {
        c.active().unwrap().1
    } else if revision == 3 {
        None
    } else {
        Some(enter(
            loading,
            pair,
            0.0,
            0.0,
            EpisodeTransition::Support,
            &mut events,
        )?)
    };
    let mut pending_support =
        saved.is_some_and(|c| matches!(&c.status, Status::Falling(a) if a.pending_support));
    let mut current = scene;
    let mut intervals = Vec::with_capacity(count);
    for i in 0..count {
        let at = (i as f64) * f64::from(h);
        if pending_support {
            events.push(CompressionEvent {
                at,
                event: EpisodeEvent {
                    id: owner.ok_or(R::Numerical)?,
                    sources: pair,
                    transition: EpisodeTransition::Support,
                    closing_speed: current.plates.speed.max(0.0),
                },
            });
            pending_support = false;
        }
        let dx = history.displacement(f64::from(h))?;
        let middle = (dx / f64::from(h)) as f32;
        if !middle.is_finite() {
            return Err(R::Numerical);
        }
        let normal = Vector::new(current.plates.normal[0], current.plates.normal[1]);
        let handle = staged
            .body_handle(current.plates.bodies[0])
            .ok_or(R::UnsupportedScene)?;
        staged.raw.bodies[handle].set_linvel(normal * middle, true);
        gravity::phase::auxiliary_step(&mut staged, h, &CompliantPair);
        let mut after = Scene::read(&staged)?;
        let mut expected = current.clone();
        expected.plates.x = after.plates.x;
        expected.plates.speed = middle;
        if after != expected || (revision == 1 && dx > 0.0 && after.plates.x <= current.plates.x) {
            return Err(R::Numerical);
        }
        let curve = MaterialCurve::new(&current.plates.material);
        let q = curve.segment(
            current.plates.x,
            history.summary.plastic_indentation,
            after.plates.x,
            f64::from(h),
        );
        let next = (f64::from(current.plates.speed)
            + f64::from(h) * (f64::from(current.acceleration) - q.mean / f64::from(current.mass)))
            as f32;
        // Ownership follows the actual monotone backend drift, not a predicted
        // endpoint or a raw contact manifold on the undeformed envelope.
        let old_p = history.summary.plastic_indentation;
        let crossing =
            || at + f64::from(h) * (old_p - current.plates.x) / (after.plates.x - current.plates.x);
        if owner.is_none() && after.plates.x >= old_p {
            owner = Some(enter(
                staged.loading.as_mut().unwrap(),
                pair,
                if current.plates.x == old_p {
                    at
                } else {
                    crossing()
                },
                middle.max(0.0),
                if revision == 3 {
                    EpisodeTransition::Impact
                } else {
                    EpisodeTransition::Support
                },
                &mut events,
            )?);
            pending_support = revision == 3;
        }
        let interval = history.finish(&current, after.plates.x, next, h, at, owner)?;
        if after.plates.x < history.summary.plastic_indentation
            && let Some(id) = owner.take()
        {
            events.push(CompressionEvent {
                at: crossing(),
                event: EpisodeEvent {
                    id,
                    sources: pair,
                    transition: EpisodeTransition::End,
                    closing_speed: 0.0,
                },
            });
            staged.loading.as_mut().unwrap().episodes.remove(&pair);
            pending_support = false;
        }
        if count + events.len() > staged.loading.as_ref().unwrap().config.max_records_per_step {
            return Err(R::Limit);
        }
        staged.raw.bodies[handle].set_linvel(normal * next, true);
        after.plates.speed = next;
        after.plates.material = history.current_material();
        staged
            .loading
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .materials
            .insert(
                after.plates.sources[1].region,
                after.plates.material.clone(),
            );
        intervals.push(interval);
        current = after;
    }
    let summary = history.summary.clone();
    staged.loading.as_mut().unwrap().dynamic_compression = Some(Continuation {
        revision,
        history,
        status: if revision == 1 {
            Status::Active(Box::new(Active {
                scene: current,
                owner: owner.ok_or(R::Numerical)?,
            }))
        } else if revision == 3 {
            Status::Falling(Box::new(FallingActive {
                scene: current,
                owner,
                pending_support,
            }))
        } else {
            Status::Rebound(Box::new(ReboundActive {
                scene: current,
                owner,
            }))
        },
    });
    if !staged.loading.as_ref().unwrap().valid_for(&staged) {
        return Err(R::Numerical);
    }
    *world = staged;
    Ok(DynamicCompressionStep {
        events,
        intervals,
        summary: Some(summary),
    })
}

fn enter(
    loading: &mut LoadingState,
    pair: [ContactSource; 2],
    at: f64,
    speed: f32,
    transition: EpisodeTransition,
    events: &mut Vec<CompressionEvent>,
) -> Result<EpisodeId, R> {
    if loading.episodes.len() >= loading.config.max_episodes {
        return Err(R::Limit);
    }
    let next = loading.next_episode.checked_add(1).ok_or(R::Limit)?;
    let id = EpisodeId(loading.next_episode);
    loading.next_episode = next;
    loading.episodes.insert(
        pair,
        Episode {
            id,
            absent_seconds: 0.0,
        },
    );
    events.push(CompressionEvent {
        at,
        event: EpisodeEvent {
            id,
            sources: pair,
            transition,
            closing_speed: speed,
        },
    });
    Ok(id)
}
