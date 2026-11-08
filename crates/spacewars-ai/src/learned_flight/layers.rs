use super::Tensor;

pub(super) fn linear(x: &[f32], weight: &Tensor, bias: &Tensor, activation: bool) -> Vec<f32> {
    weight
        .values
        .chunks_exact(x.len())
        .zip(&bias.values)
        .map(|(row, b)| {
            let v = row.iter().zip(x).fold(*b, |sum, (w, x)| sum + w * x);
            if activation { v.tanh() } else { v }
        })
        .collect()
}

/// Stride two, zero radial padding and circular angular padding. Output is CHW.
pub(super) fn conv(
    x: &[f32],
    channels: usize,
    height: usize,
    width: usize,
    weight: &Tensor,
    bias: &Tensor,
) -> Vec<f32> {
    let oh = height.div_ceil(2);
    let ow = width.div_ceil(2);
    let mut out = vec![0.0; 8 * oh * ow];
    for oc in 0..8 {
        for r in 0..oh {
            for a in 0..ow {
                let mut sum = bias.values[oc];
                for ic in 0..channels {
                    for kr in 0..3 {
                        let y = r as isize * 2 + kr as isize - 1;
                        if y < 0 || y >= height as isize {
                            continue;
                        }
                        for ka in 0..3 {
                            let col = (a * 2 + ka + width - 1) % width;
                            sum += weight.values[((oc * channels + ic) * 3 + kr) * 3 + ka]
                                * x[(ic * height + y as usize) * width + col];
                        }
                    }
                }
                out[(oc * oh + r) * ow + a] = sum.tanh();
            }
        }
    }
    out
}

pub(super) fn pool(x: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(192);
    for c in 0..8 {
        for r in 0..3 {
            for a in 0..8 {
                let i = (c * 6 + r * 2) * 16 + a * 2;
                out.push((x[i] + x[i + 1] + x[i + 16] + x[i + 17]) * 0.25);
            }
        }
    }
    out
}
