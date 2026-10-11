//! Bounded prepared inputs: simulation/source/projection are not queued-render
//! timings. Definitions stay alive with this sequence, unlike live producer GC.
use super::{Error, live_retained, terrain};
use crate::{
    client_scenarios::RenderBackend,
    gpu_scene::{
        plan::Shape,
        presentation::{self, Clip, Pass, Plan},
    },
    render::Viewport,
};
use std::{collections::HashSet, mem::size_of, sync::Arc, time::Duration};

pub(super) const MAX_FRAMES: u32 = 128;
const MAX_PAYLOAD: usize = 256 * 1024 * 1024;

pub(super) struct Sequence {
    pub frames: Vec<Plan>,
    pub payload_bytes: usize,
}

pub(super) fn limits(frames: u32, width: u32, height: u32, depth: usize) -> Result<usize, Error> {
    if frames == 0 || frames > MAX_FRAMES || !(1..=8).contains(&depth) || width == 0 || height == 0
    {
        return Err(
            "stream history requires 1..128 frames, positive dimensions and queue depth 1..8"
                .into(),
        );
    }
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4));
    match bytes.and_then(|n| n.checked_mul(depth).map(|total| (n, total))) {
        Some((bytes, total)) if total <= 64 * 1024 * 1024 => Ok(bytes),
        _ => Err("stream history validation exceeds the 64 MiB pixel-pack budget".into()),
    }
}

pub(super) fn prepare(
    name: &str,
    v: Viewport,
    count: u32,
    validation: bool,
) -> Result<Sequence, Error> {
    let mut replay = live_retained::FIXTURES
        .iter()
        .find(|f| f.name == name)
        .map(|f| live_retained::scenario(v, *f))
        .transpose()?;
    let mut terrain = if replay.is_none() {
        Some(terrain::StreamSource::new(name)?)
    } else {
        None
    };
    let mut frames = Vec::new();
    let mut payload_bytes = 0;
    let mut definitions = HashSet::new();
    for step in 0..count as usize {
        let viewport = if validation && (64..72).contains(&step) {
            Viewport::new((v.width * 0.625).ceil(), (v.height * 0.5).ceil())
        } else {
            v
        };
        let mut plan = if let Some(s) = &mut replay {
            s.step(&[], Duration::from_nanos(16_666_667));
            let frames = s.render_frames(RenderBackend::Raster, viewport);
            presentation::build_with_retained(&frames, viewport, s.frame_layout(), 1., true)?
        } else {
            let scene = terrain.as_mut().unwrap().generate(step, viewport)?;
            Plan {
                width: scene.width,
                height: scene.height,
                passes: vec![Pass::Direct {
                    clip: Clip::new(viewport, (scene.width, scene.height)).unwrap(),
                    scene,
                }],
            }
        };
        if validation && step == 22 {
            plan.passes.clear();
        }
        // A single-frame overflow uses repeated ordinary markers; the existing
        // submission lab also covers full-camera overflow.
        if validation && step == 23 {
            for _ in 0..9 {
                let scene = crate::gpu_scene::plan::Scene {
                    width: plan.width,
                    height: plan.height,
                    draws: vec![crate::gpu_scene::plan::Draw {
                        shape: Shape::Circle {
                            center: engine_common::RenderPoint::new(8., 8.),
                            radius: 2.,
                        },
                        fill: Some(engine_common::RenderColor::RED),
                        stroke: None,
                    }],
                    reuse: vec![],
                    retained: vec![],
                };
                plan.passes.push(Pass::Direct {
                    scene,
                    clip: Clip::new(viewport, (plan.width, plan.height)).unwrap(),
                });
            }
        }
        for pass in &plan.passes {
            let s = pass.scene();
            payload_bytes += s.draws.capacity() * size_of::<crate::gpu_scene::plan::Draw>();
            payload_bytes +=
                s.retained.capacity() * size_of::<crate::gpu_scene::retained::Placement>();
            for draw in &s.draws {
                payload_bytes += match &draw.shape {
                    Shape::Polygon(points) => {
                        points.capacity() * size_of::<engine_common::RenderPoint>()
                    }
                    Shape::LocalPolygon(p) => std::mem::size_of_val(p.definition.points()),
                    _ => 0,
                };
            }
            for p in &s.retained {
                if definitions.insert(Arc::as_ptr(&p.definition)) {
                    payload_bytes += p.definition.payload_bytes();
                }
            }
        }
        if payload_bytes > MAX_PAYLOAD {
            return Err("prepared stream history exceeds 256 MiB accounted payload".into());
        }
        frames.push(plan);
    }
    Ok(Sequence {
        frames,
        payload_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_reject_unbounded_history_and_capture_allocations() {
        assert_eq!(limits(80, 1024, 768, 3).unwrap(), 3_145_728);
        for frames in [0, 129, u32::MAX] {
            assert!(limits(frames, 1024, 768, 3).is_err());
        }
        for depth in [0, 9, usize::MAX] {
            assert!(limits(80, 1024, 768, depth).is_err());
        }
        assert!(limits(80, u32::MAX, u32::MAX, 8).is_err());
        assert!(limits(80, 4096, 4096, 3).is_err());
        assert!(limits(80, 0, 768, 3).is_err());
    }
}
