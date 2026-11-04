//! Offline TUF/payload check for a separately authorized publication stage.
//! Usage: feed_inventory TRUSTED_ROOT FLAT_DIRECTORY UTC_TIMESTAMP
use engine_ota::{MAX_METADATA_BYTES, UsableTime, publication};
use std::io::Read;
use std::path::Path;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [root, directory, now] = args.as_slice() else {
        return Err("usage: feed_inventory TRUSTED_ROOT FLAT_DIRECTORY UTC_TIMESTAMP".into());
    };
    let mut bytes = Vec::new();
    std::fs::File::open(root)?
        .take(MAX_METADATA_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let inventory =
        publication::inventory(&bytes, Path::new(directory), UsableTime(now.parse()?)).await?;
    println!("{}", serde_json::to_string_pretty(&inventory)?);
    Ok(())
}
