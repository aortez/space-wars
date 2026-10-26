//! Whole-scene certificate; role order is platen then support.
use super::*;
use rapier2d::prelude::ActiveCollisionTypes;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(in crate::world::loading::response) struct Scene {
    pub(in crate::world::loading::response) bodies: [BodyId; 2],
    pub(in crate::world::loading::response) sources: [ContactSource; 2],
    colliders: [Vec<ColliderId>; 2],
    slices: [usize; 2],
    copies: [usize; 2],
    pub(in crate::world::loading::response) normal: [f32; 2],
    origin: [f32; 2],
    pub(in crate::world::loading::response) x: f64,
    pub(in crate::world::loading::response) speed: f32,
    pub(in crate::world::loading::response) material: StoredRegion,
    capture: CaptureConfig,
    integration: IntegrationParameters,
}
impl Scene {
    pub(in crate::world::loading::response) fn read(world: &PhysicsWorld) -> Result<Self, R> {
        Self::read_mode(world, false)
    }

    pub(in crate::world::loading::response) fn read_dynamic(
        world: &PhysicsWorld,
    ) -> Result<Self, R> {
        Self::read_mode(world, true)
    }

    fn read_mode(world: &PhysicsWorld, dynamic: bool) -> Result<Self, R> {
        let loading = world.loading.as_ref().ok_or(R::ResponseDisabled)?;
        let response = loading.response.as_ref().ok_or(R::ResponseDisabled)?;
        if world.raw.bodies.len() != 2
            || world.raw.colliders.len() > 16
            || (!dynamic && world.raw.gravity != Vector::ZERO)
            || (!dynamic && loading.dynamic_compression.is_some())
            || (dynamic && loading.compression.is_some())
            || !world.raw.impulse_joints.is_empty()
            || world.raw.multibody_joints.iter().next().is_some()
            || loading.regions.len() != 2
            || response.materials.len() != 1
            || loading.bindings.len() != world.raw.colliders.len()
            || loading.gravity.is_some()
            || loading.arrival_departures.is_some()
            || loading.flat_departures.is_some()
        {
            return Err(R::UnsupportedScene);
        }
        let driver = world
            .raw
            .bodies
            .iter()
            .find(|(_, b)| {
                b.body_type()
                    == if dynamic {
                        RigidBodyType::Dynamic
                    } else {
                        RigidBodyType::KinematicVelocityBased
                    }
            })
            .map(|(h, _)| h)
            .ok_or(R::UnsupportedScene)?;
        let fixed = world
            .raw
            .bodies
            .iter()
            .find(|(_, b)| b.is_fixed())
            .map(|(h, _)| h)
            .ok_or(R::UnsupportedScene)?;
        let handles = [driver, fixed];
        let base = &world.raw.bodies[fixed];
        let turn = QuarterTurn::from_angle(base.rotation().angle()).ok_or(R::UnsupportedScene)?;
        if *base.rotation() != turn.rotation() {
            return Err(R::UnsupportedScene);
        }
        let n = turn.rotate(Vec2::X);
        let origin = [base.translation().x - n.x, base.translation().y - n.y];
        for axis in 0..2 {
            let center = [base.translation().x, base.translation().y][axis];
            if f64::from(origin[axis]) != f64::from(center) - f64::from([n.x, n.y][axis])
                || !(-16.0..=16.0).contains(&origin[axis])
            {
                return Err(R::UnsupportedScene);
            }
        }
        let mut ids = Vec::new();
        let mut sources = Vec::new();
        let mut colliders = [Vec::new(), Vec::new()];
        let mut slices = [0; 2];
        let mut copies = [0; 2];
        for (role, h) in handles.into_iter().enumerate() {
            let b = &world.raw.bodies[h];
            let id = world
                .bodies
                .iter()
                .find(|entry| entry.handle == h)
                .map(|entry| entry.id)
                .ok_or(R::UnsupportedScene)?;
            if b.user_data != encode_body(id)
                || world.body_handle(id) != Some(h)
                || !b.is_enabled()
                || (b.is_sleeping() && b.linvel() != Vector::ZERO)
                || *b.rotation() != turn.rotation()
                || *b.next_position() != *b.position()
                || b.angvel() != 0.0
                || !b.linvel().is_finite()
                || !b.translation().is_finite()
                || b.user_force() != Vector::ZERO
                || b.user_torque() != 0.0
                || b.linear_damping() != 0.0
                || b.angular_damping() != 0.0
                || b.is_ccd_enabled()
                || b.soft_ccd_prediction() != 0.0
                || b.dominance_group() != 0
                || b.locked_axes().intersects(LockedAxes::TRANSLATION_LOCKED)
                || b.additional_solver_iterations() != 0
                || (b.is_dynamic()
                    && (b.is_sleeping()
                        || !b.locked_axes().contains(LockedAxes::ROTATION_LOCKED)
                        || b.gravity_scale() != 1.0
                        || b.activation().normalized_linear_threshold >= 0.0
                        || b.activation().angular_threshold >= 0.0))
                || !(1..=8).contains(&b.colliders().len())
                || -n.y * b.linvel().x + n.x * b.linvel().y != 0.0
                || (role == 1 && b.linvel() != Vector::ZERO)
            {
                return Err(R::UnsupportedScene);
            }
            let tangent = -f64::from(n.y) * (f64::from(b.translation().x) - f64::from(origin[0]))
                + f64::from(n.x) * (f64::from(b.translation().y) - f64::from(origin[1]));
            if tangent != 0.0 {
                return Err(R::UnsupportedScene);
            }
            let mut source = None;
            let mut pieces = Vec::new();
            for ch in b.colliders() {
                let c = &world.raw.colliders[*ch];
                let cid = decode_collider(c.user_data).ok_or(R::UnsupportedScene)?;
                let s = *loading.bindings.get(&cid).ok_or(R::UnsupportedScene)?;
                let shape = c.shape().as_cuboid().ok_or(R::UnsupportedScene)?;
                let pose = c.position_wrt_parent().ok_or(R::UnsupportedScene)?;
                if world.collider_handle(cid) != Some(*ch)
                    || !c.is_enabled()
                    || c.is_sensor()
                    || c.friction() != 0.0
                    || c.restitution() != 0.0
                    || c.contact_skin() != 0.0
                    || !c.active_hooks().is_empty()
                    || c.active_collision_types() != ActiveCollisionTypes::default()
                    || c.collision_groups() != CollisionGroups::ALL.to_rapier()
                    || c.solver_groups() != CollisionGroups::ALL.to_rapier()
                    || shape.half_extents.x != 0.5
                    || ![0.25, 0.5, 1.0].contains(&shape.half_extents.y)
                    || pose.translation.x != 0.0
                    || !pose.translation.y.is_finite()
                    || pose.rotation != Pose::IDENTITY.rotation
                    || source.is_some_and(|other| other != s)
                    || !loading.live(s.region)
                    || loading.regions[&s.region.id].body != id
                    || response.materials.contains_key(&s.region) != (role == 1)
                {
                    return Err(R::UnsupportedScene);
                }
                source = Some(s);
                colliders[role].push(cid);
                pieces.push([
                    pose.translation.y - shape.half_extents.y,
                    pose.translation.y + shape.half_extents.y,
                ]);
            }
            colliders[role].sort();
            pieces.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
            let mut unique = pieces.clone();
            unique.dedup();
            slices[role] = unique.len();
            copies[role] = pieces.len() / unique.len();
            if ![1, 2, 4].contains(&slices[role])
                || ![1, 2].contains(&copies[role])
                || pieces.len() != slices[role] * copies[role]
            {
                return Err(R::UnsupportedScene);
            }
            let width = 2.0 / slices[role] as f32;
            for (i, piece) in unique.iter().enumerate() {
                if *piece != [-1.0 + i as f32 * width, -1.0 + (i + 1) as f32 * width]
                    || pieces.iter().filter(|p| *p == piece).count() != copies[role]
                {
                    return Err(R::UnsupportedScene);
                }
            }
            ids.push(id);
            sources.push(source.ok_or(R::UnsupportedScene)?);
        }
        if colliders.iter().map(Vec::len).sum::<usize>() != world.raw.colliders.len() {
            return Err(R::UnsupportedScene);
        }
        let b = &world.raw.bodies[driver];
        let x = f64::from(n.x) * (f64::from(b.translation().x) - f64::from(origin[0]))
            + f64::from(n.y) * (f64::from(b.translation().y) - f64::from(origin[1]));
        let mut integration = world.raw.integration_parameters;
        integration.dt = 0.0;
        Ok(Self {
            bodies: [ids[0], ids[1]],
            sources: [sources[0], sources[1]],
            colliders,
            slices,
            copies,
            normal: [n.x, n.y],
            origin,
            x,
            speed: n.x * b.linvel().x + n.y * b.linvel().y,
            material: response.materials[&sources[1].region].clone(),
            capture: loading.config,
            integration,
        })
    }
}

#[cfg(test)]
pub(super) fn frozen_v14_scene(s: &Scene) -> Vec<u8> {
    let m = &s.material;
    bincode::serialize(&(
        s.bodies,
        s.sources,
        &s.colliders,
        s.slices,
        s.copies,
        s.normal,
        s.origin,
        s.x,
        s.speed,
        (
            m.parameters,
            m.brittle,
            m.volume,
            m.remaining,
            m.thickness,
            m.depth,
        ),
        s.capture,
        s.integration,
    ))
    .unwrap()
}
