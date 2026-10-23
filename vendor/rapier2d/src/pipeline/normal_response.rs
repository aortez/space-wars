//! Checked, transient paired normal impulses for the batch hook (2D).
use super::SolverContactBatchContext;
use crate::alloc_prelude::*;
use crate::dynamics::{RigidBodyHandle, RigidBodyVelocity};
use crate::geometry::{ColliderHandle, SolverFlags};
use crate::math::Real;

/// One nonnegative normal impulse at an existing solver point. Collider order
/// must match `ContactPair`, and indices are valid only during this callback.
#[derive(Clone, Copy, Debug)]
pub struct ContactImpulse {
    /// Exact collider order from the retained contact pair.
    pub colliders: [ColliderHandle; 2],
    /// Index in that pair's manifold array.
    pub manifold: usize,
    /// Index in that manifold's solver-contact array.
    pub contact: usize,
    /// Nonnegative magnitude, applied away from the opposing body.
    pub impulse: Real,
}

impl SolverContactBatchContext<'_> {
    /// Apply at most 256 paired normal impulses atomically. Only frictionless,
    /// touching contacts (|distance| <= 1e-5) with no tangent drive are admitted.
    /// Rejects duplicate/stale targets, invalid numbers, and batches leaving a
    /// targeted point closing faster than 1e-4. No state changes on rejection.
    ///
    /// The host must account for the complete coupled contact set, constraints
    /// and external work. This method checks application, not energy ownership.
    /// Paired impulses use the existing point and normal and wake dynamic bodies.
    /// Targeted restitution and warmstarts are cleared to prevent applying the
    /// old response again. Contacts remain enabled for ordinary solving.
    /// Injected impulses are not included in Rapier's subsequent force-event
    /// impulse totals: hosts must retain their own response records.
    pub fn apply_contact_impulses(&mut self, requests: &[ContactImpulse]) -> bool {
        if requests.len() > 256 {
            return false;
        }
        let mut staged: Vec<(RigidBodyHandle, RigidBodyVelocity<Real>)> = Vec::new();
        for (i, request) in requests.iter().enumerate() {
            if !request.impulse.is_finite()
                || request.impulse < 0.0
                || requests[..i].iter().any(|r| {
                    r.colliders == request.colliders
                        && r.manifold == request.manifold
                        && r.contact == request.contact
                })
            {
                return false;
            }
            let Some(pair) = self
                .narrow_phase
                .contact_pair(request.colliders[0], request.colliders[1])
            else {
                return false;
            };
            if [pair.collider1, pair.collider2] != request.colliders {
                return false;
            }
            let Some(manifold) = pair.manifolds.get(request.manifold) else {
                return false;
            };
            let Some(contact) = manifold.data.solver_contacts.get(request.contact) else {
                return false;
            };
            if !manifold
                .data
                .solver_flags
                .contains(SolverFlags::COMPUTE_IMPULSES)
                || !manifold.data.normal.is_finite()
                || !contact.point.is_finite()
                || (manifold.data.normal.length_squared() - 1.0).abs() > 1.0e-5
                || !contact.dist.is_finite()
                || contact.dist.abs() > 1.0e-5
                || contact.friction != 0.0
                || contact.tangent_velocity != crate::math::Vector::ZERO
                || manifold
                    .points
                    .get(contact.contact_id[0] as usize)
                    .is_none()
            {
                return false;
            }
            for (collider, sign) in request.colliders.into_iter().zip([-1.0, 1.0]) {
                let Some(collider) = self.colliders.get(collider) else {
                    return false;
                };
                let Some(handle) = collider.parent() else {
                    continue;
                };
                let Some(body) = self.bodies.get(handle) else {
                    return false;
                };
                if !body.is_dynamic() {
                    continue;
                }
                let index = match staged.iter().position(|(h, _)| *h == handle) {
                    Some(index) => index,
                    None => {
                        staged.push((handle, *body.vels()));
                        staged.len() - 1
                    }
                };
                let velocity = &mut staged[index].1;
                let impulse = manifold.data.normal * (sign * request.impulse);
                let arm = contact.point - body.center_of_mass();
                velocity.linvel += impulse * body.mass_properties().effective_inv_mass;
                velocity.angvel += (arm.x * impulse.y - arm.y * impulse.x)
                    * body.mass_properties().effective_world_inv_inertia;
                if !velocity.linvel.is_finite() || !velocity.angvel.is_finite() {
                    return false;
                }
            }
        }
        // Validate the combined response, never intermediate per-contact motion.
        for request in requests {
            let pair = self
                .narrow_phase
                .contact_pair(request.colliders[0], request.colliders[1])
                .unwrap();
            let manifold = &pair.manifolds[request.manifold];
            let contact = &manifold.data.solver_contacts[request.contact];
            let velocities = request.colliders.map(|h| {
                self.colliders[h]
                    .parent()
                    .map(|h| {
                        staged
                            .iter()
                            .find(|(id, _)| *id == h)
                            .map(|(_, v)| v)
                            .unwrap_or(self.bodies[h].vels())
                            .velocity_at_point(contact.point, self.bodies[h].center_of_mass())
                    })
                    .unwrap_or(crate::math::Vector::ZERO)
            });
            let approach = (velocities[0] - velocities[1]).dot(manifold.data.normal);
            if !approach.is_finite() || approach > 1.0e-4 {
                return false;
            }
        }
        for (handle, velocity) in staged {
            self.bodies[handle].set_vels(velocity, true);
            self.bodies[handle].wake_up(true);
        }
        for request in requests {
            let pair = self
                .narrow_phase
                .contact_pair_mut(request.colliders[0], request.colliders[1])
                .unwrap();
            let manifold = &mut pair.manifolds[request.manifold];
            let contact = &mut manifold.data.solver_contacts[request.contact];
            contact.restitution = 0.0;
            contact.warmstart_impulse = 0.0;
            contact.warmstart_tangent_impulse = na::zero();
            contact.warmstart_twist_impulse = 0.0;
            manifold.points[contact.contact_id[0] as usize].data = Default::default();
        }
        true
    }
}
