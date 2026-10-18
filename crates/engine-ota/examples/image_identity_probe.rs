//! Inspect a build artifact record, or the fixed installed record without a path.
use engine_ota::{ImageIdentity, MAX_IMAGE_IDENTITY_BYTES};
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let identity = match arguments.as_slice() {
        [path] => {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(MAX_IMAGE_IDENTITY_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            ImageIdentity::parse(&bytes)?
        }
        #[cfg(target_os = "linux")]
        [] => ImageIdentity::read_installed()?,
        _ => return Err("usage: image_identity_probe [image.json]".into()),
    };
    println!("{}", serde_json::to_string_pretty(&identity)?);
    Ok(())
}
