//! Fixture-only arrival scheduling for two/three parallel rectangular plates.
//! Construction supplies complete geometry; native shape admission is unchanged.
use super::*;
use rapier2d::prelude::ActiveHooks;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

mod departure;
mod native;
mod tests;

const TOUCH: f64 = 1.0e-6;
const MAX_ADVANCES: usize = 8;

#[derive(Debug, PartialEq)]
enum Refusal {
    InvalidTime,
    Geometry,
    Unaccounted,
    PreviewChanged,
    Response(ResponseRejection),
    Limit { applied: usize },
}

#[derive(Debug, PartialEq)]
struct Trace {
    durations: Vec<f32>,
    responses_at: Vec<f64>,
    seams_removed: usize,
    departures: Vec<departure::Departure>,
}

struct Plates {
    handles: Vec<RigidBodyHandle>,
    normal: [f64; 2],
    tangent: [f64; 2],
}

fn delta(a: Vector, b: Vector) -> [f64; 2] {
    [
        f64::from(a.x) - f64::from(b.x),
        f64::from(a.y) - f64::from(b.y),
    ]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

impl Plates {
    fn new(world: &PhysicsWorld) -> Result<Self, Refusal> {
        let count = world.raw.bodies.len();
        assert!((2..=3).contains(&count));
        let handles: Vec<_> = (1..=count)
            .map(|n| world.body_handle(body(n as u64)).unwrap())
            .collect();
        // Certify the geometry needed to identify internal faces. Each body
        // must be exactly the solid [-.5,.5] x [-1,1] rectangle, represented
        // by contiguous vertical slices and optional coincident duplicates.
        let orientation = world.raw.bodies[handles[0]].position().rotation;
        for h in &handles {
            let b = &world.raw.bodies[*h];
            if b.position().rotation != orientation
                || b.angvel() != 0.0
                || (b.is_dynamic() && !b.is_rotation_locked())
            {
                return Err(Refusal::Geometry);
            }
            let mut intervals = Vec::new();
            for c in b.colliders() {
                let c = &world.raw.colliders[*c];
                let shape = c.shape().as_cuboid().ok_or(Refusal::Geometry)?;
                let p = c.position_wrt_parent().ok_or(Refusal::Geometry)?;
                if shape.half_extents.x != 0.5
                    || p.translation.x != 0.0
                    || p.rotation.angle() != 0.0
                {
                    return Err(Refusal::Geometry);
                }
                intervals.push([
                    p.translation.y - shape.half_extents.y,
                    p.translation.y + shape.half_extents.y,
                ]);
            }
            intervals.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            let mut end = -1.0;
            for [lo, hi] in intervals {
                if !lo.is_finite()
                    || !hi.is_finite()
                    || lo < -1.0
                    || hi > 1.0
                    || hi <= lo
                    || lo > end
                {
                    return Err(Refusal::Geometry);
                }
                end = end.max(hi);
            }
            if end != 1.0 {
                return Err(Refusal::Geometry);
            }
        }
        let n = world.raw.bodies[handles[0]].position().rotation * Vector::X;
        let length = libm::hypot(f64::from(n.x), f64::from(n.y));
        let normal = [f64::from(n.x) / length, f64::from(n.y) / length];
        Ok(Self {
            handles,
            normal,
            tangent: [-normal[1], normal[0]],
        })
    }

    fn positions(&self, world: &PhysicsWorld) -> Vec<Vector> {
        self.handles
            .iter()
            .map(|h| world.raw.bodies[*h].translation())
            .collect()
    }

    fn next_time(&self, world: &PhysicsWorld, speed: &[Vector]) -> Result<Option<f64>, Refusal> {
        let p = self.positions(world);
        if p.iter().chain(speed).any(|v| !v.is_finite()) {
            return Err(Refusal::Geometry);
        }
        let mut next: Option<f64> = None;
        for i in 0..p.len() {
            for j in i + 1..p.len() {
                let d = delta(p[j], p[i]);
                let v = delta(speed[i], speed[j]);
                let gap = dot(d, self.normal) - 1.0;
                let width = 2.0 - dot(d, self.tangent).abs();
                // The experiment covers a finite face overlap and normal
                // relative motion. It does not infer edge/corner arrival.
                let roundoff = 16.0 * f64::from(f32::EPSILON) * (v[0].abs() + v[1].abs() + 1.0);
                if gap < -1.0e-5 || width <= 1.0e-5 || dot(v, self.tangent).abs() > roundoff {
                    return Err(Refusal::Geometry);
                }
                let approach = dot(v, self.normal);
                if approach <= 0.0 {
                    continue;
                }
                if gap <= TOUCH {
                    return Err(Refusal::Unaccounted);
                }
                let time = gap / approach;
                if !time.is_finite() || time <= 0.0 {
                    return Err(Refusal::Geometry);
                }
                next = Some(next.map_or(time, |old| old.min(time)));
            }
        }
        Ok(next)
    }

    fn check_drift(
        &self,
        world: &PhysicsWorld,
        start: &[Vector],
        speed: &[Vector],
        dt: f32,
    ) -> Result<(), Refusal> {
        for (i, p) in self.positions(world).iter().enumerate() {
            for axis in 0..2 {
                let s = f64::from([start[i].x, start[i].y][axis]);
                let movement = f64::from([speed[i].x, speed[i].y][axis]) * f64::from(dt);
                let error = 8.0 * f64::from(f32::EPSILON) * (s.abs() + movement.abs() + 1.0);
                if !p.is_finite() || (f64::from([p.x, p.y][axis]) - s - movement).abs() > error {
                    return Err(Refusal::Geometry);
                }
            }
        }
        // Zero relative motion still runs all overlap/penetration checks.
        self.next_time(world, &vec![Vector::ZERO; self.handles.len()])?;
        Ok(())
    }
}

struct Hooks<'a> {
    loading: Mutex<&'a mut LoadingState>,
    handles: &'a [RigidBodyHandle],
    observed_dt: f32,
    after: Mutex<Option<Vec<Vector>>>,
    invalid: AtomicBool,
    exterior_only: bool,
    seams_removed: AtomicUsize,
}

fn internal_face(context: &ContactModificationContext) -> bool {
    [context.collider1, context.collider2]
        .into_iter()
        .zip([1.0, -1.0])
        .any(|(h, sign)| internal_tangent_face(&context.colliders[h], *context.normal * sign))
}

fn internal_tangent_face(c: &rapier2d::prelude::Collider, outward_normal: Vector) -> bool {
    let local = c.position().rotation.inverse() * outward_normal;
    if local.x.abs() > 1.0e-6 || (local.y.abs() - 1.0).abs() > 1.0e-6 {
        return false;
    }
    let side = c.position_wrt_parent().unwrap().translation.y
        + local.y.signum() * c.shape().as_cuboid().unwrap().half_extents.y;
    // Plates::new proved both adjacent sides are filled by this body's union.
    // Faces at the rectangle's actual top/bottom are never removed.
    side > -1.0 && side < 1.0
}

impl PhysicsHooks for Hooks<'_> {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        if self.exterior_only && internal_face(context) {
            self.seams_removed
                .fetch_add(context.solver_contacts.len(), Ordering::Relaxed);
            context.solver_contacts.clear();
            return;
        }
        context.solver_contacts.retain(|contact| {
            let velocity = [context.rigid_body1, context.rigid_body2].map(|h| {
                h.map_or(Vector::ZERO, |h| {
                    context.bodies[h].velocity_at_point(contact.point)
                })
            });
            let approach = (velocity[0] - velocity[1]).dot(*context.normal);
            if !contact.dist.is_finite() || contact.dist < -1.0e-5 || !approach.is_finite() {
                self.invalid.store(true, Ordering::Relaxed);
            }
            f64::from(contact.dist) <= TOUCH && approach >= 0.0
        });
    }

    fn modify_solver_contacts_batch(&self, context: &mut SolverContactBatchContext) {
        context.dt = self.observed_dt;
        self.loading.lock().unwrap().capture(context, false);
        let mut after = self.after.lock().unwrap();
        if after.is_some() {
            self.invalid.store(true, Ordering::Relaxed);
        }
        *after = Some(
            self.handles
                .iter()
                .map(|h| context.bodies()[*h].linvel())
                .collect(),
        );
    }
}

fn advance(
    world: &mut PhysicsWorld,
    plates: &Plates,
    dt: f32,
    preview: bool,
    exterior_only: bool,
) -> Result<(Vec<Vector>, usize), Refusal> {
    let flags: Vec<_> = world
        .raw
        .colliders
        .iter()
        .map(|(h, c)| (h, c.active_hooks()))
        .collect();
    for (h, f) in &flags {
        world.raw.colliders[*h].set_active_hooks(*f | ActiveHooks::MODIFY_SOLVER_CONTACTS);
    }
    world.raw.integration_parameters.dt = dt;
    let hooks = Hooks {
        loading: Mutex::new(world.loading.as_mut().unwrap()),
        handles: &plates.handles,
        observed_dt: if preview { 0.0 } else { dt },
        after: Mutex::new(None),
        invalid: AtomicBool::new(false),
        exterior_only,
        seams_removed: AtomicUsize::new(0),
    };
    world.raw.step_with_events(&hooks, &());
    let invalid = hooks.invalid.load(Ordering::Relaxed);
    let seams_removed = hooks.seams_removed.load(Ordering::Relaxed);
    let speed = hooks.after.into_inner().unwrap().ok_or(Refusal::Geometry)?;
    for (h, f) in flags {
        world.raw.colliders[h].set_active_hooks(f);
    }
    if invalid || speed.iter().any(|v| !v.is_finite()) {
        return Err(Refusal::Geometry);
    }
    if let Some(reason) = world.material_response_diagnostics().last_rejection {
        return Err(Refusal::Response(reason));
    }
    if !preview
        && plates.handles.iter().enumerate().any(|(i, h)| {
            world.raw.bodies[*h].linvel() != speed[i] || world.raw.bodies[*h].angvel() != 0.0
        })
    {
        return Err(Refusal::Unaccounted);
    }
    Ok((speed, seams_removed))
}

fn step(world: &mut PhysicsWorld, dt: f32, limit: usize) -> Result<Trace, Refusal> {
    step_with_policy(world, dt, limit, true)
}

fn step_with_policy(
    world: &mut PhysicsWorld,
    dt: f32,
    limit: usize,
    exterior_only: bool,
) -> Result<Trace, Refusal> {
    step_tracked(world, dt, limit, exterior_only, None)
}

fn step_tracked(
    world: &mut PhysicsWorld,
    dt: f32,
    limit: usize,
    exterior_only: bool,
    tracker: Option<&mut departure::Tracker>,
) -> Result<Trace, Refusal> {
    if !dt.is_finite() || dt < 0.0 {
        return Err(Refusal::InvalidTime);
    }
    let mut trace = Trace {
        durations: Vec::new(),
        responses_at: Vec::new(),
        seams_removed: 0,
        departures: Vec::new(),
    };
    if dt == 0.0 {
        world.step(0.0);
        return Ok(trace);
    }
    let plates = Plates::new(world)?;
    let mut staged = world.clone();
    let mut staged_tracker = tracker.as_deref().cloned();
    staged.loading.as_mut().unwrap().begin_step();
    let mut elapsed = 0.0;
    while elapsed < f64::from(dt) {
        if trace.durations.len() >= limit.min(MAX_ADVANCES) {
            return Err(Refusal::Limit {
                applied: staged.material_response_diagnostics().applied_batches,
            });
        }
        let remaining = (f64::from(dt) - elapsed) as f32;
        let mut preview = staged.clone();
        let (speed, _) = advance(&mut preview, &plates, remaining, true, exterior_only)?;
        let duration = plates
            .next_time(&staged, &speed)?
            .unwrap_or(f64::from(remaining))
            .min(f64::from(remaining)) as f32;
        if !duration.is_finite() || duration <= 0.0 {
            return Err(Refusal::Geometry);
        }
        let start = plates.positions(&staged);
        let count = staged.material_response_batches().len();
        let (actual, seams_removed) =
            advance(&mut staged, &plates, duration, false, exterior_only)?;
        if actual != speed
            || !staged.material_response_batches()[count..]
                .iter()
                .map(|b| &b.result)
                .eq(preview
                    .material_response_batches()
                    .iter()
                    .map(|b| &b.result))
        {
            return Err(Refusal::PreviewChanged);
        }
        if staged.material_response_batches().len() > count {
            trace.responses_at.push(elapsed);
        }
        plates.check_drift(&staged, &start, &speed, duration)?;
        elapsed += f64::from(duration);
        if let Some(tracker) = &mut staged_tracker {
            trace.departures.extend(departure::retire(
                &mut staged,
                tracker,
                &plates,
                &start,
                &speed,
                duration,
                elapsed,
            )?);
        }
        trace.durations.push(duration);
        trace.seams_removed += seams_removed;
        if duration == remaining {
            break;
        }
    }
    *world = staged;
    if let (Some(tracker), Some(next)) = (tracker, staged_tracker) {
        *tracker = next;
    }
    Ok(trace)
}

fn rotate(p: Vec2, angle: f32) -> Vec2 {
    let (s, c) = (libm::sinf(angle), libm::cosf(angle));
    Vec2::new(c * p.x - s * p.y, s * p.x + c * p.y)
}

fn fixture(
    parts: &[u16],
    copies: u16,
    reverse: bool,
    angle: f32,
    shift: Vec2,
    offset: f32,
    dynamic: bool,
) -> (PhysicsWorld, Vec<RegionToken>) {
    assert!((2..=3).contains(&parts.len()));
    assert!(parts.iter().all(|n| [1, 2, 4].contains(n)) && [1, 2].contains(&copies));
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::ZERO,
        collect_events: false,
        ..Default::default()
    });
    world
        .enable_loading_capture(CaptureConfig::default())
        .unwrap();
    world.enable_material_response().unwrap();
    let mut ids: Vec<_> = (1..=parts.len() as u64).collect();
    if reverse {
        ids.reverse();
    }
    for n in ids {
        let count = parts[(n - 1) as usize];
        let mut specs = Vec::new();
        for part in 0..count {
            for copy in 0..copies {
                let mut spec = ColliderSpec::cuboid(
                    collider(n, part * copies + copy),
                    0.5,
                    1.0 / f32::from(count),
                );
                spec.local_position.y = -1.0 + (f32::from(part) + 0.5) * 2.0 / f32::from(count);
                spec.friction = 0.0;
                spec.mass_properties = Some(ColliderMassProperties {
                    center: Vec2::ZERO,
                    mass: 20.0 / f32::from(count * copies),
                    inertia: 10.0 / f32::from(count * copies),
                });
                specs.push(spec);
            }
        }
        if reverse {
            specs.reverse();
        }
        let position = Vec2::new(
            if n == 1 { -0.02 } else { (n - 1) as f32 },
            if n == 1 { 0.0 } else { offset },
        );
        assert!(world.insert_body(
            body(n),
            BodySpec {
                position: rotate(position, angle),
                angle,
                linear_velocity: shift,
                kind: if n < parts.len() as u64 || dynamic {
                    BodyKind::Dynamic
                } else if shift == Vec2::ZERO {
                    BodyKind::Fixed
                } else {
                    BodyKind::KinematicVelocity
                },
                lock_rotation: true,
                can_sleep: false,
                ..Default::default()
            },
            &specs
        ));
        world.refresh_mass_properties(body(n));
    }
    let mut tokens = Vec::new();
    for n in 1..=parts.len() as u64 {
        let token = world
            .create_loading_region(
                RegionId {
                    entity: PhysicsId::new(n),
                    part: 0,
                },
                body(n),
            )
            .unwrap();
        world
            .register_loading_material(
                token,
                Region::new(if n == parts.len() as u64 { WOOD } else { ARMOR }, 20.0).unwrap(),
                Section::new(1.0, 1.0),
            )
            .unwrap();
        for part in 0..parts[(n - 1) as usize] * copies {
            world
                .bind_loading_collider(
                    collider(n, part),
                    ContactSource {
                        region: token,
                        patch: 0,
                    },
                )
                .unwrap();
        }
        tokens.push(token);
    }
    if parts.len() == 3 {
        step(&mut world, DT, MAX_ADVANCES).unwrap();
        for n in 1..=3 {
            let position = Vec2::new(
                if n == 1 { -0.02 } else { (n - 1) as f32 },
                if n == 1 { 0.0 } else { offset },
            );
            world.set_pose(body(n), rotate(position, angle), angle, true);
        }
    }
    world.set_velocity(body(1), shift + rotate(Vec2::X * 40.0, angle), 0.0, true);
    (world, tokens)
}
