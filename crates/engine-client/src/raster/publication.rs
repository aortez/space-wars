//! Opt-in RGB/RGBA publication experiment. Rasterization stays RGB in both
//! cases. Conversion and any buffer allocation are charged to RasterTimings::image.
//! This changes neither GPU texture lifetime nor the default software path.

use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use slint::{Image, Rgb8Pixel, Rgba8Pixel, SharedPixelBuffer};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum UploadFormat {
    #[default]
    Rgb,
    Rgba,
}

impl UploadFormat {
    pub(crate) fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "rgb" => Ok(Self::Rgb),
            "rgba" => Ok(Self::Rgba),
            _ => Err("raster upload format must be rgb or rgba"),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
        }
    }
}

thread_local! {
    // UI-thread diagnostic state only. Never persisted; restarts restore RGB.
    static REQUESTED: Cell<UploadFormat> = const { Cell::new(UploadFormat::Rgb) };
}

pub(crate) fn requested_format() -> UploadFormat {
    REQUESTED.get()
}

pub(crate) fn request_format(format: UploadFormat) -> Result<(), &'static str> {
    if format == UploadFormat::Rgb
        && super::texture_upload::requested() != super::texture_upload::Mode::Slint
    {
        return Err("select host raster-textures slint before returning to RGB publication");
    }
    #[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
    let gpu_active = i_slint_backend_linuxkms::profiling::overlay_summary()
        .is_some_and(|summary| summary.renderer == "femtovg");
    #[cfg(not(all(target_os = "linux", feature = "pi-kiosk-gpu")))]
    let gpu_active = false;
    require_gpu(gpu_active)?;
    REQUESTED.set(format);
    Ok(())
}

fn require_gpu(gpu_active: bool) -> Result<(), &'static str> {
    if gpu_active {
        Ok(())
    } else {
        Err(
            "raster-upload is experimental and requires the pi-kiosk-gpu build running linuxkms-femtovg",
        )
    }
}

#[derive(Debug, Default)]
pub(super) struct Publisher {
    // Follow the existing RGB pool's slots. Images retained by Slint remain
    // immutable through SharedPixelBuffer's copy-on-write semantics.
    rgba: [Option<SharedPixelBuffer<Rgba8Pixel>>; super::BUFFER_COUNT],
}

/// Wall-clock sub-stages nested in RasterTimings::image, not GPU timings.
/// Counts distinguish pool allocation/resize from SharedPixelBuffer detachment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Timings {
    pub buffers: Duration,
    pub make_mut: Duration,
    pub convert: Duration,
    pub wrap: Duration,
    pub new_buffers: u64,
    pub cow_copies: u64,
    pub alignment_prefix_pixels: u64,
}

impl std::ops::AddAssign for Timings {
    fn add_assign(&mut self, rhs: Self) {
        self.buffers += rhs.buffers;
        self.make_mut += rhs.make_mut;
        self.convert += rhs.convert;
        self.wrap += rhs.wrap;
        self.new_buffers += rhs.new_buffers;
        self.cow_copies += rhs.cow_copies;
        self.alignment_prefix_pixels += rhs.alignment_prefix_pixels;
    }
}

impl Publisher {
    pub(super) fn publish(
        &mut self,
        rgb: &SharedPixelBuffer<Rgb8Pixel>,
        slot: usize,
        format: UploadFormat,
    ) -> (Image, Timings) {
        let mut timings = Timings::default();
        if format == UploadFormat::Rgb {
            let started = Instant::now();
            let image = Image::from_rgb8(rgb.clone());
            timings.wrap = started.elapsed();
            return (image, timings);
        }
        let started = Instant::now();
        let rgba = self.rgba[slot].get_or_insert_with(|| {
            timings.new_buffers += 1;
            SharedPixelBuffer::new(rgb.width(), rgb.height())
        });
        if rgba.width() != rgb.width() || rgba.height() != rgb.height() {
            *rgba = SharedPixelBuffer::new(rgb.width(), rgb.height());
            timings.new_buffers += 1;
        }
        timings.buffers = started.elapsed();

        let before = rgba.as_slice().as_ptr();
        let started = Instant::now();
        let pixels = rgba.make_mut_slice();
        timings.make_mut = started.elapsed();
        // A retained old allocation remains alive during a copy-on-write
        // detach, so a changed data pointer identifies a real copy here.
        timings.cow_copies = u64::from(before != pixels.as_ptr());

        let started = Instant::now();
        timings.alignment_prefix_pixels = expand_rgba(rgb.as_slice(), pixels) as u64;
        timings.convert = started.elapsed();
        let started = Instant::now();
        let image = Image::from_rgba8(rgba.clone());
        timings.wrap = started.elapsed();
        (image, timings)
    }
}

/// Keep the bulk destination aligned for the compiler's vectorized stores.
/// On the Pi 4, an unaligned SharedPixelBuffer destination costs substantially
/// more than the probe's aligned Vec. A short prefix avoids that penalty without
/// changing Slint's allocation layout, adding a scratch copy or using intrinsics.
/// The return value is diagnostic only: how many pixels precede the aligned tail.
pub(crate) fn expand_rgba(rgb: &[Rgb8Pixel], rgba: &mut [Rgba8Pixel]) -> usize {
    assert_eq!(rgb.len(), rgba.len());
    // align_offset counts elements, not bytes. If alignment is impossible it
    // returns usize::MAX; clamping still converts the entire slice correctly.
    let prefix = rgba.as_ptr().align_offset(16).min(rgba.len());
    let (head_rgb, tail_rgb) = rgb.split_at(prefix);
    let (head_rgba, tail_rgba) = rgba.split_at_mut(prefix);
    for (src, dst) in head_rgb.iter().zip(head_rgba) {
        *dst = Rgba8Pixel {
            r: src.r,
            g: src.g,
            b: src.b,
            a: 255,
        };
    }
    for (src, dst) in tail_rgb.iter().zip(tail_rgba) {
        *dst = Rgba8Pixel {
            r: src.r,
            g: src.g,
            b: src.b,
            a: 255,
        };
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_is_exact_for_offsets_small_lengths_and_vector_tails() {
        let source: Vec<_> = (0..320)
            .map(|i| Rgb8Pixel {
                r: (i % 256) as u8,
                g: ((i * 7) % 256) as u8,
                b: ((i * 13) % 256) as u8,
            })
            .collect();
        let sentinel = Rgba8Pixel {
            r: 77,
            g: 88,
            b: 99,
            a: 0,
        };
        let mut destination = vec![sentinel; 320];
        for length in [0, 1, 2, 3, 4, 7, 8, 15, 16, 31, 32, 33, 63, 64, 127, 256] {
            for src_start in 0..16 {
                for dst_start in 0..16 {
                    destination.fill(sentinel);
                    let src = &source[src_start..src_start + length];
                    let dst = &mut destination[dst_start..dst_start + length];
                    let expected_prefix = dst.as_ptr().align_offset(16).min(length);
                    let prefix = expand_rgba(src, dst);
                    assert_eq!(prefix, expected_prefix);
                    if prefix < length {
                        assert_eq!(dst[prefix..].as_ptr() as usize % 16, 0);
                    }
                    for (src, dst) in src.iter().zip(dst.iter()) {
                        assert_eq!((dst.r, dst.g, dst.b, dst.a), (src.r, src.g, src.b, 255));
                    }
                    assert!(destination[..dst_start].iter().all(|p| *p == sentinel));
                    assert!(
                        destination[dst_start + length..]
                            .iter()
                            .all(|p| *p == sentinel)
                    );
                }
            }
        }
    }

    #[test]
    #[should_panic]
    fn conversion_rejects_mismatched_lengths_instead_of_truncating() {
        expand_rgba(&[Rgb8Pixel { r: 1, g: 2, b: 3 }], &mut []);
    }

    #[test]
    fn publication_formats_match_and_rgb_never_allocates_rgba() {
        let mut publisher = Publisher::default();
        let pixels: Vec<_> = (0..=255)
            .map(|r| Rgb8Pixel {
                r,
                g: 255 - r,
                b: r / 2,
            })
            .collect();
        let mut rgb = SharedPixelBuffer::<Rgb8Pixel>::new(16, 16);
        rgb.make_mut_slice().copy_from_slice(&pixels);
        let (unchanged, timings) = publisher.publish(&rgb, 0, UploadFormat::Rgb);
        assert_eq!(unchanged.to_rgb8().unwrap().as_slice(), rgb.as_slice());
        assert!(publisher.rgba.iter().all(Option::is_none));
        assert_eq!(timings.new_buffers, 0);
        assert_eq!(timings.cow_copies, 0);
        assert_eq!(
            timings.buffers + timings.make_mut + timings.convert,
            Duration::ZERO
        );
        for slot in 0..super::super::BUFFER_COUNT {
            let (image, timings) = publisher.publish(&rgb, slot, UploadFormat::Rgba);
            assert_eq!(timings.new_buffers, 1);
            assert_eq!(timings.cow_copies, 0);
            let rgba = image.to_rgba8().unwrap();
            assert_eq!(rgba.width(), 16);
            assert_eq!(rgba.height(), 16);
            for (src, dst) in rgb.as_slice().iter().zip(rgba.as_slice()) {
                assert_eq!((src.r, src.g, src.b, 255), (dst.r, dst.g, dst.b, dst.a));
            }
        }
    }

    #[test]
    fn retained_images_survive_slot_reuse_format_changes_and_resize() {
        let mut publisher = Publisher::default();
        let mut rgb = SharedPixelBuffer::<Rgb8Pixel>::new(5, 3);
        rgb.make_mut_slice().fill(Rgb8Pixel { r: 7, g: 43, b: 99 });
        let (first, _) = publisher.publish(&rgb, 0, UploadFormat::Rgba);
        let original = first.to_rgba8().unwrap();
        rgb.make_mut_slice().fill(Rgb8Pixel { r: 99, g: 43, b: 7 });
        publisher.publish(&rgb, 0, UploadFormat::Rgb);
        let (next, timings) = publisher.publish(&rgb, 0, UploadFormat::Rgba);
        assert_eq!(timings.new_buffers, 0);
        assert_eq!(timings.cow_copies, 1);
        assert_ne!(original.as_slice(), next.to_rgba8().unwrap().as_slice());
        let small = SharedPixelBuffer::<Rgb8Pixel>::new(1, 2);
        let (resized, timings) = publisher.publish(&small, 0, UploadFormat::Rgba);
        assert_eq!(timings.new_buffers, 1);
        assert_eq!(
            timings.cow_copies, 0,
            "resize replaces instead of detaching"
        );
        let resized = resized.to_rgba8().unwrap();
        assert_eq!((resized.width(), resized.height()), (1, 2));
        assert_eq!(first.to_rgba8().unwrap().as_slice(), original.as_slice());
        assert_eq!(
            original.as_slice()[0],
            Rgba8Pixel {
                r: 7,
                g: 43,
                b: 99,
                a: 255
            }
        );
    }

    #[test]
    fn released_slots_are_reused_without_allocating_or_copying() {
        let mut publisher = Publisher::default();
        let rgb = SharedPixelBuffer::<Rgb8Pixel>::new(8, 7);
        let mut previous = None;
        for frame in 0..12 {
            let (image, timings) =
                publisher.publish(&rgb, frame % super::super::BUFFER_COUNT, UploadFormat::Rgba);
            assert_eq!(
                timings.new_buffers,
                u64::from(frame < super::super::BUFFER_COUNT)
            );
            assert_eq!(timings.cow_copies, 0);
            // The UI may keep the previous image until the new one is set.
            previous = Some(image);
        }
        assert!(previous.is_some());
    }

    #[test]
    fn selection_is_strict_gpu_only_and_not_a_persisted_default() {
        assert_eq!(requested_format(), UploadFormat::Rgb);
        assert_eq!(UploadFormat::parse("rgb"), Ok(UploadFormat::Rgb));
        assert_eq!(UploadFormat::parse("rgba"), Ok(UploadFormat::Rgba));
        for invalid in ["", "RGBA", "rgba extra", "bgra"] {
            assert!(UploadFormat::parse(invalid).is_err());
        }
        assert!(require_gpu(false).is_err());
        assert!(require_gpu(true).is_ok());
        assert!(request_format(UploadFormat::Rgba).is_err()); // no active GPU in unit tests
        assert_eq!(requested_format(), UploadFormat::Rgb);
    }
}
