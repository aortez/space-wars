//! Pinned workstation tooling only; no candidate-provided command is executed.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use engine_ota::artifact::{Candidate, MAX_POLICY_BYTES, Policy, pack, verify};
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;
    fn read(path: &std::ffi::OsStr, limit: usize) -> std::io::Result<Vec<u8>> {
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        if !handle.metadata()?.is_file() {
            return Err(std::io::Error::other("metadata must be a regular file"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(format!("/proc/self/fd/{}", handle.as_raw_fd()))?
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    match arguments.as_slice() {
        [mode, policy, root, output] if mode == "pack" => {
            let policy = Policy::parse(&read(policy, MAX_POLICY_BYTES)?)?;
            println!("{}", serde_json::to_string_pretty(&pack(&policy, Path::new(root), Path::new(output))?)?);
        }
        [mode, policy, candidate, payload, scratch] if mode == "verify" => {
            let policy = Policy::parse(&read(policy, MAX_POLICY_BYTES)?)?;
            let candidate = Candidate::parse(&read(candidate, 4096)?)?;
            println!("{}", serde_json::to_string_pretty(&verify(&policy, &candidate, Path::new(payload), Path::new(scratch))?)?);
        }
        _ => return Err("usage: image_candidate pack POLICY EXPANDED_EXT4 NEW_DIRECTORY | verify POLICY CANDIDATE_JSON ZSTD_IMAGE SCRATCH_DIRECTORY".into()),
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("image qualification requires Linux filesystem tools");
    std::process::exit(1);
}
