//! Exact f64 sums of the selected f32 durations; no temporal error allowance.
use super::Rejection;

// FastTwoSum's residual is exact for finite nonnegative inputs. Refuse when
// f64 cannot retain the sum, even if ordinary equality would hide the loss.
fn exact_sum(a: f64, b: f64) -> Result<f64, Rejection> {
    let sum = a + b;
    let error = if a >= b { (a - sum) + b } else { (b - sum) + a };
    if !sum.is_finite() || error != 0.0 {
        return Err(Rejection::TimePrecision);
    }
    Ok(sum)
}

pub(in crate::world::loading::response::gravity) struct Clock {
    total: f64,
    elapsed: f64,
}

impl Clock {
    pub(in crate::world::loading::response::gravity) fn new(duration: f32) -> Self {
        Self {
            total: f64::from(duration),
            elapsed: 0.0,
        }
    }
    pub(in crate::world::loading::response::gravity) fn elapsed(&self) -> f64 {
        self.elapsed
    }
    pub(in crate::world::loading::response::gravity) fn remaining(&self) -> Result<f64, Rejection> {
        let remaining = self.total - self.elapsed;
        if remaining < 0.0 || exact_sum(self.elapsed, remaining)? != self.total {
            return Err(Rejection::TimePrecision);
        }
        Ok(remaining)
    }
    pub(in crate::world::loading::response::gravity) fn advance(
        &mut self,
        duration: f32,
    ) -> Result<(), Rejection> {
        if !duration.is_finite() || duration <= 0.0 || f64::from(duration) > self.remaining()? {
            return Err(Rejection::TimePrecision);
        }
        let next = exact_sum(self.elapsed, f64::from(duration))?;
        if next <= self.elapsed || next > self.total {
            return Err(Rejection::TimePrecision);
        }
        self.elapsed = next;
        Ok(())
    }
}

pub(in crate::world::loading::response::gravity) fn bounded(
    preferred: f64,
    remaining: f64,
) -> Result<f32, Rejection> {
    let mut duration = preferred as f32;
    if f64::from(duration) > remaining {
        duration = f32::from_bits(duration.to_bits() - 1);
    }
    if !duration.is_finite() || duration <= 0.0 || f64::from(duration) > remaining {
        return Err(Rejection::TimePrecision);
    }
    Ok(duration)
}
