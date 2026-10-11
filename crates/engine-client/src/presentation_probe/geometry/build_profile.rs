//! Cold immutable-template construction only. All templates in a batch stay
//! alive until it completes; source/simulation/projection and GL are untimed.
use super::{Context, Error, Stamp, live_retained};
use crate::{
    client_scenarios::RenderBackend,
    gpu_scene::{
        presentation,
        scalable::{BuildProfile, Builder},
    },
    presentation_probe::Options,
    render::Viewport,
};
use std::{collections::BTreeSet, fs::File, io::Write, sync::Arc, time::Duration};

pub(super) fn run(
    context: &Context,
    options: &Options,
    width: u32,
    height: u32,
) -> Result<(), Error> {
    let v = Viewport::new(width as f32, height as f32);
    let metadata = serde_json::json!({"version":2,"kind":"template-build-profile",
        "gl":context.identity,"width":width,"height":height,"batches":options.presentation_frames,
        "repeats":options.presentation_repeats,"timing":"CPU template construction and batch destruction only; no source generation, simulation, projection, cache insertion, GL or scanout",
        "instrumentation":"rotated plain/profiled/memoized/memoized-profiled blocks; detailed clocks only in profiled modes"});
    eprintln!("build_profile={metadata}");
    if let Some(path) = &options.geometry_output {
        std::fs::create_dir(path)?;
        File::create_new(path.join("manifest.json"))?
            .write_all(serde_json::to_string_pretty(&metadata)?.as_bytes())?;
    }
    println!(
        "fixture,mode,repeat,batches,definitions,polygons,bytes,wall_ms,cpu_ms,domain_ms,prepare_ms,fill_ms,stroke_ms,finish_ms,reuse_ms,build_reuses"
    );
    for fixture in live_retained::FIXTURES {
        let mut s = live_retained::scenario(v, fixture)?;
        s.step(&[], Duration::from_nanos(16_666_667));
        let frames = s.render_frames(RenderBackend::Raster, v);
        let plan = presentation::build_with_retained(&frames, v, s.frame_layout(), 1., true)?;
        let mut seen = BTreeSet::new();
        let definitions: Vec<_> = plan
            .passes
            .iter()
            .flat_map(|p| &p.scene().retained)
            .filter(|p| seen.insert(Arc::as_ptr(&p.definition) as usize))
            .map(|p| p.definition.clone())
            .collect();
        if definitions.is_empty() {
            return Err("build profile contains no production definitions".into());
        }
        let polygons: usize = definitions.iter().map(|d| d.len()).sum();
        let mut builder = Builder::default();
        for definition in &definitions {
            let plain = builder.build(definition, true)?;
            for memoized in [false, true] {
                let (profiled, _) = builder.build_profiled(definition, true, memoized)?;
                if !plain.identical(&profiled) {
                    return Err("profiling/memoization changed template geometry or guards".into());
                }
            }
            if !plain.identical(&builder.build_memoized(definition, true)?) {
                return Err("unprofiled memoization changed template geometry or guards".into());
            }
        }
        eprintln!(
            "validated build profile fixture={} definitions={} polygons={polygons}",
            fixture.name,
            definitions.len()
        );
        for repeat in 0..options.presentation_repeats {
            for offset in 0..4 {
                let mode = (repeat + offset) % 4;
                let detailed = mode % 2 != 0;
                let memoized = mode >= 2;
                let name = ["plain", "profiled", "memoized", "memoized-profiled"][mode as usize];
                let mut builder = Builder::default();
                let (mut wall, mut cpu, mut bytes) = (0., 0., 0);
                let mut profile = BuildProfile::default();
                let mut build_reuses = 0;
                for _ in 0..options.presentation_frames {
                    let stamp = Stamp::now()?;
                    let mut meshes = Vec::with_capacity(definitions.len());
                    for definition in &definitions {
                        let mesh = if detailed {
                            let (mesh, part) =
                                builder.build_profiled(definition, true, memoized)?;
                            profile.accumulate(part);
                            mesh
                        } else if memoized {
                            builder.build_memoized(definition, true)?
                        } else {
                            builder.build(definition, true)?
                        };
                        build_reuses += mesh.build_reuses;
                        meshes.push(mesh);
                    }
                    bytes = std::hint::black_box(&meshes)
                        .iter()
                        .map(|m| m.bytes())
                        .sum::<usize>();
                    drop(meshes);
                    let (w, c) = stamp.elapsed()?;
                    wall += w;
                    cpu += c;
                }
                let n = f64::from(options.presentation_frames);
                let stages = [
                    profile.domain,
                    profile.prepare,
                    profile.fill,
                    profile.stroke,
                    profile.finish,
                    profile.reuse,
                ]
                .map(|d| format!("{:.6}", d.as_secs_f64() * 1000. / n))
                .join(",");
                println!(
                    "{},{name},{repeat},{},{},{polygons},{bytes},{:.6},{:.6},{stages},{:.1}",
                    fixture.name,
                    options.presentation_frames,
                    definitions.len(),
                    wall / n,
                    cpu / n,
                    build_reuses as f64 / n
                );
            }
        }
    }
    Ok(())
}
