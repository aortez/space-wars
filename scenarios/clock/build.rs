//! Sample the bundled outline fonts once at build time, on every target.
//! Runtime rendering, water and physics all consume the same tiny cell masks.
use std::{env, fs, path::PathBuf};

fn main() {
    let mut output = String::new();
    for (name, file) in [("SANS", "DejaVuSans.ttf"), ("SERIF", "DejaVuSerif.ttf")] {
        let path = format!("fonts/{file}");
        println!("cargo:rerun-if-changed={path}");
        let font =
            fontdue::Font::from_bytes(fs::read(path).unwrap(), fontdue::FontSettings::default())
                .unwrap();
        let glyphs: Vec<_> = ('0'..='9').map(|c| font.rasterize(c, 128.0)).collect();
        // The face's dash for an unknown time is the font's own hyphen.
        let (mut dash_metrics, dash_bitmap) = font.rasterize('-', 128.0);
        let left = glyphs.iter().map(|(m, _)| m.xmin).min().unwrap();
        let right = glyphs
            .iter()
            .map(|(m, _)| m.xmin + m.width as i32)
            .max()
            .unwrap();
        let bottom = glyphs.iter().map(|(m, _)| m.ymin).min().unwrap();
        let top = glyphs
            .iter()
            .map(|(m, _)| m.ymin + m.height as i32)
            .max()
            .unwrap();
        let frame = Frame {
            left,
            right,
            bottom,
            top,
        };
        let mut masks = [0u64; 10];
        for (digit, (m, bitmap)) in glyphs.iter().enumerate() {
            masks[digit] = frame.sample(m, bitmap);
            assert!(
                masks[digit] != 0 && !masks[..digit].contains(&masks[digit]),
                "distinct sampled digits"
            );
        }
        // A proportional hyphen sits low and to the left of the tabular digits,
        // so it is centred in their box first, where the colon's middle is.
        dash_metrics.xmin = (left + right - dash_metrics.width as i32) / 2;
        dash_metrics.ymin = (bottom + top - dash_metrics.height as i32) / 2;
        // Half a pixel of rounding can drop one antialiased end of it, so the
        // dash is made symmetric about the slot's middle, as a centred dash is.
        let dash_mask = symmetric(frame.sample(&dash_metrics, &dash_bitmap));
        assert!(
            dash_mask != 0 && !masks.contains(&dash_mask),
            "a dash distinct from every digit"
        );
        output.push_str(&format!("const {name}: [u64; 10] = {masks:?};\n"));
        output.push_str(&format!("const {name}_DASH: u64 = {dash_mask:?};\n"));
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("sampled_fonts.rs"),
        output,
    )
    .unwrap();
}

/// `mask` together with its mirror image across the slot's middle column.
fn symmetric(mask: u64) -> u64 {
    (0..54)
        .filter(|bit| mask & (1 << bit) != 0)
        .fold(mask, |mirrored, bit| {
            mirrored | (1 << (bit / 6 * 6 + 5 - bit % 6))
        })
}

/// The box every glyph of one font is sampled in: the union of its digits.
struct Frame {
    left: i32,
    right: i32,
    bottom: i32,
    top: i32,
}

impl Frame {
    /// The 6x9 cells a glyph covers by at least 28%.
    fn sample(&self, m: &fontdue::Metrics, bitmap: &[u8]) -> u64 {
        let mut mask = 0;
        for y in 0..9 {
            for x in 0..6 {
                let mut coverage = 0u32;
                let samples = 16;
                for sy in 0..samples {
                    for sx in 0..samples {
                        let px = self.left as f32
                            + (x as f32 + (sx as f32 + 0.5) / samples as f32)
                                * (self.right - self.left) as f32
                                / 6.0;
                        let py = self.bottom as f32
                            + (y as f32 + (sy as f32 + 0.5) / samples as f32)
                                * (self.top - self.bottom) as f32
                                / 9.0;
                        let col = px.floor() as i32 - m.xmin;
                        let row = m.height as i32 - 1 - (py.floor() as i32 - m.ymin);
                        if col >= 0 && col < m.width as i32 && row >= 0 && row < m.height as i32 {
                            coverage += u32::from(bitmap[row as usize * m.width + col as usize]);
                        }
                    }
                }
                if coverage as f32 / (samples * samples * 255) as f32 >= 0.28 {
                    mask |= 1 << (y * 6 + x);
                }
            }
        }
        mask
    }
}
