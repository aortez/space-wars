use super::*;

/// A short physical push from a real supporting body. The anchor follows that
/// body's motion; CCD and contact resolution remain in charge of the character.
#[derive(Clone, Copy)]
pub(super) struct GetUpAssist {
    support: BodyId,
    origin_local: Vec2,
    lift: f32,
    remaining: f32,
}

impl SpacelingAssembly {
    pub(super) fn try_get_up(
        &mut self,
        physics: &mut PhysicsWorld,
        snapshot: &SpacelingSnapshot,
        has_gravity: bool,
        severe: bool,
    ) {
        if self.get_up_assist.is_some() {
            return;
        }
        self.get_up_attempts += 1;
        if !has_gravity {
            self.get_up_result = SpacelingGetUpResult::NoGravity;
            return;
        }
        let Some(contact) = snapshot.support else {
            self.get_up_result = SpacelingGetUpResult::NoSupport;
            return;
        };
        if severe
            || contact_relative_velocity(snapshot.motion, contact).length()
                > self.spec.balance.settle_speed * 2.0
            || (snapshot.motion.angular_velocity - contact.angular_velocity).abs()
                > self.spec.balance.settle_angular_speed
        {
            self.get_up_result = SpacelingGetUpResult::Unsettled;
            return;
        }
        let Some(support) = physics.collider_body(contact.collider) else {
            self.get_up_result = SpacelingGetUpResult::NoSupport;
            return;
        };
        let Some(parent) = physics.motion(support) else {
            self.get_up_result = SpacelingGetUpResult::NoSupport;
            return;
        };
        let pose = snapshot.motion;
        let upright_angle = self.up.y.atan2(self.up.x) - std::f32::consts::FRAC_PI_2;
        let vertical_segment = Vec2::Y.rotate_radians(pose.angle).dot(self.up).abs();
        let base_lift = self.spec.half_segment * (1.0 - vertical_segment) + 0.08;
        let max_lift = self.spec.half_height() + 0.15;
        let clearance = physics
            .collider_translation_clearance(self.collider, self.up, max_lift)
            .unwrap_or(0.0);
        let lift = [base_lift, base_lift + 0.15, base_lift + 0.3]
            .into_iter()
            .find(|&lift| {
                lift <= max_lift
                    && lift + 0.02 <= clearance
                    && physics.collider_fits_at(
                        self.collider,
                        pose.position + self.up * lift,
                        upright_angle,
                    ) == Some(true)
            });
        let Some(lift) = lift else {
            self.get_up_result = SpacelingGetUpResult::Blocked;
            return;
        };
        self.set_balance(physics, SpacelingBalance::Recovering);
        self.recovery_support = Some(support);
        self.get_up_assist = Some(GetUpAssist {
            support,
            origin_local: (pose.position - parent.position).rotate_radians(-parent.angle),
            lift,
            remaining: self.spec.balance.recovery_seconds,
        });
        self.get_up_result = SpacelingGetUpResult::Started;
    }

    pub(super) fn drive_get_up(&mut self, physics: &mut PhysicsWorld, gravity: Vec2, dt: f32) {
        let Some(mut assist) = self.get_up_assist else {
            return;
        };
        let Some(parent) = physics.motion(assist.support) else {
            self.set_balance(physics, SpacelingBalance::KnockedDown);
            self.get_up_result = SpacelingGetUpResult::NoSupport;
            return;
        };
        let motion = physics.motion(self.body).expect("retained spaceling");
        let anchor = parent.position + assist.origin_local.rotate_radians(parent.angle);
        let support_velocity = physics
            .velocity_at_point(assist.support, anchor)
            .expect("retained support");
        let remaining_height = assist.lift - (motion.position - anchor).dot(self.up);
        let speed_limit = self.spec.jump_speed * 0.5;
        let desired_speed = (remaining_height * 8.0).clamp(-speed_limit, speed_limit);
        let relative_speed = (motion.linear_velocity - support_velocity).dot(self.up);
        let acceleration = self.spec.ground_acceleration * 2.0;
        let correction =
            (desired_speed - relative_speed).clamp(-acceleration * dt, acceleration * dt);
        // Briefly push against gravity while physically turning. A ballistic hop
        // alone is too short to turn under the stronger gravity inside a planet.
        physics.apply_velocity_delta(
            self.body,
            self.up * (correction - gravity.dot(self.up) * dt),
            true,
        );
        assist.remaining -= dt;
        if assist.remaining > 0.0 {
            self.get_up_assist = Some(assist);
        } else {
            self.get_up_assist = None;
            self.get_up_result = SpacelingGetUpResult::Blocked;
        }
    }
}
