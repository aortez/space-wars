//! Piecewise-constant normal gravity with one retained work/momentum history.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct LoadChange {
    pub elapsed: f64,
    pub indentation: f64,
    pub acceleration: f32,
}

impl History {
    pub(super) fn current_acceleration(&self) -> f32 {
        self.loads
            .as_ref()
            .and_then(|loads| loads.last())
            .map_or(self.acceleration, |load| load.acceleration)
    }

    /// Integrate each force over its own displacement and time. Changing force
    /// at a fixed position/time contributes neither work nor impulse.
    pub(super) fn gravity_totals(&self, x: f64, t: f64) -> [f64; 2] {
        if self.loads.is_none() {
            let weight = f64::from(self.mass) * f64::from(self.acceleration);
            return [weight * (x - self.initial_x), weight * t];
        }
        let (mut position, mut time, mut acceleration) = (self.initial_x, 0.0, self.acceleration);
        let mut totals = [0.0; 2];
        for load in self.loads.iter().flatten() {
            let weight = f64::from(self.mass) * f64::from(acceleration);
            totals[0] += weight * (load.indentation - position);
            totals[1] += weight * (load.elapsed - time);
            (position, time, acceleration) = (load.indentation, load.elapsed, load.acceleration);
        }
        let weight = f64::from(self.mass) * f64::from(acceleration);
        totals[0] += weight * (x - position);
        totals[1] += weight * (t - time);
        totals
    }

    pub(super) fn valid_loads(&self) -> bool {
        let Some(loads) = &self.loads else {
            return true;
        };
        if loads.is_empty() || loads.len() > MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES {
            return false;
        }
        let c = MaterialCurve::new(&self.initial);
        let (mut time, mut x, mut acceleration) = (0.0, self.initial_x, self.acceleration);
        for load in loads {
            if !load.elapsed.is_finite()
                || load.elapsed <= 0.0
                || load.elapsed < time
                || load.elapsed > self.summary.elapsed
                || !(-c.depth / 4.0..=c.depth / 4.0).contains(&load.indentation)
                || (load.elapsed == time && load.indentation != x)
                || (load.elapsed == self.summary.elapsed
                    && load.indentation != self.summary.indentation)
                || !self.accepts_acceleration(load.acceleration)
                || load.acceleration == acceleration
            {
                return false;
            }
            (time, x, acceleration) = (load.elapsed, load.indentation, load.acceleration);
        }
        true
    }

    fn accepts_acceleration(&self, acceleration: f32) -> bool {
        acceleration.is_finite()
            && (0.0..=1e6).contains(&acceleration)
            && f64::from(self.mass) * f64::from(acceleration)
                <= 2.0 * MaterialCurve::new(&self.initial).y
    }

    pub(super) fn serialize_loads(&self, bytes: &mut Vec<u8>) -> Result<(), PhysicsWorldError> {
        let loads = self
            .loads
            .as_ref()
            .ok_or_else(|| PhysicsWorldError("missing changing-load history".into()))?;
        bincode::serialize_into(&mut *bytes, &self.incoming)
            .map_err(|e| PhysicsWorldError(e.to_string()))?;
        bincode::serialize_into(&mut *bytes, &(loads.len() as u32))
            .map_err(|e| PhysicsWorldError(e.to_string()))?;
        for load in loads {
            bincode::serialize_into(&mut *bytes, load)
                .map_err(|e| PhysicsWorldError(e.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn deserialize_loads(
        &mut self,
        reader: &mut impl std::io::Read,
    ) -> Result<(), PhysicsWorldError> {
        self.incoming = bincode::deserialize_from(&mut *reader)
            .map_err(|e| PhysicsWorldError(e.to_string()))?;
        let count: u32 = bincode::deserialize_from(&mut *reader)
            .map_err(|e| PhysicsWorldError(e.to_string()))?;
        if count == 0 || count as usize > MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES {
            return Err(PhysicsWorldError("changing-load history limit".into()));
        }
        let mut loads = Vec::with_capacity(count as usize);
        for _ in 0..count {
            loads.push(
                bincode::deserialize_from(&mut *reader)
                    .map_err(|e| PhysicsWorldError(e.to_string()))?,
            );
        }
        self.loads = Some(loads);
        Ok(())
    }
}

impl PhysicsWorld {
    /// Change normal gravity for an already active dynamic-compression history.
    ///
    /// Acceleration is along the admitted contact normal, with 0 <= mass*a <= 2Y
    /// and a <= 1e6. This is a piecewise-constant external force, not an impulse:
    /// pose, velocity, material, ownership, work and error allowances are retained.
    /// Subsequent intervals attribute work and impulse to the new acceleration.
    /// Both unloading and renewed loading use the signed unilateral solve.
    ///
    /// At most 32 actual changes are admitted, including changes while paused.
    /// Repeating the current value is a no-op and preserves legacy snapshot bytes.
    /// An accepted change selects snapshot v18. Invalid/stale/over-limit commands
    /// leave all state and outputs unchanged. Ordinary `set_gravity` still
    /// invalidates continuation; this API cannot enroll or revive a history.
    pub fn set_material_dynamic_compression_acceleration(
        &mut self,
        acceleration: f32,
    ) -> Result<(), compression::CompressionRejection> {
        let saved = self
            .loading
            .as_ref()
            .and_then(|l| l.dynamic_compression.as_ref())
            .ok_or(R::UnsupportedScene)?;
        if saved.active().is_none() || !saved.valid_for(self) {
            return Err(R::StaleContinuation);
        }
        if !saved.history.accepts_acceleration(acceleration) {
            return Err(R::InvalidCommand);
        }
        if acceleration == saved.history.current_acceleration() {
            return Ok(());
        }
        if saved.history.loads.as_ref().map_or(0, Vec::len) == MAX_DYNAMIC_COMPRESSION_LOAD_CHANGES
        {
            return Err(R::Limit);
        }
        let mut next = saved.clone();
        let (scene, owner) = saved.active().unwrap();
        let mut scene = scene.clone();
        scene.acceleration = acceleration;
        let normal = Vector::new(scene.plates.normal[0], scene.plates.normal[1]);
        let pending_support = matches!(
            &saved.status,
            Status::Falling(a) | Status::ChangingLoad(a) if a.pending_support
        );
        next.history
            .loads
            .get_or_insert_with(Vec::new)
            .push(LoadChange {
                elapsed: next.history.summary.elapsed,
                indentation: next.history.summary.indentation,
                acceleration,
            });
        next.revision = 4;
        next.status = Status::ChangingLoad(Box::new(FallingActive {
            scene,
            owner,
            pending_support,
        }));
        if !next.history.valid() {
            return Err(R::Numerical);
        }
        self.raw.gravity = normal * acceleration;
        self.loading.as_mut().unwrap().dynamic_compression = Some(next);
        Ok(())
    }

    /// Integral of normal weight over elapsed time, including all load changes.
    /// Together with reaction impulse and initial momentum this closes the
    /// momentum balance. Retained even after continuation is invalidated.
    pub fn material_dynamic_compression_gravity_impulse(&self) -> Option<f64> {
        let history = &self.loading.as_ref()?.dynamic_compression.as_ref()?.history;
        Some(history.gravity_totals(history.summary.indentation, history.summary.elapsed)[1])
    }
}
