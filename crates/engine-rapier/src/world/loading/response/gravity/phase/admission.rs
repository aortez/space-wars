//! Native gravity geometry and identities, separate from episode enrollment.
use super::*;

impl Scene {
    pub(super) fn admit(world: &mut PhysicsWorld) -> Result<(Self, Active), Rejection> {
        // Continuation selects the legacy or bounded-mass contract. Stack enrollment
        // observes three roles separately before establishing its own owners.
        let (scene, plates) = Self::observe::<2>(world)?;
        let observed = Active {
            plates,
            capture: world.loading.as_ref().unwrap().config,
            integration: IntegrationParameters {
                dt: 0.0,
                ..world.raw.integration_parameters
            },
            episode: None,
        };
        Ok((scene, observed))
    }

    // Called only on a disposable world: mass refresh is part of certification.
    // Role order is geometric: free A, optional stationary dynamic B, fixed base.
    pub(in crate::world::loading::response::gravity) fn observe<const N: usize>(
        world: &mut PhysicsWorld,
    ) -> Result<(Self, [Plate; N]), Rejection> {
        let plates = Plates::certify(world).map_err(geometry)?;
        if ![2, 3].contains(&N)
            || plates.bodies.len() != N
            || plates.normal != [1.0, 0.0]
            || !world.raw.gravity.x.is_finite()
            || world.raw.gravity.x <= 0.0
            || world.raw.gravity.y != 0.0
        {
            return Err(Rejection::UnsupportedScene);
        }
        for (i, handle) in plates.bodies.iter().enumerate() {
            let b = &world.raw.bodies[*handle];
            if (i == N - 1 && !b.is_fixed())
                || (i < N - 1 && !b.is_dynamic())
                || b.rotation() != &Pose::IDENTITY.rotation
                || b.translation().y != 0.0
                || b.linvel().y != 0.0
                || b.gravity_scale() != 1.0
                || (i == 0 && f64::from(b.translation().x) > TOUCH)
                || (i > 0
                    && (b.translation() != Vector::X * i as f32 || b.linvel() != Vector::ZERO))
            {
                return Err(Rejection::UnsupportedScene);
            }
        }
        plates.refresh_mass(world).map_err(geometry)?;
        let masses: Vec<_> = plates.bodies[..N - 1]
            .iter()
            .map(|h| world.raw.bodies[*h].mass())
            .collect();
        if !valid_masses(2, &masses) {
            return Err(Rejection::UnsupportedScene);
        }
        let loading = world.loading.as_ref().unwrap();
        let materials = &loading.response.as_ref().unwrap().materials;
        if materials.values().any(|r| r.remaining <= 0.0) {
            return Err(Rejection::FailedRegion);
        }
        let identities = std::array::from_fn(|i| {
            let b = &world.raw.bodies[plates.bodies[i]];
            let source = plates.sources[i];
            let r = &materials[&source.region];
            let first = &world.raw.colliders[b.colliders()[0]];
            // The shared certificate proved exact uniform partitions and copies.
            let slices = (1.0 / first.shape().as_cuboid().unwrap().half_extents.y) as u8;
            let copies = b.colliders().len() as u8 / slices;
            let mut ids: Vec<_> = b
                .colliders()
                .iter()
                .map(|h| decode_collider(world.raw.colliders[*h].user_data).unwrap())
                .collect();
            ids.sort();
            let mut colliders = [None; 8];
            for (slot, id) in colliders.iter_mut().zip(ids) {
                *slot = Some(id);
            }
            Plate {
                body: loading.regions[&source.region.id].body,
                source,
                slices,
                copies,
                colliders,
                material: MaterialIdentity {
                    parameters: r.parameters,
                    brittle: r.brittle,
                    volume: r.volume,
                    thickness: r.thickness,
                    depth: r.depth,
                },
                endpoint: Endpoint::read(b),
                additional_iterations: b.additional_solver_iterations(),
            }
        });
        let integration = IntegrationParameters {
            dt: 0.0,
            ..world.raw.integration_parameters
        };
        if !loading.config.valid()
            || identities.iter().any(|p| !p.endpoint.valid())
            || !valid_integration(&integration, &identities)
        {
            return Err(Rejection::UnsupportedScene);
        }
        Ok((
            Self {
                plates,
                acceleration: f64::from(world.raw.gravity.x),
            },
            identities,
        ))
    }
}

#[cfg(test)]
mod tests;
