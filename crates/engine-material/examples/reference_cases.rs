//! Prints the material model's reference tables as Markdown and writes the
//! JSON and Markdown reports that `docs/design/material-damage.md` cites and
//! the crate's tests compare against.

use engine_material::fixtures::{render_markdown, report};
use std::{fs, path::Path};

fn main() {
    let report = report();
    let markdown = render_markdown(&report);
    print!("{markdown}");
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/data");
    let json = serde_json::to_string_pretty(&report).expect("serializable report") + "\n";
    for (name, content) in [
        ("material-model-v2.json", json),
        ("material-model-v2.md", markdown),
    ] {
        let path = data.join(name);
        fs::write(&path, content).expect("write the report beside the other evidence files");
        eprintln!("wrote {}", path.display());
    }
}
