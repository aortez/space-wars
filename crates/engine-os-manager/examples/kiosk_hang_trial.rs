//! Explicit root-owner diagnostic, excluded from ordinary image builds.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use engine_os_manager::supervised_kiosk_hang as hang;
    use std::io::Read;
    if unsafe { libc::geteuid() } != 0 {
        return Err("kiosk hang trial requires separately authenticated root access".into());
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !matches!(args.as_slice(), [op] if op == "inspect")
        && !matches!(args.as_slice(), [op, _] if op == "stop")
    {
        return Err(
            "usage: kiosk_hang_trial inspect < expectation.json | stop PLAN_SHA256 < plan.json"
                .into(),
        );
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(hang::MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > hang::MAX_INPUT_BYTES {
        return Err("oversized hang-trial input".into());
    }
    let result = if args[0] == "inspect" {
        let plan = hang::inspect(serde_json::from_slice(&bytes)?)?;
        serde_json::json!({"plan_sha256": plan.fingerprint()?, "plan": plan})
    } else {
        hang::stop(serde_json::from_slice(&bytes)?, args[1].clone().try_into()?)?
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("kiosk hang qualification requires Linux");
    std::process::exit(1);
}
