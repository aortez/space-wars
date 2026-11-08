//! Snapshot transaction and one force-free prescribed backend drift.
use super::*;

pub(super) fn advance(world: &mut PhysicsWorld, dt: f32, speed: f32) -> Result<CompressionStep, R> {
    if !dt.is_finite() || !(0.0..=1.0).contains(&dt) || !speed.is_finite() || speed.abs() > 0.25 {
        return Err(R::InvalidCommand);
    }
    if dt == 0.0 {
        world.step(0.0);
        return Ok(CompressionStep {
            summary: world.material_compression_summary(),
            ..Default::default()
        });
    }
    let saved = world.loading.as_ref().and_then(|l| l.compression.as_ref());
    if saved.is_some_and(|c| !matches!(c.status, Status::Active(_)) || !c.valid_for(world)) {
        return Err(R::StaleContinuation);
    }
    let before = Scene::read(world)?;
    if before.material.remaining <= 0.0 {
        return Err(R::FailedRegion);
    }
    let mut continuation = if let Some(saved) = saved {
        saved.clone()
    } else {
        if before.x != 0.0
            || before.speed != 0.0
            || !world.loading.as_ref().unwrap().episodes.is_empty()
        {
            return Err(R::UnsupportedScene);
        }
        Continuation {
            revision: 1,
            history: History::new(before.material.clone())?,
            status: Status::Active(Box::new(Active {
                scene: before.clone(),
                owner: None,
            })),
        }
    };
    let old_p = continuation.history.summary.plastic_indentation;
    let Status::Active(active) = &mut continuation.status else {
        unreachable!()
    };
    let mut pair = before.sources;
    pair.sort();
    let mut staged =
        PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().map_err(|_| R::Snapshot)?)
            .map_err(|_| R::Snapshot)?;
    staged.loading.as_mut().unwrap().begin_step();
    let driver = staged
        .body_handle(before.bodies[0])
        .ok_or(R::UnsupportedScene)?;
    staged.raw.bodies[driver].set_linvel(
        Vector::new(before.normal[0] * speed, before.normal[1] * speed),
        true,
    );
    let previous_dt = staged.raw.integration_parameters.dt;
    staged.raw.integration_parameters.dt = dt;
    // Admission permits only velocity-kinematic/fixed bodies with default
    // collision types. Rapier cannot impart a solver impulse to either body.
    // This drift does not invoke contact capture or the transient impact law.
    staged.raw.step_with_events(&(), &());
    staged.raw.integration_parameters.dt = previous_dt;
    let mut after = Scene::read(&staged)?;
    let mut expected = before.clone();
    expected.x = after.x;
    expected.speed = speed;
    if after != expected {
        return Err(R::Numerical);
    }
    let mut load = continuation.history.advance(&before, after.x, dt, speed)?;
    let state = staged.loading.as_mut().unwrap();
    let mut events = Vec::new();
    if active.owner.is_none() && (before.x >= old_p || after.x >= old_p) {
        if state.episodes.len() >= state.config.max_episodes {
            return Err(R::Limit);
        }
        let next = state.next_episode.checked_add(1).ok_or(R::Limit)?;
        let id = EpisodeId(state.next_episode);
        state.next_episode = next;
        active.owner = Some(id);
        let at = if before.x >= old_p {
            0.0
        } else {
            f64::from(dt) * (old_p - before.x) / (after.x - before.x)
        };
        events.push(CompressionEvent {
            at,
            event: EpisodeEvent {
                id,
                sources: pair,
                transition: EpisodeTransition::Support,
                closing_speed: speed.max(0.0),
            },
        });
        state.episodes.insert(
            pair,
            Episode {
                id,
                absent_seconds: 0.0,
            },
        );
    }
    load.episode = active.owner;
    if after.x < continuation.history.summary.plastic_indentation
        && let Some(id) = active.owner.take()
    {
        let at = if before.x <= old_p {
            0.0
        } else {
            f64::from(dt) * (old_p - before.x) / (after.x - before.x)
        };
        events.push(CompressionEvent {
            at,
            event: EpisodeEvent {
                id,
                sources: pair,
                transition: EpisodeTransition::End,
                closing_speed: 0.0,
            },
        });
        state.episodes.remove(&pair);
    }
    if events.len() + 1 > state.config.max_records_per_step {
        return Err(R::Limit);
    }
    after.material = continuation.history.current_material();
    state
        .response
        .as_mut()
        .unwrap()
        .materials
        .insert(before.sources[1].region, after.material.clone());
    active.scene = after;
    let summary = continuation.history.summary.clone();
    state.compression = Some(continuation);
    if !staged.loading.as_ref().unwrap().valid_for(&staged) {
        return Err(R::Numerical);
    }
    *world = staged;
    Ok(CompressionStep {
        events,
        load: Some(load),
        summary: Some(summary),
    })
}
