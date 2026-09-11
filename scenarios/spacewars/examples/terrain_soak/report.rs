use std::{error::Error, fmt::Write as _, fs, path::Path};

use engine_common::{Fill, RenderColor, RenderFrame, RenderPrimitive, Stroke, TextAnchor};

use super::Report;

pub fn write(output: &Path, report: &Report) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_string(report)?;
    // Embedded data must not be able to close its script element (labels and
    // command arguments can contain arbitrary user text).
    let html = include_str!("report.html").replace("__DATA__", &json.replace('<', "\\u003c"));
    // Keep the last completed report readable while publishing a larger one.
    for (name, contents) in [
        ("report.json", json.as_str()),
        ("report.html", html.as_str()),
    ] {
        let temporary = output.join(format!("{name}.tmp"));
        fs::write(&temporary, contents)?;
        fs::rename(temporary, output.join(name))?;
    }
    Ok(())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn color(c: RenderColor) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (c.r.clamp(0.0, 1.0) * 255.0) as u8,
        (c.g.clamp(0.0, 1.0) * 255.0) as u8,
        (c.b.clamp(0.0, 1.0) * 255.0) as u8
    )
}

fn style(fill: Option<Fill>, stroke: Option<Stroke>) -> String {
    let opacity = fill.map_or(1.0, |f| f.color.a);
    let fill = fill.map_or_else(|| "none".into(), |f| color(f.color));
    let stroke = stroke.map_or_else(String::new, |s| {
        format!(
            " stroke=\"{}\" stroke-width=\"{:.3}\" stroke-opacity=\"{:.3}\"",
            color(s.color),
            s.width,
            s.color.a
        )
    });
    format!("fill=\"{fill}\" fill-opacity=\"{opacity:.3}\"{stroke}")
}

/// Actual scenario draw primitives, captured between ticks. SVG is an inspection
/// view, not a screenshot of either client renderer or a measured render pass.
pub fn svg(frame: &RenderFrame) -> String {
    let bounds = frame.camera.world_bounds(5.0 / 3.0);
    let mut output = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{:.2} {:.2} {:.2} {:.2}\" role=\"img\" aria-label=\"Recorded simulation world\"><g transform=\"scale(1 -1)\">",
        bounds.min.x,
        -bounds.max.y,
        bounds.width(),
        bounds.height()
    );
    for layer in frame.ordered_layers() {
        for primitive in &layer.primitives {
            match primitive {
                RenderPrimitive::Circle(c) => {
                    write!(
                        output,
                        "<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" {}/>",
                        c.center.x,
                        c.center.y,
                        c.radius,
                        style(c.fill, c.stroke)
                    )
                    .unwrap();
                }
                RenderPrimitive::Line(l) => {
                    write!(
                        output,
                        "<path d=\"M{:.2},{:.2}L{:.2},{:.2}\" {}/>",
                        l.start.x,
                        l.start.y,
                        l.end.x,
                        l.end.y,
                        style(None, Some(l.stroke))
                    )
                    .unwrap();
                }
                RenderPrimitive::Polygon(p) => {
                    output.push_str("<polygon points=\"");
                    for point in &p.points {
                        write!(output, "{:.2},{:.2} ", point.x, point.y).unwrap();
                    }
                    write!(output, "\" {}/>", style(p.fill, p.stroke)).unwrap();
                }
                RenderPrimitive::Text(t) => {
                    let (anchor, baseline) = match t.anchor {
                        TextAnchor::Center => ("middle", "central"),
                        TextAnchor::TopLeft => ("start", "hanging"),
                    };
                    write!(output, "<text transform=\"translate({:.2} {:.2}) scale(1 -1)\" fill=\"{}\" fill-opacity=\"{:.3}\" font-family=\"sans-serif\" font-size=\"{:.2}\" text-anchor=\"{anchor}\" dominant-baseline=\"{baseline}\">{}</text>", t.position.x, t.position.y, color(t.color), t.color.a, t.size * frame.camera.height / 540.0, escape(&t.text)).unwrap();
                }
            }
        }
    }
    output.push_str("</g></svg>");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{RenderPoint, RenderText};

    #[test]
    fn snapshot_escapes_text_and_preserves_draw_order() {
        let mut frame = RenderFrame::default();
        frame.push_primitive(
            2,
            RenderPrimitive::Text(RenderText::new(
                RenderPoint::ZERO,
                "<script>late & unsafe</script>",
            )),
        );
        frame.push_primitive(
            -1,
            RenderPrimitive::Text(RenderText::new(RenderPoint::ZERO, "early")),
        );
        let image = svg(&frame);
        assert!(!image.contains("<script>"));
        assert!(image.contains("&lt;script&gt;late &amp; unsafe&lt;/script&gt;"));
        assert!(image.find(">early</text>").unwrap() < image.find("&lt;script&gt;late").unwrap());
    }
}
