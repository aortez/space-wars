//! One transaction, a persistent support owner, bounded midpoint/drift solves.
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
    if saved.is_some_and(|c| !matches!(c.status, Status::Active(_)) || !c.valid_for(world)) {
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
    if count + usize::from(saved.is_none()) > loading.config.max_records_per_step {
        return Err(R::Limit);
    }
    let mut staged =
        PhysicsWorld::from_snapshot_bytes(&world.snapshot_bytes().map_err(|_| R::Snapshot)?)
            .map_err(|_| R::Snapshot)?;
    let loading = staged.loading.as_mut().unwrap();
    loading.begin_step();
    let mut events = Vec::new();
    let owner = if let Some(Continuation {
        status: Status::Active(a),
        ..
    }) = saved
    {
        a.owner
    } else {
        if loading.episodes.len() >= loading.config.max_episodes {
            return Err(R::Limit);
        }
        let next = loading.next_episode.checked_add(1).ok_or(R::Limit)?;
        let id = EpisodeId(loading.next_episode);
        loading.next_episode = next;
        let mut pair = scene.plates.sources;
        pair.sort();
        loading.episodes.insert(
            pair,
            Episode {
                id,
                absent_seconds: 0.0,
            },
        );
        events.push(CompressionEvent {
            at: 0.0,
            event: EpisodeEvent {
                id,
                sources: pair,
                transition: EpisodeTransition::Support,
                closing_speed: 0.0,
            },
        });
        id
    };
    let mut current = scene;
    let mut intervals = Vec::with_capacity(count);
    for i in 0..count {
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
        if after != expected || (dx > 0.0 && after.plates.x <= current.plates.x) {
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
        let interval = history.finish(
            &current,
            after.plates.x,
            next,
            h,
            (i as f64) * f64::from(h),
            owner,
        )?;
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
        revision: 1,
        history,
        status: Status::Active(Box::new(Active {
            scene: current,
            owner,
        })),
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
