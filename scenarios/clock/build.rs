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
        let mut masks = [0u64; 10];
        for (digit, (m, bitmap)) in glyphs.iter().enumerate() {
            for y in 0..9 {
                for x in 0..6 {
                    let mut coverage = 0u32;
                    let samples = 16;
                    for sy in 0..samples {
                        for sx in 0..samples {
                            let px = left as f32
                                + (x as f32 + (sx as f32 + 0.5) / samples as f32)
                                    * (right - left) as f32
                                    / 6.0;
                            let py = bottom as f32
                                + (y as f32 + (sy as f32 + 0.5) / samples as f32)
                                    * (top - bottom) as f32
                                    / 9.0;
                            let col = px.floor() as i32 - m.xmin;
                            let row = m.height as i32 - 1 - (py.floor() as i32 - m.ymin);
                            if col >= 0 && col < m.width as i32 && row >= 0 && row < m.height as i32
                            {
                                coverage +=
                                    u32::from(bitmap[row as usize * m.width + col as usize]);
                            }
                        }
                    }
                    if coverage as f32 / (samples * samples * 255) as f32 >= 0.28 {
                        masks[digit] |= 1 << (y * 6 + x);
                    }
                }
            }
            assert!(
                masks[digit] != 0 && !masks[..digit].contains(&masks[digit]),
                "distinct sampled digits"
            );
        }
        output.push_str(&format!("const {name}: [u64; 10] = {masks:?};\n"));
    }
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("sampled_fonts.rs"),
        output,
    )
    .unwrap();
}
