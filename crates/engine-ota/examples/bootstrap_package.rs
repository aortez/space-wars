//! Workstation artifact preparation only; never installs a device.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use engine_ota::boot::SlotRoots;
    use engine_ota::bootstrap::{Board, PlanRequest, build_package};
    use engine_ota::{Sha256, recovery::Slot};
    use std::io::Read;
    use std::path::Path;
    let usage = "usage: bootstrap_package ROOTFS_EXT4 BOOT_DIRECTORY NEW_OUTPUT_DIRECTORY pi4|pi5 hdmi|hyperpixel|picade ROOT_A_PARTUUID ROOT_B_PARTUUID a|b [--retained-initramfs BYTES:SHA256]";
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let (positional, retained) = match arguments.as_slice() {
        [positional @ .., flag, value] if flag == "--retained-initramfs" => {
            // The device's current spacewars-recovery.cpio.gz. The device check
            // derives the same plan from its own boot files and refuses a mismatch.
            let (bytes, sha256) = value
                .to_str()
                .and_then(|value| value.split_once(':'))
                .ok_or("retained initramfs must be BYTES:SHA256")?;
            (
                positional,
                Some(engine_ota::Blob {
                    bytes: bytes.parse()?,
                    sha256: Sha256::try_from(sha256.to_owned())?,
                }),
            )
        }
        positional => (positional, None),
    };
    let [root, boot, output, board, profile, root_a, root_b, target] = positional else {
        return Err(usage.into());
    };
    let board = match board.to_str() {
        Some("pi4") => Board::Pi4,
        Some("pi5") => Board::Pi5,
        _ => return Err("board must be pi4 or pi5".into()),
    };
    let target = match target.to_str() {
        Some("a") => Slot::A,
        Some("b") => Slot::B,
        _ => return Err("target must be a or b".into()),
    };
    let mut nonce = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut nonce)?;
    let request = PlanRequest {
        board,
        profile: profile.to_str().ok_or("invalid profile")?.into(),
        target,
        roots: SlotRoots::new(
            root_a.to_str().ok_or("invalid root A PARTUUID")?.into(),
            root_b.to_str().ok_or("invalid root B PARTUUID")?.into(),
        )?,
        trial_id: Sha256::digest(&nonce),
        retained_initramfs: retained,
    };
    let package = build_package(
        Path::new(root),
        Path::new(boot),
        Path::new(output),
        &request,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "purpose": package.purpose, "output": Path::new(output).display().to_string(), "source": package.image.source,
            "build_id": package.image.build_id, "boot_identity": package.image.boot_identity,
            "board": package.plan.board, "profile": package.plan.profile, "target": package.plan.target,
            "staged_asset_bytes": package.plan.staged_asset_bytes,
            "compressed": package.compressed, "expanded": package.expanded,
            "device_changes": false,
        }))?
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bootstrap packaging requires Linux filesystem tools");
    std::process::exit(1);
}
