use super::{Error, Pass, Plan, save_png};
use crate::gpu_scene::small_strokes;
use std::{fs::File, io::Write, path::Path};

pub(super) struct Report<'a> {
    output: Option<&'a Path>,
    candidate: &'static str,
    csv: Option<File>,
    previous: Option<(u32, u32, Vec<i16>)>,
    pub frames: u64,
    pub max_mae: f64,
    pub max_error: u8,
    pub max_temporal: f64,
}
impl<'a> Report<'a> {
    pub fn new(
        output: Option<&'a Path>,
        candidate: &'static str,
        counter: &'static str,
    ) -> Result<Self, Error> {
        let mut csv = output
            .map(|path| File::create_new(path.join("quality.csv")))
            .transpose()?;
        if let Some(f) = &mut csv {
            writeln!(
                f,
                "state,width,height,{counter},changed_pixels,eligible_pixels,rgb_mae,eligible_rgb_mae,max_channel_error,rgb_sum_error_over_30_pixels,temporal_error_mae,temporal_error_peak"
            )?;
        }
        Ok(Self {
            output,
            candidate,
            csv,
            previous: None,
            frames: 0,
            max_mae: 0.,
            max_error: 0,
            max_temporal: 0.,
        })
    }
    pub fn clear_previous(&mut self) {
        self.previous = None;
    }
    pub fn check(
        &mut self,
        plan: &Plan,
        label: &str,
        pixels: &[Vec<u8>; 2],
        applied: u64,
        save: bool,
    ) -> Result<(), Error> {
        self.compare(plan, label, pixels, applied, save, true)
    }
    pub fn check_history(
        &mut self,
        plan: &Plan,
        label: &str,
        pixels: &[Vec<u8>; 2],
    ) -> Result<(), Error> {
        // History comparisons also include the reference cache's ordinary
        // expansion differences. Keep them separate from approximation locality.
        self.clear_previous();
        self.compare(plan, label, pixels, 0, true, false)
    }
    fn compare(
        &mut self,
        plan: &Plan,
        label: &str,
        pixels: &[Vec<u8>; 2],
        applied: u64,
        save: bool,
        locality: bool,
    ) -> Result<(), Error> {
        let size = plan.width as usize * plan.height as usize;
        if pixels.iter().any(|p| p.len() != size * 4) {
            return Err("small outline readback dimensions differ".into());
        }
        let mut allowed = vec![false; size];
        for pass in &plan.passes {
            let scene = pass.scene();
            for draw in &scene.draws {
                let Some(mut b) = small_strokes::bounds(draw) else {
                    continue;
                };
                // Use the deliberately generous, existing guarded-stroke
                // envelope plus a resolve pixel. This is a safety/locality
                // check, not a visual-quality threshold or a culling policy.
                let pad = 4. * ((b[2] - b[0]) + (b[3] - b[1]) + draw.stroke.unwrap().width) + 4.;
                b = [b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad];
                let clip = match pass {
                    Pass::Direct { clip, .. } => [
                        clip.x as f32,
                        clip.y as f32,
                        (clip.x + clip.width) as f32,
                        (clip.y + clip.height) as f32,
                    ],
                    Pass::Minimap { destination: d, .. } => {
                        b = [
                            d.x + b[0] * d.width / scene.width as f32,
                            d.y + b[1] * d.height / scene.height as f32,
                            d.x + b[2] * d.width / scene.width as f32,
                            d.y + b[3] * d.height / scene.height as f32,
                        ];
                        [d.x, d.y, d.x + d.width, d.y + d.height]
                    }
                };
                let lo = [
                    b[0].max(clip[0]).floor().max(0.) as u32,
                    b[1].max(clip[1]).floor().max(0.) as u32,
                ];
                let hi = [
                    b[2].min(clip[2]).ceil().max(0.).min(plan.width as f32) as u32,
                    b[3].min(clip[3]).ceil().max(0.).min(plan.height as f32) as u32,
                ];
                for y in lo[1]..hi[1] {
                    for x in lo[0]..hi[0] {
                        allowed[y as usize * plan.width as usize + x as usize] = true;
                    }
                }
            }
        }
        let (mut absolute, mut changed, mut bad, mut peak) = (0_u64, 0, 0, 0_u8);
        let (mut temporal, mut temporal_peak) = (0_u64, 0_i16);
        let mut errors = Vec::with_capacity(size * 3);
        let mut diff = Vec::with_capacity(size * 4);
        let previous = self
            .previous
            .as_ref()
            .filter(|(w, h, _)| (*w, *h) == (plan.width, plan.height));
        for (i, (a, b)) in pixels[0]
            .chunks_exact(4)
            .zip(pixels[1].chunks_exact(4))
            .enumerate()
        {
            if a[3] != 255 || b[3] != 255 {
                return Err("small outline target is not opaque".into());
            }
            let mut sum = 0;
            for channel in 0..3 {
                let error = i16::from(b[channel]) - i16::from(a[channel]);
                errors.push(error);
                let delta = error.unsigned_abs() as u8;
                sum += u64::from(delta);
                peak = peak.max(delta);
                diff.push(delta.saturating_mul(8));
                if let Some((_, _, old)) = previous {
                    let change = (error - old[i * 3 + channel]).abs();
                    temporal += change as u64;
                    temporal_peak = temporal_peak.max(change);
                }
            }
            diff.push(255);
            absolute += sum;
            changed += usize::from(sum != 0);
            bad += usize::from(sum > 30);
            if locality && sum != 0 && !allowed[i] {
                return Err(format!(
                    "small outline changed pixels outside eligible envelopes: {label} at {},{}",
                    i % plan.width as usize,
                    i / plan.width as usize
                )
                .into());
            }
        }
        let eligible = allowed.iter().filter(|v| **v).count();
        let mae = absolute as f64 / (size.max(1) * 3) as f64;
        let local_mae = absolute as f64 / (eligible.max(1) * 3) as f64;
        let temporal_mae = temporal as f64 / (size.max(1) * 3) as f64;
        self.previous = Some((plan.width, plan.height, errors));
        self.frames += 1;
        self.max_mae = self.max_mae.max(mae);
        self.max_error = self.max_error.max(peak);
        self.max_temporal = self.max_temporal.max(temporal_mae);
        if let Some(f) = &mut self.csv {
            writeln!(
                f,
                "{label},{},{},{applied},{changed},{eligible},{mae:.8},{local_mae:.8},{peak},{bad},{temporal_mae:.8},{temporal_peak}",
                plan.width, plan.height
            )?;
        }
        if save && let Some(path) = self.output {
            for (name, data) in [
                ("reference", &pixels[0]),
                (self.candidate, &pixels[1]),
                ("difference-x8", &diff),
            ] {
                save_png(
                    &path.join(format!("{label}-{name}.png")),
                    plan.width,
                    plan.height,
                    png::ColorType::Rgba,
                    data,
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_scene::{
        plan::{Draw, Scene, Shape},
        presentation::Clip,
    };
    use engine_common::{RenderColor, RenderPoint, Stroke};

    fn plan() -> Plan {
        let color = RenderColor::WHITE;
        Plan {
            width: 40,
            height: 40,
            passes: vec![Pass::Direct {
                clip: Clip {
                    x: 0,
                    y: 0,
                    width: 40,
                    height: 40,
                },
                scene: Scene {
                    width: 40,
                    height: 40,
                    reuse: vec![],
                    retained: vec![],
                    draws: vec![Draw {
                        shape: Shape::Polygon(vec![
                            RenderPoint::new(2., 2.),
                            RenderPoint::new(3., 2.),
                            RenderPoint::new(2., 3.),
                        ]),
                        fill: Some(color),
                        stroke: Some(Stroke::new(color, 0.75)),
                    }],
                },
            }],
        }
    }

    #[test]
    fn quality_checks_dimensions_alpha_locality_and_signed_temporal_error() {
        let plan = plan();
        let reference = [10, 20, 30, 255].repeat(1600);
        let mut changed = reference.clone();
        changed[(2 * 40 + 2) * 4] = 20;
        let mut q = Report::new(None, "small-bevel", "small_strokes").unwrap();
        q.check(
            &plan,
            "one",
            &[reference.clone(), changed.clone()],
            1,
            false,
        )
        .unwrap();
        assert_eq!(q.max_error, 10);
        changed[(2 * 40 + 2) * 4] = 0;
        q.check(
            &plan,
            "two",
            &[reference.clone(), changed.clone()],
            1,
            false,
        )
        .unwrap();
        assert!((q.max_temporal - 20. / 4800.).abs() < 1e-10);
        changed[(39 * 40 + 39) * 4] = 0;
        assert!(
            q.check(&plan, "outside", &[reference.clone(), changed], 1, false)
                .is_err()
        );
        let mut alpha = reference.clone();
        alpha[3] = 0;
        assert!(
            q.check(&plan, "alpha", &[reference.clone(), alpha], 0, false)
                .is_err()
        );
        assert!(
            q.check(&plan, "size", &[reference, vec![]], 0, false)
                .is_err()
        );
    }
}
