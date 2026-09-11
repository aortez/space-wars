// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use crate::profiling::{self, Scope, Stage};
use i_slint_core::platform::PlatformError;
use i_slint_core::software_renderer::{
    DrawTextureArgs, PhysicalRegion, RenderingRotation, TargetPixel, TexturePixelFormat,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum TextureMode {
    Generic,
    Rgb,
    Compare,
}

impl TextureMode {
    pub(in super::super) fn parse(value: Option<&str>) -> Result<Self, PlatformError> {
        match value {
            // The same-process Picade comparison favors the specialized path.
            // Keep generic and alternating-frame modes for reproducible checks.
            None | Some("rgb") => Ok(Self::Rgb),
            Some("generic") => Ok(Self::Generic),
            Some("compare") => Ok(Self::Compare),
            Some(_) => Err("SPACEWARS_KMS_TEXTURE must be 'generic', 'rgb' or 'compare'".into()),
        }
    }

    pub(in super::super) fn name(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Rgb => "rgb",
            Self::Compare => "compare",
        }
    }
}

/// Only handle exact 1:1 / 2:1 source-to-destination sampling. Slint's generic
/// renderer uses fixed-point nearest-neighbor sampling; integer steps and
/// offsets produce exactly the same pixels without per-pixel tiling arithmetic.
/// Reject everything else before touching the destination.
pub(super) fn draw_rgb<P: TargetPixel>(
    args: &DrawTextureArgs,
    clip: &PhysicalRegion,
    pixels: &mut [P],
    stride: usize,
) -> bool {
    if args.alpha != 255
        || args.colorize.is_some()
        || args.tiling.is_some()
        || args.rotation != RenderingRotation::NoRotation
        || args.dst_width == 0
        || args.dst_height == 0
        || stride == 0
    {
        return false;
    }
    let source = args.source();
    // Slint stores clipped source offsets in unsigned 12.4 fixed point. Limit
    // the source extent so every offset this path handles is representable;
    // otherwise the generic path can reject geometry that we would draw.
    if source.pixel_format != TexturePixelFormat::Rgb
        || source.width > 4096
        || source.height > 4096
        || source.byte_stride % 3 != 0
        || source.byte_stride / 3 > u16::MAX as usize
    {
        return false;
    }
    let step = |src: usize, dst: usize| {
        if src == dst {
            Some(1)
        } else if dst.checked_mul(2) == Some(src) {
            Some(2)
        } else {
            None
        }
    };
    let Some(step_x) = step(source.width as usize, args.dst_width) else { return false };
    let Some(step_y) = step(source.height as usize, args.dst_height) else { return false };

    let Some(row_bytes) = (source.width as usize).checked_mul(3) else { return false };
    let source_end = (source.height as usize - 1)
        .checked_mul(source.byte_stride)
        .and_then(|last_row| last_row.checked_add(row_bytes));
    if source.byte_stride < row_bytes || source_end.is_none_or(|end| end > source.data.len()) {
        return false;
    }
    let Some(right) = args.dst_x.checked_add_unsigned(args.dst_width) else { return false };
    let Some(bottom) = args.dst_y.checked_add_unsigned(args.dst_height) else { return false };
    // Slint's software scene uses i16 geometry.
    // Keep conversions lossless and leave unusually large requests to Slint.
    if [args.dst_x, args.dst_y, right, bottom].iter().any(|&n| i16::try_from(n).is_err()) {
        return false;
    }

    let _blit = Scope::new(Stage::RgbBlit);
    let mut written = 0;
    for (position, size) in clip.iter() {
        let left = i64::from(position.x).max(args.dst_x as i64).max(0);
        let top = i64::from(position.y).max(args.dst_y as i64).max(0);
        let right =
            (i64::from(position.x) + i64::from(size.width)).min(right as i64).min(stride as i64);
        let bottom = (i64::from(position.y) + i64::from(size.height))
            .min(bottom as i64)
            .min((pixels.len() / stride) as i64);
        if left >= right || top >= bottom {
            continue;
        }
        let count = (right - left) as usize;
        let src_x = (left - args.dst_x as i64) as usize * step_x * 3;
        for y in top..bottom {
            let src_y = (y - args.dst_y as i64) as usize * step_y;
            let start = src_y * source.byte_stride + src_x;
            let row = &source.data[start..start + count * step_x * 3];
            let out = &mut pixels
                [y as usize * stride + left as usize..y as usize * stride + right as usize];
            // Constant-size chunks let LLVM vectorize format conversion. No
            // destination reads, intermediate image or unsafe code.
            if step_x == 1 {
                rgb_row::<P, 3>(row, out);
            } else {
                rgb_row::<P, 6>(row, out);
            }
        }
        written += count * (bottom - top) as usize;
    }
    profiling::rgb_blit(written);
    true
}

fn rgb_row<P: TargetPixel, const BYTES: usize>(source: &[u8], output: &mut [P]) {
    for (rgb, pixel) in source.chunks_exact(BYTES).zip(output) {
        *pixel = P::from_rgb(rgb[0], rgb[1], rgb[2]);
    }
}
