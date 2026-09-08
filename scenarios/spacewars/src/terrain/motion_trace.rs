//! Opt-in motion history for diagnosing short spikes before collision solving.

use std::collections::VecDeque;

use super::*;

const HISTORY_FRAMES: usize = 6 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TerrainMotionStage {
    BeforeEdits,
    AfterEdits,
    BeforeGravity,
    AfterGravity,
    AfterPhysics,
    AfterCollisions,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct TerrainMotionPeaks {
    pub speed: f32,
    pub spin: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerrainMotionBody {
    pub entity: u64,
    pub role: u32,
    pub position: [f32; 2],
    pub angle: f32,
    pub center_of_mass: [f32; 2],
    pub velocity: [f32; 2],
    pub spin: f32,
    pub mass: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerrainMotionFrame {
    /// Number of completed ticks at the start of the observed step.
    pub tick: u64,
    pub stage: TerrainMotionStage,
    pub bodies: Vec<TerrainMotionBody>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerrainMotionAnomaly {
    pub tick: u64,
    pub stage: TerrainMotionStage,
    pub entity: u64,
    pub role: u32,
    pub speed: f32,
    pub non_finite: bool,
    pub history: VecDeque<TerrainMotionFrame>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct MotionTrace {
    history: VecDeque<TerrainMotionFrame>,
    peaks: TerrainMotionPeaks,
    anomaly: Option<TerrainMotionAnomaly>,
    triggered: bool,
}

impl SpacewarsState {
    /// Retain up to one second of six-stage rigid-body history at 60 Hz.
    /// Disabled by default. Capturing does not alter mechanics or observations.
    pub fn enable_terrain_motion_trace(&mut self) {
        self.terrain.motion_trace = Some(MotionTrace::default());
    }

    /// Largest observed speed and absolute spin, including pre-collision motion.
    pub fn terrain_motion_peaks(&self) -> Option<TerrainMotionPeaks> {
        self.terrain.motion_trace.as_ref().map(|trace| trace.peaks)
    }

    /// Take the first anomaly and its preceding history. Recording that history
    /// stops at the anomaly; peak measurements continue through the rest of the run.
    pub fn take_terrain_motion_anomaly(&mut self) -> Option<TerrainMotionAnomaly> {
        self.terrain.motion_trace.as_mut()?.anomaly.take()
    }
}

pub(crate) fn capture_motion(state: &mut SpacewarsState, stage: TerrainMotionStage) -> Duration {
    let Some(trace) = &mut state.terrain.motion_trace else {
        return Duration::ZERO;
    };
    let started = Instant::now();
    let world = &state.physics.world;
    let mut frame = (!trace.triggered).then(|| {
        let mut frame = if trace.history.len() == HISTORY_FRAMES {
            trace.history.pop_front().unwrap()
        } else {
            TerrainMotionFrame {
                tick: state.tick,
                stage,
                bodies: Vec::new(),
            }
        };
        frame.tick = state.tick;
        frame.stage = stage;
        frame.bodies.clear();
        frame
    });
    let mut offender = None;
    // Match the fixed 60 Hz endurance test's world-crossing anomaly threshold.
    // This is a diagnostic trigger, not a gameplay velocity limit.
    let speed_limit = 120.0 * state.config.universe_radius as f32;
    for record in world.motions() {
        let motion = record.motion;
        let speed = motion.linear_velocity.x.hypot(motion.linear_velocity.y);
        trace.peaks.speed = trace.peaks.speed.max(speed);
        trace.peaks.spin = trace.peaks.spin.max(motion.angular_velocity.abs());
        if let Some(frame) = &mut frame {
            let center = world.center_of_mass(record.id).expect("recorded body");
            let body = TerrainMotionBody {
                entity: record.id.entity.value(),
                role: record.id.role.value(),
                position: [motion.position.x, motion.position.y],
                angle: motion.angle,
                center_of_mass: [center.x, center.y],
                velocity: [motion.linear_velocity.x, motion.linear_velocity.y],
                spin: motion.angular_velocity,
                mass: world.body_mass(record.id).expect("recorded body"),
            };
            let non_finite = body
                .position
                .into_iter()
                .chain(body.center_of_mass)
                .chain(body.velocity)
                .chain([body.spin, body.angle, speed])
                .any(|v| !v.is_finite());
            if offender.is_none() && (non_finite || speed > speed_limit) {
                offender = Some((body.entity, body.role, speed, non_finite));
            }
            frame.bodies.push(body);
        }
    }
    if let Some(frame) = frame {
        trace.history.push_back(frame);
    }
    if let Some((entity, role, speed, non_finite)) = offender {
        trace.triggered = true;
        trace.anomaly = Some(TerrainMotionAnomaly {
            tick: state.tick,
            stage,
            entity,
            role,
            speed,
            non_finite,
            history: std::mem::take(&mut trace.history),
        });
    }
    started.elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_preserves_a_transient_spike_with_bounded_history() {
        let mut state = SpacewarsScenario::init_terrain_fixture(42);
        assert!(state.terrain_motion_peaks().is_none());
        let observation = SpacewarsScenario::observe(&state);
        state.enable_terrain_motion_trace();
        for _ in 0..HISTORY_FRAMES + 5 {
            capture_motion(&mut state, TerrainMotionStage::BeforeGravity);
        }
        assert_eq!(
            observation.payload,
            SpacewarsScenario::observe(&state).payload
        );
        assert!(
            state
                .physics
                .apply_velocity_delta(MechanicalEntity::Ship(0), Vec2::X * 60_001.0)
        );
        let body = state
            .physics
            .world
            .motions()
            .find(|body| body.motion.linear_velocity.x > 60_000.0)
            .unwrap()
            .id;
        capture_motion(&mut state, TerrainMotionStage::AfterGravity);
        assert!(
            state
                .physics
                .apply_velocity_delta(MechanicalEntity::Ship(0), Vec2::X * -60_001.0)
        );
        capture_motion(&mut state, TerrainMotionStage::AfterPhysics);
        let anomaly = state.take_terrain_motion_anomaly().unwrap();
        assert_eq!(anomaly.stage, TerrainMotionStage::AfterGravity);
        assert_eq!(anomaly.entity, body.entity.value());
        assert_eq!(anomaly.history.len(), HISTORY_FRAMES);
        assert!(!anomaly.non_finite);
        assert_eq!(
            anomaly.history.back().unwrap().stage,
            TerrainMotionStage::AfterGravity
        );
        assert_eq!(state.terrain_motion_peaks().unwrap().speed, 60_001.0);
        assert!(state.take_terrain_motion_anomaly().is_none());
    }
}
