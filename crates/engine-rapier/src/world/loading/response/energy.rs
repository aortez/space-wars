//! Check the actual rounded backend motion before committing material work.
use super::*;
use rapier2d::prelude::{RigidBodySet, RigidBodyVelocity};

pub(super) struct EnergyCheck {
    bodies: Vec<RigidBodyHandle>,
    frame: [f64; 2],
    prescribed_work: f64,
    expected_loss: f64,
    available: f64,
}

impl EnergyCheck {
    pub(super) fn prepare(
        handles: &[RigidBodyHandle],
        motions: &[shared_impact::BodyMotion],
        contacts: &[shared_impact::Contact],
        budget: &shared_impact::Budget,
        recoverable: &[f64],
        coefficients: &[f32],
    ) -> Vec<Self> {
        // Energy belongs to bodies, so merge orthogonal normal-mode components
        // that share a body before measuring its kinetic change. Disconnected
        // sets are checked separately and cannot subsidize each other's errors.
        let mut visited = vec![false; handles.len()];
        let mut checks = Vec::new();
        for first in 0..handles.len() {
            if visited[first] {
                continue;
            }
            let mut group = vec![first];
            visited[first] = true;
            let mut cursor = 0;
            while cursor < group.len() {
                for contact in contacts {
                    if contact.bodies.contains(&group[cursor]) {
                        for i in contact.bodies {
                            if !visited[i] {
                                visited[i] = true;
                                group.push(i);
                            }
                        }
                    }
                }
                cursor += 1;
            }
            // Prefer a prescribed support's frame, where a translating support
            // contributes no transport work. For all-dynamic sets use the first
            // canonical body's incoming translation.
            let reference = group
                .iter()
                .copied()
                .find(|i| motions[*i].inverse_mass == Vec2::ZERO)
                .unwrap_or(first);
            let velocity = motions[reference].linear_velocity;
            let frame = [f64::from(velocity.x), f64::from(velocity.y)];
            let mut check = Self {
                bodies: group.iter().map(|i| handles[*i]).collect(),
                frame,
                prescribed_work: 0.0,
                expected_loss: 0.0,
                available: 0.0,
            };
            let mut components = vec![false; budget.components.len()];
            for allocation in &budget.contacts {
                let contact = &contacts[allocation.id as usize];
                if !group.contains(&contact.bodies[0]) {
                    continue;
                }
                components[allocation.component] = true;
                let impulse = allocation.compression_impulse
                    * (1.0 + f64::from(coefficients[allocation.component]));
                for (index, sign) in contact.bodies.into_iter().zip([1.0, -1.0]) {
                    let m = &motions[index];
                    let nx = f64::from(contact.normal.x) * sign;
                    let ny = f64::from(contact.normal.y) * sign;
                    let jacobian = [
                        nx,
                        ny,
                        (f64::from(contact.point.x) - f64::from(m.center.x)) * ny
                            - (f64::from(contact.point.y) - f64::from(m.center.y)) * nx,
                    ];
                    let velocity = [
                        f64::from(m.linear_velocity.x) - frame[0],
                        f64::from(m.linear_velocity.y) - frame[1],
                        f64::from(m.angular_velocity),
                    ];
                    for (axis, inverse) in [m.inverse_mass.x, m.inverse_mass.y, m.inverse_inertia]
                        .into_iter()
                        .enumerate()
                    {
                        if inverse == 0.0 {
                            check.prescribed_work += impulse * jacobian[axis] * velocity[axis];
                        }
                    }
                }
            }
            for (index, included) in components.into_iter().enumerate() {
                if included {
                    check.available += budget.components[index].available;
                    check.expected_loss += budget.components[index].available - recoverable[index];
                }
            }
            checks.push(check);
        }
        checks
    }

    pub(super) fn accepts(
        &self,
        bodies: &RigidBodySet,
        staged: &[(RigidBodyHandle, RigidBodyVelocity<f32>)],
    ) -> bool {
        let mut realized_loss = self.prescribed_work;
        for handle in &self.bodies {
            let body = &bodies[*handle];
            if !body.is_dynamic() {
                continue;
            }
            let after = staged
                .iter()
                .find(|(h, _)| h == handle)
                .map(|(_, v)| v)
                .unwrap_or(body.vels());
            let before = [
                f64::from(body.linvel().x) - self.frame[0],
                f64::from(body.linvel().y) - self.frame[1],
                f64::from(body.angvel()),
            ];
            let after = [
                f64::from(after.linvel.x) - self.frame[0],
                f64::from(after.linvel.y) - self.frame[1],
                f64::from(after.angvel),
            ];
            let properties = body.mass_properties();
            for (axis, inverse) in [
                properties.effective_inv_mass.x,
                properties.effective_inv_mass.y,
                properties.effective_world_inv_inertia,
            ]
            .into_iter()
            .enumerate()
            {
                if inverse > 0.0 {
                    realized_loss +=
                        0.5 * (before[axis] - after[axis]) * (before[axis] + after[axis])
                            / f64::from(inverse);
                }
            }
        }
        // A common transport velocity must not inflate this tolerance. Subtract
        // the frame in f64 and compare work, not large world-frame KE totals.
        realized_loss.is_finite()
            && self.expected_loss.is_finite()
            && (realized_loss - self.expected_loss).abs() <= 1.0e-5 * self.available
    }
}
