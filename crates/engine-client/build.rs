fn main() {
    // Source archives may inject an identity; never mistake a runtime checkout
    // or a machine's installed git for the version of this executable.
    println!("cargo:rerun-if-env-changed=SPACEWARS_BUILD_REVISION");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git = |args: &[&str]| {
        // An archive inside another checkout must not inherit its identity.
        if !root.join(".git").exists() {
            return None;
        }
        std::process::Command::new("git")
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .current_dir(&root)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|text| text.trim().to_owned())
    };
    // Resolve these via git as .git can be a worktree indirection file.
    for path in ["HEAD", "index", "packed-refs", "refs/tags"] {
        if let Some(path) = git(&["rev-parse", "--git-path", path]) {
            watch_existing(root.join(path));
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        watch_existing(root.join(path));
    }
    // Track source edits too, so a clean build followed by an unstaged edit is
    // not accidentally labelled as a clean commit. Keep the normal root-level
    // target/data directories out of this conservative source-root watch list.
    for path in ["Cargo.toml", "Cargo.lock", "crates", "scenarios", "vendor"] {
        watch_existing(root.join(path));
    }
    let revision = std::env::var("SPACEWARS_BUILD_REVISION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            // `describe --dirty` refreshes the index that this script watches,
            // invalidating its own build. Read the identity and status without
            // rewriting Git metadata instead.
            let revision = git(&["describe", "--always", "--abbrev=12"])?;
            let status = git(&["status", "--porcelain", "--untracked-files=no"])?;
            Some(if status.is_empty() {
                revision
            } else {
                format!("{revision}-dirty")
            })
        })
        .unwrap_or_else(|| "unknown (source archive)".into());
    println!("cargo:rustc-env=SPACEWARS_BUILD_REVISION={revision}");
    slint_build::compile("ui/main.slint").expect("slint compile failed");
}

fn watch_existing(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    // Cargo treats a missing watched path as stale on every invocation. Git's
    // packed refs and loose current-branch ref are both optional; existing HEAD,
    // index and ref storage still invalidate the identity when commits change.
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
