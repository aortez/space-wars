//! Explicit operator diagnostic, excluded from ordinary device builds.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use engine_ota::recovery_qualification as test;
    use std::io::Read;
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: recovery_qualify inspect|prepare|activate|restore|status ID_OR_PLAN_SHA256"
                .into(),
        );
    }
    let id = args[1].clone().try_into().map_err(std::io::Error::other)?;
    let result = match args[0].as_str() {
        "inspect" => test::inspect(id)?,
        "prepare" => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(test::MAX_PLAN_BYTES + 1)
                .read_to_end(&mut bytes)?;
            test::prepare(&bytes, id)?
        }
        "activate" => test::activate(id)?,
        "restore" => test::restore(id)?,
        "status" => test::status(id)?,
        _ => return Err("unknown recovery qualification operation".into()),
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("recovery qualification requires Linux");
    std::process::exit(1);
}
