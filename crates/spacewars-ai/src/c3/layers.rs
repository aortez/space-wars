use super::encoder::Tensor;

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
pub(super) fn conv<const WIDTH: usize>(
    x: &[f32],
    channels: usize,
    height: usize,
    weight: &Tensor,
    bias: &Tensor,
) -> Vec<f32> {
    let out_height = height.div_ceil(2);
    let out_width = WIDTH / 2;
    let mut out = vec![0.0; 8 * out_height * out_width];
    for (channel, plane) in out.chunks_exact_mut(out_height * out_width).enumerate() {
        let filter = &weight.values[channel * channels * 9..][..channels * 9];
        for (r, sums) in plane.chunks_exact_mut(out_width).enumerate() {
            sums.fill(bias.values[channel]);
            for (ic, kernel) in filter.chunks_exact(9).enumerate() {
                for (kr, weights) in kernel.chunks_exact(3).enumerate() {
                    let Some(y) = (r * 2 + kr).checked_sub(1).filter(|&y| y < height) else {
                        continue;
                    };
                    let row = &x[(ic * height + y) * WIDTH..][..WIDTH];
                    // Adjacent output sectors are independent. Accumulate each
                    // in the exported channel/radial/angular weight order.
                    for (a, sum) in sums.iter_mut().enumerate() {
                        let col = a * 2;
                        *sum += weights[0] * row[(col + WIDTH - 1) % WIDTH];
                        *sum += weights[1] * row[col];
                        *sum += weights[2] * row[col + 1];
                    }
                }
            }
            for sum in sums {
                *sum = sum.tanh();
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
