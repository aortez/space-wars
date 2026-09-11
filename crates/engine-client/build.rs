fn main() {
    // Source archives may inject an identity; never mistake a runtime checkout
    // or a machine's installed git for the version of this executable.
    println!("cargo:rerun-if-env-changed=SPACEWARS_BUILD_REVISION");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|text| text.trim().to_owned())
    };
    // Resolve these via git as .git can be a worktree indirection file.
    for path in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    // Track source edits too, so a clean build followed by an unstaged edit is
    // not accidentally labelled as a clean commit. Exclude build/data trees.
    for path in ["Cargo.toml", "Cargo.lock", "crates", "scenarios", "vendor"] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    let revision = std::env::var("SPACEWARS_BUILD_REVISION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| git(&["describe", "--always", "--dirty", "--abbrev=12"]))
        .unwrap_or_else(|| "unknown (source archive)".into());
    println!("cargo:rustc-env=SPACEWARS_BUILD_REVISION={revision}");
    slint_build::compile("ui/main.slint").expect("slint compile failed");
}
