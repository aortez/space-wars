//! Replay a frozen shared packing case without running the originating scenario.
use clap::Parser;
use engine_core::Vec2;
use engine_rapier::terrain::PackingSnapshot;
use std::{fmt::Write, path::PathBuf};

#[derive(Parser)]
struct Args {
    snapshot: PathBuf,
    /// Output stem; writes .json and one .svg per attempted placement.
    #[arg(long)]
    output: PathBuf,
}

fn polygon(svg: &mut String, vertices: impl IntoIterator<Item = Vec2>, color: &str, width: f32) {
    let points = vertices
        .into_iter()
        .map(|p| format!("{},{}", p.x, -p.y))
        .collect::<Vec<_>>()
        .join(" ");
    writeln!(svg, r#"<polygon points="{points}" fill="{color}" fill-opacity="0.35" stroke="{color}" stroke-width="{width}"/>"#).unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let snapshot = PackingSnapshot::from_bytes(&std::fs::read(&args.snapshot)?)?;
    let report = snapshot.inspect()?;
    std::fs::write(
        args.output.with_extension("json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    let size = report.cell_size;
    let min = report
        .grains
        .iter()
        .fold(Vec2::new(f32::INFINITY, f32::INFINITY), |m, g| {
            Vec2::new(m.x.min(g.position.x), m.y.min(g.position.y))
        })
        - Vec2::new(size * 3.0, size * 3.0);
    let max = report
        .grains
        .iter()
        .fold(Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY), |m, g| {
            Vec2::new(m.x.max(g.position.x), m.y.max(g.position.y))
        })
        + Vec2::new(size * 3.0, size * 3.0);
    let width = max.x - min.x;
    let height = max.y - min.y;
    for (i, attempt) in report.attempts.iter().enumerate() {
        let mut svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="1400" height="{}" viewBox="{} {} {} {}"><rect x="{}" y="{}" width="{}" height="{}" fill="#101820"/>"##,
            1400.0 * (height + size * 1.5) / width,
            min.x,
            -max.y - size * 1.5,
            width,
            height + size * 1.5,
            min.x,
            -max.y - size * 1.5,
            width,
            height + size * 1.5
        );
        writeln!(svg, r##"<text x="{}" y="{}" fill="white" font-size="{}">Attempt {}: {} grains · Final result: {} accepted, {}</text><text x="{}" y="{}" fill="white" font-size="{}">Purple: cells · Green: clear · Red: blocked · Gold: unstable · Pink: blocker bounds/ID</text>"##,
            min.x+size*0.2,-max.y-size*0.9,size*0.3,i+1,attempt.grains.len(),report.accepted.len(),report.remaining_reason,
            min.x+size*0.2,-max.y-size*0.35,size*0.26).unwrap();
        writeln!(svg, r#"<defs><clipPath id="plot"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath></defs><g clip-path="url(#plot)">"#, min.x, -max.y, width, height).unwrap();
        for p in &report.terrain {
            polygon(&mut svg, p.iter().copied(), "#927444", size * 0.01);
        }
        for g in &report.grains {
            if let Some(vertices) = g.shape.vertices(g.radius) {
                polygon(
                    &mut svg,
                    vertices
                        .into_iter()
                        .map(|p| g.position + p.rotate_radians(g.angle)),
                    "#e9c983",
                    size * 0.035,
                );
            } else {
                writeln!(svg, r##"<circle cx="{}" cy="{}" r="{}" fill="#e9c983" fill-opacity="0.45" stroke="#e9c983" stroke-width="{}"/>"##,g.position.x,-g.position.y,g.radius,size*0.035).unwrap();
            }
        }
        for &p in &attempt.centers {
            polygon(
                &mut svg,
                [
                    Vec2::new(-0.5, -0.5),
                    Vec2::new(0.5, -0.5),
                    Vec2::new(0.5, 0.5),
                    Vec2::new(-0.5, 0.5),
                ]
                .into_iter()
                .map(|v| p + (v * size).rotate_radians(report.angle)),
                "#ac8cff",
                size * 0.035,
            );
        }
        for p in &attempt.patches {
            polygon(
                &mut svg,
                p.vertices.iter().copied(),
                if p.clear { "#5bdba0" } else { "#ff4b4b" },
                size * 0.045,
            );
        }
        for p in &attempt.unstable_centers {
            writeln!(svg, r##"<circle cx="{}" cy="{}" r="{}" fill="none" stroke="#ffd34d" stroke-width="{}"/>"##, p.x,-p.y,size*0.35,size*0.09).unwrap();
        }
        let blockers: std::collections::BTreeSet<_> = attempt
            .patches
            .iter()
            .flat_map(|p| &p.blockers)
            .map(|id| id.entity)
            .collect();
        for obstacle in report
            .obstacles
            .iter()
            .filter(|o| blockers.contains(&o.body.entity))
        {
            let (lo, hi) = obstacle.bounds;
            writeln!(svg, r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#ff86e0" stroke-width="{}" stroke-dasharray="{} {}"/><text x="{}" y="{}" fill="#ff86e0" font-size="{}">{} {}</text>"##,
                lo.x,-hi.y,hi.x-lo.x,hi.y-lo.y,size*0.04,size*0.08,size*0.06,
                lo.x,-hi.y-size*0.1,size*0.22,obstacle.kind,obstacle.body.entity.value()).unwrap();
        }
        svg.push_str("</g></svg>");
        let path = args.output.with_extension(format!("attempt-{}.svg", i + 1));
        std::fs::write(&path, svg)?;
    }
    println!(
        "{} grains; {} accepted; {} attempts; {}",
        report.grains.len(),
        report.accepted.len(),
        report.attempts.len(),
        report.remaining_reason
    );
    Ok(())
}
