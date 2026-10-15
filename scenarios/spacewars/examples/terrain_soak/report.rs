use std::{error::Error, fs, path::Path};

#[cfg(test)]
use engine_common::{RenderColor, RenderFrame, RenderPrimitive};

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

#[path = "svg.rs"]
mod svg;
pub use svg::svg;

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
