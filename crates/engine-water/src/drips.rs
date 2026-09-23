//! Optional temporal batching for small outfalls. Credits are requests, not
//! extra liquid: all waiting water remains in the donor pool's columns.
use crate::{SUBSTEP, WaterError, WaterWorld};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DripConfig {
    /// Unit-depth area requested before releasing a useful drop.
    pub target_volume: f64,
    /// Release even a smaller remainder after this many simulated seconds.
    /// Capacity backpressure can delay it further, without discarding water.
    pub max_delay: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Credit {
    pub amount: f64,
    pub age: f64,
}

impl Credit {
    pub fn request(
        &mut self,
        config: DripConfig,
        dt: f64,
        flow: f64,
        available: f64,
    ) -> (f64, bool) {
        if flow <= 0.0 || available <= 0.0 {
            *self = Self::default();
            return (0.0, false);
        }
        let waiting = self.amount > 0.0;
        // Interior fluxes/other outlets may spend the same donor. Credits may
        // never reserve phantom liquid, nor accumulate an unbounded time debt.
        self.amount = (self.amount + flow).min(available);
        self.age = (self.age + dt).min(config.max_delay);
        if self.amount >= config.target_volume || self.age >= config.max_delay {
            (self.amount, waiting || flow < config.target_volume)
        } else {
            (0.0, false)
        }
    }
}

impl WaterWorld {
    /// Configure both outlets of one preallocated pool. `None` is the default:
    /// continuous outflow with no timing batching. Only small flows wait; a single
    /// substep's flow >= target_volume can still form connected ribbons.
    /// This approximates outflow timing, not physical surface tension/pressure.
    pub fn set_drip_config(
        &mut self,
        pool: usize,
        config: Option<DripConfig>,
    ) -> Result<(), WaterError> {
        if config.is_some_and(|c| {
            !c.target_volume.is_finite()
                || !(0.001..=1e6).contains(&c.target_volume)
                || !c.max_delay.is_finite()
                || !(SUBSTEP..=2.0).contains(&c.max_delay)
        }) {
            return Err(WaterError::InvalidInput);
        }
        let pool = self.pools.get_mut(pool).ok_or(WaterError::InvalidInput)?;
        if pool.drip_config != config {
            pool.drip_config = config;
            pool.outlet_credit = [Credit::default(); 2];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
