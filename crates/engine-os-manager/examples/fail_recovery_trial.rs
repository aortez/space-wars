//! Explicit owner recovery-observer fault; excluded from ordinary images.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use engine_os_manager::supervised_recovery_failure as failure;
    use std::io::Read;
    if unsafe { libc::geteuid() } != 0 {
        return Err(
            "recovery failure qualification requires separately authenticated root access".into(),
        );
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !matches!(args.as_slice(), [op] if op == "inspect")
        && !matches!(args.as_slice(), [op, _] if op == "kill" || op == "observe")
    {
        return Err("usage: fail_recovery_trial inspect < expectation.json | kill PLAN_SHA256 < plan.json | observe PLAN_SHA256 < plan.json".into());
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(failure::MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > failure::MAX_INPUT_BYTES {
        return Err("oversized recovery failure input".into());
    }
    let result = match args[0].as_str() {
        "inspect" => {
            let plan = failure::inspect(serde_json::from_slice(&bytes)?)?;
            serde_json::json!({"plan_sha256":plan.fingerprint()?, "plan":plan})
        }
        "kill" => failure::kill(serde_json::from_slice(&bytes)?, args[1].clone().try_into()?)?,
        "observe" => {
            failure::observe(serde_json::from_slice(&bytes)?, args[1].clone().try_into()?)?
        }
        _ => unreachable!(),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("recovery failure qualification requires Linux");
    std::process::exit(1);
}
