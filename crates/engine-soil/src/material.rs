use crate::{SoilError, math::Mat};

/// Unit-depth, cohesionless Hencky elasticity with Drucker–Prager yielding.
/// Parameters are experimental world units, not a calibration for real dirt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Material {
    pub density: f32,
    pub young_modulus: f32,
    pub poisson_ratio: f32,
    pub friction_angle_degrees: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            density: 1.0,
            young_modulus: 2000.0,
            poisson_ratio: 0.2,
            friction_angle_degrees: 35.0,
        }
    }
}

impl Material {
    pub(crate) fn validate(self) -> Result<(), SoilError> {
        if !self.density.is_finite()
            || !(0.01..=10000.0).contains(&self.density)
            || !self.young_modulus.is_finite()
            || !(1.0..=1.0e6).contains(&self.young_modulus)
            || !self.poisson_ratio.is_finite()
            || !(0.0..=0.45).contains(&self.poisson_ratio)
            || !self.friction_angle_degrees.is_finite()
            || !(0.0..=60.0).contains(&self.friction_angle_degrees)
        {
            return Err(SoilError::InvalidInput);
        }
        Ok(())
    }

    pub(crate) fn lame(self) -> (f32, f32) {
        let mu = self.young_modulus / (2.0 * (1.0 + self.poisson_ratio));
        let lambda = self.young_modulus * self.poisson_ratio
            / ((1.0 + self.poisson_ratio) * (1.0 - 2.0 * self.poisson_ratio));
        (mu, lambda)
    }

    pub(crate) fn wave_speed(self) -> f32 {
        let (mu, lambda) = self.lame();
        ((lambda + 2.0 * mu) / self.density).sqrt()
    }

    /// Project the logarithmic elastic strain and return Kirchhoff stress.
    /// Klár et al. 2016 §§7.1–7.2: elastic compression is retained; tensile
    /// states go to the cone tip; shear yielding preserves the determinant.
    pub(crate) fn project(
        self,
        trial: Mat,
        log_dilation: &mut f32,
    ) -> Result<(Mat, Mat, bool), SoilError> {
        let (u, sigma, v) = trial.svd().ok_or(SoilError::NumericalFailure)?;
        // Carry stress-free expansion forward. Otherwise separated material
        // immediately acquires pressure on recompression, even while still
        // sparse, and can support artificial hollow piles. Distribute this
        // scalar volume memory isotropically, leaving deviatoric strain intact.
        let mut strain = sigma.map(|s| s.ln() + *log_dilation * 0.5);
        let trace = strain[0] + strain[1];
        let dilation = trace.max(0.0);
        let dev = [(strain[0] - strain[1]) * 0.5, (strain[1] - strain[0]) * 0.5];
        let norm = dev[0].hypot(dev[1]);
        let (mu, lambda) = self.lame();
        let sin_phi = self.friction_angle_degrees.to_radians().sin();
        let alpha = (2.0_f32 / 3.0).sqrt() * 2.0 * sin_phi / (3.0 - sin_phi);
        let delta = norm + (lambda + mu) / mu * trace * alpha;
        let yielded = trace > 0.0 || delta > 0.0;
        // Check the elastic region first: hydrostatic compression has norm=0
        // and must keep its pressure, rather than taking the tension branch.
        if yielded {
            if trace > 0.0 || norm < 1.0e-12 {
                strain = [0.0; 2];
            } else {
                strain = std::array::from_fn(|i| strain[i] - delta * dev[i] / norm);
            }
        }
        let projected = u
            .mul(Mat::diagonal(strain.map(f32::exp)))
            .mul(v.transpose());
        let trace = strain[0] + strain[1];
        let stress = u
            .mul(Mat::diagonal(strain.map(|e| 2.0 * mu * e + lambda * trace)))
            .mul(u.transpose());
        if !projected.finite() || !stress.finite() {
            return Err(SoilError::NumericalFailure);
        }
        *log_dilation = dilation;
        Ok((projected, stress, yielded))
    }
}
