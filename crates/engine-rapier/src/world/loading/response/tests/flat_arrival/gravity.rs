//! Force-integration and support evidence before admitting gravity-driven arrivals.
use super::*;

mod ballistic;
mod tests;

fn fixture(
    parts: &[u16],
    copies: u16,
    reverse: bool,
    gap: f32,
    speed: f32,
    gravity: f32,
) -> (PhysicsWorld, Vec<RegionToken>) {
    // The face normal is +x; rotating this picture makes the fixed plate a floor.
    let (mut world, tokens) = super::fixture(parts, copies, reverse, 0.0, Vec2::ZERO, 0.0, false);
    world.set_pose(body(1), Vec2::X * -gap, 0.0, true);
    world.set_velocity(body(1), Vec2::X * speed, 0.0, true);
    world.set_gravity(Vec2::X * gravity);
    (world, tokens)
}

fn fixture_with_masses(
    plates: &[(u16, f32)],
    copies: u16,
    reverse: bool,
    gap: f32,
    speed: f32,
    gravity: f32,
) -> (PhysicsWorld, Vec<RegionToken>) {
    let (mut world, tokens) =
        super::fixture_with_masses(plates, copies, reverse, 0.0, Vec2::ZERO, 0.0, false);
    world.set_pose(body(1), Vec2::X * -gap, 0.0, true);
    world.set_velocity(body(1), Vec2::X * speed, 0.0, true);
    world.set_gravity(Vec2::X * gravity);
    (world, tokens)
}

// Translate the entire physical scene before creating any gravity witness.
fn translated_fixture(
    plates: &[(u16, f32)],
    copies: u16,
    reverse: bool,
    gap: f32,
    speed: f32,
    gravity: f32,
    offset: Vec2,
) -> (PhysicsWorld, Vec<RegionToken>) {
    let (mut world, tokens) = fixture_with_masses(plates, copies, reverse, gap, speed, gravity);
    for role in 0..plates.len() {
        let id = body(role as u64 + 1);
        let p = world.raw.bodies[world.body_handle(id).unwrap()].translation();
        world.set_pose(id, Vec2::new(p.x + offset.x, p.y + offset.y), 0.0, true);
    }
    (world, tokens)
}

#[derive(Debug, Clone, PartialEq)]
struct Sample {
    duration: f32,
    before_material: f32,
    after_material: f32,
    points: usize,
    incoming_warm: f64,
    material: Vec<ResponseBatch>,
    impulse: f64,
    position: f32,
    velocity: f32,
}

struct Observer<'a> {
    loading: Mutex<&'a mut LoadingState>,
    body: RigidBodyHandle,
    observed: Mutex<Option<Sample>>,
    filter_speculative: bool,
}

impl PhysicsHooks for Observer<'_> {
    fn report_applied_contact_impulses(&self) -> bool {
        true
    }

    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        if internal_face(context) {
            context.solver_contacts.clear();
        } else if self.filter_speculative {
            context
                .solver_contacts
                .retain(|c| f64::from(c.dist) <= TOUCH);
        }
    }

    fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
        let before = context.bodies()[self.body].linvel().x;
        let points = context
            .contact_pairs()
            .map(|p| {
                p.manifolds
                    .iter()
                    .map(|m| m.data.solver_contacts.len())
                    .sum::<usize>()
            })
            .sum();
        let incoming_warm = context
            .contact_pairs()
            .flat_map(|p| &p.manifolds)
            .flat_map(|m| &m.data.solver_contacts)
            .map(|c| f64::from(c.warmstart_impulse))
            .sum();
        self.loading.lock().unwrap().capture(context, true);
        let after = context.bodies()[self.body].linvel().x;
        let sample = Sample {
            duration: context.dt,
            before_material: before,
            after_material: after,
            points,
            incoming_warm,
            material: Vec::new(),
            impulse: 0.0,
            position: 0.0,
            velocity: 0.0,
        };
        assert!(self.observed.lock().unwrap().replace(sample).is_none());
    }
}

fn observe(world: &mut PhysicsWorld, dt: f32, filter_speculative: bool) -> Sample {
    assert!(dt.is_finite() && dt > 0.0);
    Plates::new(world).unwrap();
    let body = world.body_handle(body(1)).unwrap();
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, f) in &flags {
        world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    world.loading.as_mut().unwrap().begin_step();
    world.raw.integration_parameters.dt = dt;
    let observer = Observer {
        loading: Mutex::new(world.loading.as_mut().unwrap()),
        body,
        observed: Mutex::new(None),
        filter_speculative,
    };
    world.raw.step_with_events(&observer, &());
    let mut sample = observer.observed.into_inner().unwrap().unwrap();
    for (h, f) in flags {
        world.raw.colliders[h].set_active_hooks(f);
    }
    sample.material = world.material_response_batches().to_vec();
    sample.impulse = world
        .raw
        .narrow_phase
        .contact_pairs()
        .map(|p| f64::from(p.total_impulse_magnitude()))
        .sum();
    sample.position = world.raw.bodies[body].translation().x;
    sample.velocity = world.raw.bodies[body].linvel().x;
    sample
}

fn replayed(world: &mut PhysicsWorld, dt: f32, filter_speculative: bool) -> Sample {
    let mut replay = world.clone();
    let sample = observe(world, dt, filter_speculative);
    assert_eq!(sample, observe(&mut replay, dt, filter_speculative));
    assert!(
        world.snapshot_bytes().unwrap() == replay.snapshot_bytes().unwrap(),
        "snapshot mismatch"
    );
    assert_eq!(world.loading_batches(), replay.loading_batches());
    assert_eq!(
        world.material_response_batches(),
        replay.material_response_batches()
    );
    sample
}

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual} expected={expected} tolerance={tolerance}"
    );
}

// Free drift only, using N internal semi-implicit solver steps. This models
// the existing backend, not a proposed material-compatible gravity integrator.
fn predicted(start: f64, speed: f64, gravity: f64, dt: f64, n: usize) -> (f64, f64) {
    (
        start + speed * dt + 0.5 * (1.0 + 1.0 / n as f64) * gravity * dt * dt,
        speed + gravity * dt,
    )
}
