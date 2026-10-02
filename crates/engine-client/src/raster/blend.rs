//! Constant-color RGB spans. The Pi's NEON path blends sixteen or eight pixels at once,
//! preserving the scalar renderer's integer rounding and packed RGB output.

use slint::Rgb8Pixel;

pub(super) fn solid_span(pixels: &mut [Rgb8Pixel], source: Rgb8Pixel, alpha: u8) {
    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    let pixels = neon::blend_blocks(pixels, source, alpha);

    // Short spans and incomplete blocks keep the portable path. Other targets
    // use it for the entire span, without changing their generated arithmetic.
    let alpha = u32::from(alpha);
    let inverse = 255 - alpha;
    for pixel in pixels {
        pixel.r = channel(source.r, pixel.r, alpha, inverse);
        pixel.g = channel(source.g, pixel.g, alpha, inverse);
        pixel.b = channel(source.b, pixel.b, alpha, inverse);
    }
}

fn channel(source: u8, destination: u8, alpha: u32, inverse: u32) -> u8 {
    ((u32::from(source) * alpha + u32::from(destination) * inverse + 127) / 255) as u8
}

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
mod neon {
    use super::*;
    use std::arch::aarch64::*;

    // vld3/vst3 require packed, interleaved RGB bytes, with no padding.
    const _: () = {
        assert!(std::mem::size_of::<Rgb8Pixel>() == 3);
        assert!(std::mem::offset_of!(Rgb8Pixel, r) == 0);
        assert!(std::mem::offset_of!(Rgb8Pixel, g) == 1);
        assert!(std::mem::offset_of!(Rgb8Pixel, b) == 2);
    };

    pub(super) fn blend_blocks(
        pixels: &mut [Rgb8Pixel],
        source: Rgb8Pixel,
        alpha: u8,
    ) -> &mut [Rgb8Pixel] {
        let block_pixels = pixels.len() / 16 * 16;
        let (blocks, mut tail) = pixels.split_at_mut(block_pixels);
        // SAFETY: this module is only compiled with NEON enabled. Each chunk
        // contains exactly 16 packed RGB pixels (48 initialized bytes). The
        // optional smaller block contains exactly 8 (24 bytes). Loads/stores
        // support unaligned addresses. Exclusive, disjoint slices cover every
        // store; no access reaches the separate scalar tail.
        unsafe {
            let alpha = u16::from(alpha);
            let inverse = vdup_n_u8((255 - alpha) as u8);
            let red = vdupq_n_u16(u16::from(source.r) * alpha + 128);
            let green = vdupq_n_u16(u16::from(source.g) * alpha + 128);
            let blue = vdupq_n_u16(u16::from(source.b) * alpha + 128);
            for block in blocks.chunks_exact_mut(16) {
                let address = block.as_mut_ptr().cast::<u8>();
                let rgb = vld3q_u8(address);
                vst3q_u8(
                    address,
                    uint8x16x3_t(
                        blend_channel(rgb.0, red, inverse),
                        blend_channel(rgb.1, green, inverse),
                        blend_channel(rgb.2, blue, inverse),
                    ),
                );
            }
            if tail.len() >= 8 {
                let (block, rest) = tail.split_at_mut(8);
                let address = block.as_mut_ptr().cast::<u8>();
                let rgb = vld3_u8(address);
                vst3_u8(
                    address,
                    uint8x8x3_t(
                        blend_half(rgb.0, red, inverse),
                        blend_half(rgb.1, green, inverse),
                        blend_half(rgb.2, blue, inverse),
                    ),
                );
                tail = rest;
            }
        }
        tail
    }

    unsafe fn blend_channel(
        destination: uint8x16_t,
        source_term: uint16x8_t,
        inverse: uint8x8_t,
    ) -> uint8x16_t {
        // SAFETY: only called from the NEON-enabled, bounds-checked block loop.
        unsafe {
            vcombine_u8(
                blend_half(vget_low_u8(destination), source_term, inverse),
                blend_half(vget_high_u8(destination), source_term, inverse),
            )
        }
    }

    unsafe fn blend_half(
        destination: uint8x8_t,
        source_term: uint16x8_t,
        inverse: uint8x8_t,
    ) -> uint8x8_t {
        // SAFETY: only called from the NEON-enabled, bounds-checked block loops.
        unsafe {
            let weighted = vmlal_u8(source_term, destination, inverse);
            // For w = source*a + destination*(255-a), 0 <= w <= 65025:
            // ((w+128) + ((w+128)>>8))>>8 == (w+127)/255, exactly.
            // Both additions fit u16, including the all-white endpoint.
            vshrn_n_u16::<8>(vaddq_u16(weighted, vshrq_n_u16::<8>(weighted)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_rounding_matches_reference_exhaustively() {
        for weighted in 0_u32..=65025 {
            let narrow = (weighted + 128) as u16;
            assert_eq!(
                u32::from((narrow + (narrow >> 8)) >> 8),
                (weighted + 127) / 255
            );
        }
    }

    #[test]
    fn spans_match_scalar_for_every_alpha_and_unaligned_tails() {
        let colors = [
            Rgb8Pixel { r: 0, g: 0, b: 0 },
            Rgb8Pixel {
                r: 255,
                g: 255,
                b: 255,
            },
            Rgb8Pixel {
                r: 3,
                g: 127,
                b: 249,
            },
            Rgb8Pixel {
                r: 255,
                g: 140,
                b: 51,
            },
        ];
        let original: Vec<_> = (0..96)
            .map(|i| Rgb8Pixel {
                r: (i * 19) as u8,
                g: (i * 31) as u8,
                b: (i * 47) as u8,
            })
            .collect();
        for source in colors {
            for alpha in 0_u8..=255 {
                for offset in 0..16 {
                    for length in [0, 1, 7, 8, 9, 15, 16, 17, 23, 24, 25, 31, 32, 33, 64, 79] {
                        let mut actual = original.clone();
                        let mut expected = original.clone();
                        for pixel in &mut expected[offset..offset + length] {
                            let a = u32::from(alpha);
                            pixel.r = channel(source.r, pixel.r, a, 255 - a);
                            pixel.g = channel(source.g, pixel.g, a, 255 - a);
                            pixel.b = channel(source.b, pixel.b, a, 255 - a);
                        }
                        solid_span(&mut actual[offset..offset + length], source, alpha);
                        assert_eq!(
                            actual, expected,
                            "alpha={alpha}, offset={offset}, length={length}"
                        );
                    }
                }
            }
        }
    }
}
