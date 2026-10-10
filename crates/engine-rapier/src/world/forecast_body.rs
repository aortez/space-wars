//! A fresh collision world for a forecast against one prescribed surface.
use super::*;

impl PhysicsWorld {
    /// Copy one fixed/kinematic body and its solid colliders, sharing immutable
    /// geometry. No contacts, joints, other bodies or solver history are copied.
    /// Limits reject the whole request; geometry is never silently truncated.
    /// The returned world's queries are ready without advancing physical time.
    pub fn copy_kinematic_body(
        &self,
        id: BodyId,
        max_colliders: usize,
        max_shape_parts: usize,
    ) -> Option<Self> {
        let body = self.raw.bodies.get(self.body_handle(id)?)?;
        if body.is_dynamic() || !self.entities.get(&id.entity)?.joints.is_empty() {
            return None;
        }
        // Bound inspection as well as copying, including disabled/sensor parts.
        if body.colliders().len() > max_colliders {
            return None;
        }
        let mut remaining = max_shape_parts;
        let mut colliders = Vec::new();
        for handle in body.colliders() {
            let collider = self.raw.colliders.get(*handle)?;
            if collider.is_sensor() || !collider.is_enabled() {
                continue;
            }
            count_parts(collider.shared_shape(), &mut remaining)?;
            colliders.push((decode_collider(collider.user_data)?, collider.clone()));
        }
        if colliders.is_empty() {
            return None;
        }
        let mut copy = Self::new(PhysicsWorldConfig::default());
        copy.raw.gravity = self.raw.gravity;
        copy.raw.integration_parameters = self.raw.integration_parameters;
        copy.collect_events = self.collect_events;
        copy.reserve(1, colliders.len(), 0);
        let handle = copy.raw.insert_body(body.clone());
        copy.bodies.push(BodyEntry { id, handle });
        copy.body_indices.insert(id, 0);
        copy.entities.entry(id.entity).or_default().bodies.push(id);
        for (collider_id, collider) in colliders {
            let child = copy.raw.insert_collider(collider, Some(handle));
            copy.register_collider(collider_id, id, child);
        }
        copy.refresh_mass_properties(id);
        let handles: Vec<_> = copy.colliders.iter().map(|c| c.handle).collect();
        copy.raw.broad_phase.update(
            &copy.raw.integration_parameters,
            &copy.raw.colliders,
            &copy.raw.bodies,
            &handles,
            &[],
            &mut Vec::new(),
        );
        Some(copy)
    }
}

// Terrain uses bounded compounds of cuboids and convex polygons. Reject large
// mesh/heightfield representations instead of treating one mesh as one part.
fn count_parts(shape: &SharedShape, remaining: &mut usize) -> Option<()> {
    *remaining = remaining.checked_sub(1)?;
    if let Some(compound) = shape.as_compound() {
        for (_, child) in compound.shapes() {
            count_parts(child, remaining)?;
        }
    } else if let Some(polygon) = shape.as_convex_polygon() {
        *remaining = remaining.checked_sub(polygon.points().len())?;
    } else if shape.as_ball().is_none() && shape.as_cuboid().is_none() {
        return None;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forecast_body_counts_compound_children_and_rejects_unsupported_shapes() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let id = BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY);
        let child = CompoundChild {
            shape: ColliderShape::Cuboid {
                half_width: 1.0,
                half_height: 1.0,
            },
            position: Vec2::ZERO,
            angle: 0.0,
        };
        let mut collider =
            ColliderSpec::ball(ColliderId::new(id.entity, ColliderRole::PRIMARY, 0), 1.0);
        collider.shape = ColliderShape::Compound {
            children: vec![
                child.clone(),
                CompoundChild {
                    position: Vec2::new(3.0, 0.0),
                    ..child
                },
            ],
        };
        assert!(world.insert_body(
            id,
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &[collider.clone()]
        ));
        assert!(world.copy_kinematic_body(id, 1, 2).is_none());
        assert!(world.copy_kinematic_body(id, 1, 3).is_some());
        world.remove_entity(id.entity);
        collider.shape = ColliderShape::Polyline {
            vertices: vec![Vec2::ZERO, Vec2::X],
        };
        assert!(world.insert_body(
            id,
            BodySpec {
                kind: BodyKind::Fixed,
                ..Default::default()
            },
            &[collider]
        ));
        assert!(world.copy_kinematic_body(id, 1, 100).is_none());
    }

    #[test]
    fn forecast_body_is_bounded_independent_and_queryable_before_a_step() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        let id = BodyId::new(PhysicsId::new(7), BodyRole::PRIMARY);
        let collider = ColliderSpec::cuboid(
            ColliderId::new(id.entity, ColliderRole::PRIMARY, 0),
            3.0,
            1.0,
        );
        assert!(world.insert_body(
            id,
            BodySpec {
                kind: BodyKind::KinematicPosition,
                position: Vec2::new(2.0, 4.0),
                angle: 0.2,
                linear_velocity: Vec2::new(3.0, 1.0),
                angular_velocity: 0.1,
                ..BodySpec::default()
            },
            &[collider]
        ));
        world.step(1.0 / 60.0);
        let before = world.snapshot_bytes().unwrap();
        assert!(world.copy_kinematic_body(id, 0, 10).is_none());
        assert!(world.copy_kinematic_body(id, 10, 0).is_none());
        let mut copy = world.copy_kinematic_body(id, 1, 1).unwrap();
        assert_eq!((copy.body_count(), copy.collider_count()), (1, 1));
        assert_eq!(copy.motion(id), world.motion(id));
        let ray = |w: &PhysicsWorld| {
            w.cast_ray(Vec2::new(2.0, 10.0), -Vec2::Y, RayCastOptions::default())
        };
        assert!(ray(&copy).is_some());
        assert_eq!(ray(&copy), ray(&world));
        copy.set_next_kinematic_pose(id, Vec2::new(9.0, 5.0), 0.3);
        copy.step(1.0 / 60.0);
        assert_ne!(copy.motion(id), world.motion(id));
        assert_eq!(world.snapshot_bytes().unwrap(), before);
    }

    #[test]
    fn forecast_body_does_not_copy_dynamic_bodies_or_other_entities() {
        let mut world = PhysicsWorld::new(PhysicsWorldConfig::default());
        for i in 0..2 {
            let entity = PhysicsId::new(i);
            assert!(world.insert_body(
                BodyId::new(entity, BodyRole::PRIMARY),
                BodySpec {
                    kind: if i == 0 {
                        BodyKind::Dynamic
                    } else {
                        BodyKind::Fixed
                    },
                    ..BodySpec::default()
                },
                &[ColliderSpec::ball(
                    ColliderId::new(entity, ColliderRole::PRIMARY, 0),
                    1.0
                )]
            ));
        }
        assert!(
            world
                .copy_kinematic_body(BodyId::new(PhysicsId::new(0), BodyRole::PRIMARY), 4, 4)
                .is_none()
        );
        let copy = world
            .copy_kinematic_body(BodyId::new(PhysicsId::new(1), BodyRole::PRIMARY), 4, 4)
            .unwrap();
        assert_eq!(copy.body_count(), 1);
        assert!(!copy.contains_entity(PhysicsId::new(0)));
    }
}
