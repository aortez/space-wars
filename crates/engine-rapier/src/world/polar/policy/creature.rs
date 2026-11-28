use super::*;

/// The first sixteen columns are the shared motion block. Distances and speeds
/// in that block use the local sensor scales. Body dimensions and jump height
/// are in body lengths. The four previous outputs follow creature profile order.
pub const CREATURE_SELF_FEATURES: [&str; 30] = [
    "goal_com_x",
    "goal_com_y",
    "target_minus_com_velocity_x",
    "target_minus_com_velocity_y",
    "com_world_velocity_x",
    "com_world_velocity_y",
    "origin_world_velocity_x",
    "origin_world_velocity_y",
    "com_offset_x",
    "com_offset_y",
    "spin",
    "gravity_x",
    "gravity_y",
    "control_frame_velocity_x",
    "control_frame_velocity_y",
    "control_frame_spin",
    "remaining_time",
    "health_fraction",
    "grounded",
    "body_width",
    "body_height",
    "top_speed",
    "jump_height",
    "gravity_magnitude",
    "can_run",
    "can_dive",
    "previous_move",
    "previous_jump",
    "previous_run",
    "previous_dive",
];

/// Goal and applied controls supplied by the controller host at decision time.
#[derive(Debug, Clone, Copy)]
pub struct CreatureContext {
    pub goal: Vec2,
    pub goal_velocity: Vec2,
    pub remaining_seconds: f32,
    pub previous_move: f32,
    pub previous_jump: bool,
    pub previous_run: bool,
    pub previous_dive: bool,
}

/// Body measurements in world units and seconds, including effective gravity.
pub struct CreatureBody {
    pub id: BodyId,
    pub body_length: f32,
    pub width: f32,
    pub height: f32,
    pub top_speed: f32,
    pub jump_height: f32,
    pub gravity: Vec2,
    pub health_fraction: f32,
    pub support: Option<SurfaceContact>,
    pub can_run: bool,
    pub can_dive: bool,
}

#[derive(Debug, PartialEq)]
pub struct CreatureObservation {
    pub self_goal: [f32; CREATURE_SELF_FEATURES.len()],
    pub geometry: PolicyGeometry,
    /// Presence, six kind flags in ObjectKind order, then the eleven object
    /// motion/extent values. Empty rows are zero including their presence.
    pub objects: [[f32; 18]; OBJECT_CAPACITY],
    /// Body associated with each present row, for inspecting a replay.
    pub object_sources: [Option<BodyId>; OBJECT_CAPACITY],
}

impl CreatureBody {
    pub fn observe(
        &self,
        world: &PhysicsWorld,
        sensor: &PolarSensor,
        context: CreatureContext,
        classify: impl Fn(BodyId) -> ObjectKind,
    ) -> Result<CreatureObservation, &'static str> {
        if !(0.0..=1.0).contains(&self.health_fraction)
            || !(-1.0..=1.0).contains(&context.previous_move)
            || ![self.body_length, self.width, self.height]
                .into_iter()
                .all(|v| v.is_finite() && v > 0.)
            || ![self.top_speed, self.jump_height, context.remaining_seconds]
                .into_iter()
                .all(|v| v.is_finite() && v >= 0.)
            || ![self.gravity, context.goal, context.goal_velocity]
                .into_iter()
                .all(finite_vec2)
        {
            return Err("invalid creature measurements or decision context");
        }
        let motion = world.motion(self.id).ok_or("missing creature body")?;
        let center = world
            .center_of_mass(self.id)
            .ok_or("missing creature center of mass")?;
        let observer = Observer {
            origin: motion.position,
            origin_velocity: world
                .velocity_at_point(self.id, motion.position)
                .ok_or("missing creature velocity")?,
            angle: Observer::upright_angle(self.gravity),
            body_length: self.body_length,
        };
        let shared = observer.motion_inputs(PolicyMotion {
            center,
            center_velocity: motion.linear_velocity,
            spin: motion.angular_velocity,
            gravity: self.gravity,
            frame_velocity: self.support.map_or(Vec2::ZERO, |c| c.velocity),
            frame_spin: self.support.map_or(0.0, |c| c.angular_velocity),
            goal: context.goal,
            goal_velocity: context.goal_velocity,
        });
        let mut self_goal = [0.0; CREATURE_SELF_FEATURES.len()];
        self_goal[..shared.len()].copy_from_slice(&shared);
        self_goal[shared.len()..].copy_from_slice(&[
            context.remaining_seconds / TIME_SCALE,
            self.health_fraction,
            f32::from(self.support.is_some()),
            self.width / self.body_length,
            self.height / self.body_length,
            self.top_speed / (SPEED_SCALE * self.body_length),
            self.jump_height / self.body_length,
            self.gravity.length() / (ACCELERATION_SCALE * self.body_length),
            f32::from(self.can_run),
            f32::from(self.can_dive),
            context.previous_move,
            f32::from(context.previous_jump),
            f32::from(self.can_run && context.previous_run),
            f32::from(self.can_dive && context.previous_dive),
        ]);
        let geometry = observer.observe(
            world,
            sensor,
            PolarFilter {
                excluded: &[self.id.entity],
                ..Default::default()
            },
        )?;

        let mut candidates = Vec::new();
        for record in world.motions().filter(|r| r.id.entity != self.id.entity) {
            let Some(radius) = world.body_solid_radius(record.id) else {
                continue;
            };
            let kind = classify(record.id);
            let clearance = (record.motion.position - observer.origin).length() - radius;
            candidates.push((kind, clearance, record, radius));
        }
        candidates.sort_by(|a, b| {
            (a.0 == ObjectKind::OtherSolid)
                .cmp(&(b.0 == ObjectKind::OtherSolid))
                .then_with(|| a.1.total_cmp(&b.1))
                .then(a.2.id.cmp(&b.2.id))
        });
        let support = self.support.and_then(|c| world.collider_body(c.collider));
        let mut objects = [[0.0; 18]; OBJECT_CAPACITY];
        let mut object_sources = [None; OBJECT_CAPACITY];
        for (index, (kind, _, record, radius)) in
            candidates.into_iter().take(OBJECT_CAPACITY).enumerate()
        {
            object_sources[index] = Some(record.id);
            let row = &mut objects[index];
            row[0] = 1.0;
            row[kind as usize] = 1.0;
            row[7..].copy_from_slice(&observer.object_inputs(
                world,
                record,
                ObjectExtent {
                    radius,
                    distance_scale: DISTANCE_SCALE * self.body_length,
                    hazard_radius: 0.,
                    hazard_enabled: false,
                    active_frame: Some(record.id) == support,
                },
            )?);
        }
        let observation = CreatureObservation {
            self_goal,
            geometry,
            objects,
            object_sources,
        };
        if !observation.values().all(f32::is_finite) {
            return Err("non-finite creature observation");
        }
        Ok(observation)
    }
}

impl CreatureObservation {
    pub fn values(&self) -> impl Iterator<Item = f32> + '_ {
        self.self_goal
            .iter()
            .chain(self.geometry.cells.iter().flatten())
            .chain(self.geometry.rays.iter().flatten())
            .chain(&self.geometry.nearest)
            .chain(self.objects.iter().flatten())
            .copied()
    }
}
