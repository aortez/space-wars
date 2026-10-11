//! Renderer-only scaling lab. Frozen production inputs and synthetic overdraw,
//! excluding simulation, text/UI, swaps and scanout. GPU work must complete.
mod fixtures;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
mod gpu;
mod reference;
// Use the same existing coarse quality gate for independent raster references
// and mode parity. Different tessellations are not pixel-identical at edges.
const RGB_MAE_LIMIT: f64 = 6.0;
const DIFFERENT_PIXEL_LIMIT: f64 = 0.20;
#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(super) use gpu::run;

fn percentile(samples: &[f64], percentile: usize) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[(sorted.len() * percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(sorted.len() - 1)]
}

/// Compare colors without demanding byte-identical antialiasing. This is a
/// gross-regression gate, not a substitute for inspecting the paired captures.
fn difference(reference: &[u8], rgba: &[u8]) -> Result<(f64, f64), String> {
    if !reference.len().is_multiple_of(3) || rgba.len() != reference.len() / 3 * 4 {
        return Err("geometry readback dimensions differ".into());
    }
    let mut absolute = 0u64;
    let mut different = 0usize;
    for (rgb, pixel) in reference.chunks_exact(3).zip(rgba.chunks_exact(4)) {
        if pixel[3] != 255 {
            return Err("geometry target is not opaque".into());
        }
        let error: u64 = rgb
            .iter()
            .zip(pixel)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        absolute += error;
        different += usize::from(error > 30);
    }
    let mae = absolute as f64 / reference.len().max(1) as f64;
    let fraction = different as f64 / (rgba.len() / 4).max(1) as f64;
    Ok((mae, fraction))
}

fn compare(reference: &[u8], rgba: &[u8]) -> Result<(f64, f64), String> {
    let (mae, fraction) = difference(reference, rgba)?;
    // A missing scene or wrong orientation must not count as a fast result.
    if mae > RGB_MAE_LIMIT || fraction > DIFFERENT_PIXEL_LIMIT {
        return Err(format!(
            "geometry/raster comparison failed: RGB MAE={mae:.3}, pixel error>30 fraction={fraction:.3}"
        ));
    }
    Ok((mae, fraction))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantiles_use_nearest_rank_and_keep_input_order() {
        let v = vec![4.0, 1.0, 3.0, 2.0];
        assert_eq!(percentile(&v, 50), 2.0);
        assert_eq!(percentile(&v, 95), 4.0);
        assert_eq!(percentile(&[], 95), 0.0);
        assert_eq!(v[0], 4.0);
    }
    #[test]
    fn image_gate_rejects_blank_stale_wrong_size_and_alpha() {
        assert_eq!(
            compare(&[40, 50, 60], &[40, 50, 60, 255]).unwrap(),
            (0.0, 0.0)
        );
        assert!(compare(&[40, 50, 60], &[0, 0, 0, 255]).is_err());
        assert!(compare(&[40, 50, 60], &[40, 50, 60, 0]).is_err());
        assert!(compare(&[40, 50, 60], &[40, 50, 60]).is_err());
    }
}
