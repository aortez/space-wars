//! Complete scene admission and the certified exterior of each solid rectangle.
use super::*;
use FlatArrivalRejection as R;
use rapier2d::prelude::ActiveCollisionTypes;

struct Boundary {
    collider: ColliderHandle,
    internal_bottom: bool,
    internal_top: bool,
}

pub(super) struct Scene {
    pub(super) bodies: Vec<RigidBodyHandle>,
    pub(super) normal: [f64; 2],
    pub(super) sources: Vec<ContactSource>,
    tangent: [f64; 2],
    boundaries: Vec<Boundary>,
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

impl Scene {
    pub(super) fn admit(world: &PhysicsWorld) -> Result<Self, R> {
        let loading = world.loading.as_ref().ok_or(R::ResponseDisabled)?;
        let response = loading.response.as_ref().ok_or(R::ResponseDisabled)?;
        let count = world.raw.bodies.len();
        if !(2..=3).contains(&count)
            || !(count..=8 * count).contains(&world.raw.colliders.len())
            || loading.regions.len() != count
            || response.materials.len() != count
            || loading.bindings.len() != world.raw.colliders.len()
            || world.raw.gravity != Vector::ZERO
            || !world.raw.impulse_joints.is_empty()
            || world.raw.multibody_joints.iter().next().is_some()
            || loading.config.touch_tolerance != 1.0e-5
            || loading.episodes.len() > count * (count - 1) / 2
        {
            return Err(R::UnsupportedScene);
        }
        let mut bodies: Vec<_> = world.raw.bodies.iter().map(|(h, _)| h).collect();
        let orientation = world.raw.bodies[bodies[0]].position().rotation;
        let mut dynamic = false;
        for h in &bodies {
            let b = &world.raw.bodies[*h];
            dynamic |= b.is_dynamic();
            if !b.is_enabled()
                || b.is_sleeping()
                || b.is_ccd_enabled()
                || b.soft_ccd_prediction() != 0.0
                || b.dominance_group() != 0
                || b.body_type() == RigidBodyType::KinematicPositionBased
                || b.position().rotation != orientation
                || b.angvel() != 0.0
                || b.user_force() != Vector::ZERO
                || b.user_torque() != 0.0
                || b.linear_damping() != 0.0
                || b.angular_damping() != 0.0
                || (b.is_dynamic() && !b.locked_axes().contains(LockedAxes::ROTATION_LOCKED))
                || (b.is_dynamic()
                    && (b.activation().normalized_linear_threshold >= 0.0
                        || b.activation().angular_threshold >= 0.0))
                || b.locked_axes().intersects(LockedAxes::TRANSLATION_LOCKED)
                || (b.is_fixed() && b.linvel() != Vector::ZERO)
                || !b.linvel().is_finite()
                || !b.translation().is_finite()
                || !b.rotation().angle().is_finite()
                || !(1..=8).contains(&b.colliders().len())
            {
                return Err(R::UnsupportedScene);
            }
        }
        if !dynamic {
            return Err(R::UnsupportedScene);
        }
        let n = orientation * Vector::X;
        let length = libm::hypot(f64::from(n.x), f64::from(n.y));
        if !length.is_finite() || (length - 1.0).abs() > 1.0e-5 {
            return Err(R::UnsupportedScene);
        }
        let normal = [f64::from(n.x) / length, f64::from(n.y) / length];
        bodies.sort_by(|a, b| {
            dot(
                delta(world.raw.bodies[*a].translation(), Vector::ZERO),
                normal,
            )
            .total_cmp(&dot(
                delta(world.raw.bodies[*b].translation(), Vector::ZERO),
                normal,
            ))
        });
        let mut sources = vec![None; count];
        let mut intervals = vec![Vec::new(); count];
        let mut boundaries = Vec::new();
        for (h, c) in world.raw.colliders.iter() {
            let index = bodies
                .iter()
                .position(|h| Some(*h) == c.parent())
                .ok_or(R::UnsupportedScene)?;
            let shape = c.shape().as_cuboid().ok_or(R::UnsupportedScene)?;
            let p = c.position_wrt_parent().ok_or(R::UnsupportedScene)?;
            let id = decode_collider(c.user_data).ok_or(R::UnsupportedScene)?;
            let source = *loading.bindings.get(&id).ok_or(R::UnsupportedScene)?;
            if !c.is_enabled()
                || c.is_sensor()
                || c.friction() != 0.0
                || c.contact_skin() != 0.0
                || !c.active_hooks().is_empty()
                || c.active_collision_types() != ActiveCollisionTypes::default()
                || c.collision_groups() != CollisionGroups::ALL.to_rapier()
                || c.solver_groups() != CollisionGroups::ALL.to_rapier()
                || shape.half_extents.x != 0.5
                || ![0.25, 0.5, 1.0].contains(&shape.half_extents.y)
                || p.translation.x != 0.0
                || !p.translation.y.is_finite()
                || p.rotation.angle() != 0.0
                || world.collider_handle(id) != Some(h)
                || sources[index].is_some_and(|s| s != source)
                || !loading.live(source.region)
                || !response.materials.contains_key(&source.region)
                || world.raw.bodies[bodies[index]].user_data
                    != encode_body(loading.regions[&source.region.id].body)
                || world.body_handle(loading.regions[&source.region.id].body) != Some(bodies[index])
            {
                return Err(R::UnsupportedScene);
            }
            let lo = p.translation.y - shape.half_extents.y;
            let hi = p.translation.y + shape.half_extents.y;
            intervals[index].push([lo, hi]);
            boundaries.push(Boundary {
                collider: h,
                internal_bottom: lo > -1.0,
                internal_top: hi < 1.0,
            });
            sources[index] = Some(source);
        }
        let sources: Vec<_> = sources
            .into_iter()
            .collect::<Option<_>>()
            .ok_or(R::UnsupportedScene)?;
        if sources
            .iter()
            .enumerate()
            .any(|(i, a)| sources[..i].iter().any(|b| a.region == b.region))
            || loading
                .episodes
                .keys()
                .any(|pair| pair.iter().any(|s| !sources.contains(s)))
        {
            return Err(R::UnsupportedScene);
        }
        for mut pieces in intervals {
            // Exact binary dimensions: certify a filled rectangle, not just an
            // AABB. Admit only the tested uniform 1/2/4 slices and 1/2 copies.
            pieces.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            let mut unique = pieces.clone();
            unique.dedup();
            if ![1, 2, 4].contains(&unique.len()) {
                return Err(R::UnsupportedScene);
            }
            let copies = pieces.len() / unique.len();
            if !(1..=2).contains(&copies) || pieces.len() != copies * unique.len() {
                return Err(R::UnsupportedScene);
            }
            let width = 2.0 / unique.len() as f32;
            for (i, piece) in unique.iter().enumerate() {
                if *piece != [-1.0 + i as f32 * width, -1.0 + (i + 1) as f32 * width]
                    || pieces.iter().filter(|p| *p == piece).count() != copies
                {
                    return Err(R::UnsupportedScene);
                }
            }
        }
        let scene = Self {
            bodies,
            normal,
            sources,
            tangent: [-normal[1], normal[0]],
            boundaries,
        };
        let velocity: Vec<_> = scene
            .bodies
            .iter()
            .map(|h| world.raw.bodies[*h].linvel())
            .collect();
        scene.check_geometry(world, &velocity)?;
        Ok(scene)
    }

    pub(super) fn refresh_mass(&self, world: &mut PhysicsWorld) -> Result<(), R> {
        for h in &self.bodies {
            let b = &mut world.raw.bodies[*h];
            b.recompute_mass_properties_from_colliders(&world.raw.colliders);
            // With rotation locked, a finite center-of-mass offset does not
            // change face drift or the active impulse Jacobian. Even symmetric
            // sliced plates can acquire such an offset from f32 mass summation.
            if !b.local_center_of_mass().is_finite()
                || !b.center_of_mass().is_finite()
                || (b.is_dynamic()
                    && (!b.mass().is_finite()
                        || b.mass() <= 0.0
                        || !b.mass_properties().effective_inv_mass.is_finite()
                        || b.mass_properties().effective_inv_mass.min_element() <= 0.0
                        || b.mass_properties().effective_world_inv_inertia != 0.0))
            {
                return Err(R::UnsupportedScene);
            }
        }
        Ok(())
    }

    pub(super) fn positions(&self, world: &PhysicsWorld) -> Vec<Vector> {
        self.bodies
            .iter()
            .map(|h| world.raw.bodies[*h].translation())
            .collect()
    }

    fn check_geometry(&self, world: &PhysicsWorld, velocity: &[Vector]) -> Result<(), R> {
        let positions = self.positions(world);
        if positions.iter().chain(velocity).any(|v| !v.is_finite()) {
            return Err(R::Numerical);
        }
        for i in 0..positions.len() {
            for j in i + 1..positions.len() {
                let d = delta(positions[j], positions[i]);
                let v = delta(velocity[i], velocity[j]);
                let roundoff = 16.0 * f64::from(f32::EPSILON) * (v[0].abs() + v[1].abs() + 1.0);
                if dot(d, self.normal) - 1.0 < -1.0e-5
                    || 2.0 - dot(d, self.tangent).abs() <= 1.0e-5
                    || dot(v, self.tangent).abs() > roundoff
                {
                    return Err(R::Numerical);
                }
            }
        }
        Ok(())
    }

    pub(super) fn next_time(
        &self,
        world: &PhysicsWorld,
        velocity: &[Vector],
    ) -> Result<Option<f64>, R> {
        self.check_geometry(world, velocity)?;
        let positions = self.positions(world);
        let mut next: Option<f64> = None;
        for i in 0..positions.len() {
            for j in i + 1..positions.len() {
                let gap = dot(delta(positions[j], positions[i]), self.normal) - 1.0;
                let approach = dot(delta(velocity[i], velocity[j]), self.normal);
                if approach <= 0.0 {
                    continue;
                }
                if gap <= TOUCH {
                    return Err(R::Unaccounted);
                }
                let time = gap / approach;
                if !time.is_finite() || time <= 0.0 {
                    return Err(R::Numerical);
                }
                next = Some(next.map_or(time, |old| old.min(time)));
            }
        }
        Ok(next)
    }

    pub(super) fn check_drift(
        &self,
        world: &PhysicsWorld,
        start: &[Vector],
        velocity: &[Vector],
        dt: f32,
    ) -> Result<(), R> {
        for (i, p) in self.positions(world).iter().enumerate() {
            for axis in 0..2 {
                let s = f64::from([start[i].x, start[i].y][axis]);
                let movement = f64::from([velocity[i].x, velocity[i].y][axis]) * f64::from(dt);
                let error = 8.0 * f64::from(f32::EPSILON) * (s.abs() + movement.abs() + 1.0);
                if !p.is_finite() || (f64::from([p.x, p.y][axis]) - s - movement).abs() > error {
                    return Err(R::Numerical);
                }
            }
        }
        self.check_geometry(world, velocity)
    }

    pub(super) fn internal_face(&self, context: &ContactModificationContext) -> bool {
        [context.collider1, context.collider2]
            .into_iter()
            .zip([1.0, -1.0])
            .any(|(h, sign)| {
                let c = &context.colliders[h];
                let local = c.rotation().inverse() * (*context.normal * sign);
                if local.x.abs() > 1.0e-6 || (local.y.abs() - 1.0).abs() > 1.0e-6 {
                    return false;
                }
                self.boundaries.iter().any(|b| {
                    b.collider == h
                        && if local.y > 0.0 {
                            b.internal_top
                        } else {
                            b.internal_bottom
                        }
                })
            })
    }
}
