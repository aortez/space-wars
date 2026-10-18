//! Read-only inspection of an extracted build artifact, never a device installer.
use engine_ota::boot::{BootCommandLine, BootManifest, SlotRoots, hardware_profile};
use engine_ota::{ImageIdentity, Sha256};
use sha2::{Digest, Sha256 as Hasher};
use std::fs;
use std::io::Read;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [directory, image, root_a, root_b] = arguments.as_slice() else {
        return Err(
            "usage: boot_artifact_probe BOOT_DIRECTORY IMAGE_JSON ROOT_A_PARTUUID ROOT_B_PARTUUID"
                .into(),
        );
    };
    let directory = Path::new(directory);
    let image = ImageIdentity::parse(&fs::read(image)?)?;
    let manifest = BootManifest::parse(
        &fs::read(directory.join("spacewars-boot-manifest.json"))?,
        image.boot_identity,
    )?;
    let boot_id: Sha256 = fs::read_to_string(directory.join("spacewars-boot-id"))?
        .trim()
        .to_owned()
        .try_into()?;
    if boot_id != image.boot_identity {
        return Err("boot identity file differs from packaged image".into());
    }
    let mut total_bytes = 0;
    for (name, expected) in manifest.files() {
        let mut file = fs::File::open(directory.join(name))?;
        if file.metadata()?.len() != expected.bytes {
            return Err(format!("boot asset length mismatch: {name}").into());
        }
        let mut hash = Hasher::new();
        let mut bytes = 0;
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            if bytes > expected.bytes {
                return Err(format!("boot asset grew: {name}").into());
            }
            hash.update(&buffer[..count]);
        }
        let actual: Sha256 = format!("{:x}", hash.finalize()).try_into()?;
        if bytes != expected.bytes || actual != expected.sha256 {
            return Err(format!("boot asset hash mismatch: {name}").into());
        }
        total_bytes += bytes;
    }
    let template = BootCommandLine::parse(&fs::read(directory.join("spacewars-cmdline.txt"))?)?;
    // These describe the inspected artifact. Device service callers must use
    // inspect_layout(), never arguments supplied by the kiosk.
    let roots = SlotRoots::new(
        root_a.to_str().ok_or("invalid root A PARTUUID")?.into(),
        root_b.to_str().ok_or("invalid root B PARTUUID")?.into(),
    )?;
    let default = template
        .default_slot(&roots, &fs::read(directory.join("cmdline.txt"))?)
        .ok();
    let trial = template.trial(&roots, &fs::read(directory.join("spacewars-trial.txt"))?)?;
    let selection = fs::read_to_string(directory.join("spacewars-device.txt"))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "boot_identity": manifest.identity(),
            "immutable_files": manifest.files().len(),
            "immutable_bytes": total_bytes,
            "kernel_build_id": fs::read_to_string(directory.join("spacewars-kernel-build-id"))?.trim(),
            "hardware_profile": hardware_profile(&selection)?,
            "provisioned_default": default,
            "trial": trial,
        }))?
    );
    Ok(())
}
