"""Exercise the real client build script with a dependency-free Slint stand-in."""

import json
import os
from pathlib import Path
import shutil
import subprocess
from tempfile import TemporaryDirectory
import unittest


BUILD_SCRIPT = Path(__file__).resolve().parents[2] / "crates/engine-client/build.rs"
SLINT_STUB = '''
mod slint_build {
    pub fn compile(_: &str) -> Result<(), &'static str> { Ok(()) }
}
'''


class BuildReuseTests(unittest.TestCase):
    def setUp(self):
        self.directory = TemporaryDirectory(prefix="spacewars-build-reuse-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.client = self.root / "crates/engine-client"
        (self.client / "src").mkdir(parents=True)
        (self.root / "scenarios").mkdir()
        (self.root / "vendor").mkdir()
        for directory in ["scenarios", "vendor"]:
            (self.root / directory / "fixture.txt").write_text("Tracked fixture source\n")
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["crates/engine-client"]\nresolver = "2"\n'
        )
        (self.root / ".gitignore").write_text("/target/\n")
        (self.client / "Cargo.toml").write_text(
            '[package]\nname = "build-reuse-fixture"\nversion = "0.1.0"\n'
            'edition = "2024"\n'
        )
        (self.client / "build.rs").write_text(BUILD_SCRIPT.read_text() + SLINT_STUB)
        (self.client / "src/main.rs").write_text(
            'fn main() { println!("{}", env!("SPACEWARS_BUILD_REVISION")); }\n'
        )
        # No third-party dependencies, display, network, or time thresholds.
        self.env = os.environ.copy()
        for key in list(self.env):
            if key.startswith("GIT_") or key == "SPACEWARS_BUILD_REVISION":
                self.env.pop(key)
        self.env["GIT_CONFIG_NOSYSTEM"] = "1"
        self.env["GIT_CONFIG_GLOBAL"] = os.devnull
        self.env["CARGO_TARGET_DIR"] = str(self.root / "target")
        self.run_command("cargo", "generate-lockfile", "--offline")
        self.git("init", "--initial-branch=main")
        self.git("add", ".")
        self.git("commit", "-m", "Fixture")

    def run_command(self, *args, env=None):
        result = subprocess.run(
            args, cwd=self.root, env=self.env if env is None else env,
            text=True, capture_output=True, check=False, timeout=120,
        )
        self.assertEqual(
            result.returncode, 0,
            f"{args!r}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )
        return result

    def git(self, *args):
        return self.run_command(
            "git", "-c", "user.name=Build fixture",
            "-c", "user.email=build-fixture@example.invalid",
            "-c", "commit.gpgsign=false", *args,
        ).stdout.strip()

    def build(self, *, env=None):
        result = self.run_command(
            "cargo", "build", "--locked", "--offline", "--message-format=json",
            env=env,
        )
        artifacts = [
            message for line in result.stdout.splitlines()
            if (message := json.loads(line)).get("reason") == "compiler-artifact"
            and "bin" in message["target"]["kind"]
        ]
        self.assertEqual(len(artifacts), 1, result.stdout)
        return artifacts[0], result.stderr

    def assert_build(self, fresh, revision, *, env=None):
        artifact, stderr = self.build(env=env)
        self.assertEqual(artifact["fresh"], fresh, stderr)
        self.assertEqual(
            self.run_command(artifact["executable"]).stdout.strip(), revision
        )

    def revision(self):
        return self.git("rev-parse", "--short=12", "HEAD")

    def test_consecutive_builds_reuse_artifacts_without_packed_refs(self):
        self.assertFalse((self.root / ".git/packed-refs").exists())
        artifact, _ = self.build()
        self.assertFalse(artifact["fresh"])
        diagnostic_env = self.env | {"CARGO_LOG": "cargo::core::compiler::fingerprint=info"}
        artifact, stderr = self.build(env=diagnostic_env)
        self.assertTrue(artifact["fresh"], stderr)

    def test_build_identity_does_not_rewrite_the_git_index(self):
        index = self.root / ".git/index"
        before = index.read_bytes(), index.stat().st_mtime_ns
        self.assert_build(False, self.revision())
        self.assertEqual((index.read_bytes(), index.stat().st_mtime_ns), before)
        self.assert_build(True, self.revision())

    def test_dirty_restore_staged_and_committed_sources_update_identity(self):
        revision = self.revision()
        self.assert_build(False, revision)
        source = self.root / "scenarios/fixture.txt"
        original = source.read_text()
        source.write_text(original + "Changed\n")
        self.assert_build(False, revision + "-dirty")
        self.assert_build(True, revision + "-dirty")
        source.write_text(original)
        self.assert_build(False, revision)
        source.write_text(original + "Staged\n")
        self.git("add", "scenarios/fixture.txt")
        self.assert_build(False, revision + "-dirty")
        self.git("commit", "-m", "Change tracked source")
        self.assert_build(False, self.revision())
        self.assert_build(True, self.revision())

    def test_packed_and_loose_branch_refs_both_reuse_and_update(self):
        self.git("pack-refs", "--all", "--prune")
        self.assertFalse((self.root / ".git/refs/heads/main").exists())
        self.assert_build(False, self.revision())
        self.assert_build(True, self.revision())
        self.git("commit", "--allow-empty", "-m", "Advance packed branch")
        self.assertTrue((self.root / ".git/refs/heads/main").exists())
        self.assert_build(False, self.revision())
        self.assert_build(True, self.revision())
        self.git("pack-refs", "--all", "--prune")
        self.assert_build(False, self.revision())
        self.assert_build(True, self.revision())

    def test_detached_head_reuses_and_updates(self):
        self.git("switch", "--detach")
        self.assert_build(False, self.revision())
        self.assert_build(True, self.revision())
        self.git("commit", "--allow-empty", "-m", "Advance detached HEAD")
        self.assert_build(False, self.revision())
        self.assert_build(True, self.revision())

    def test_linked_worktree_resolves_shared_and_per_worktree_metadata(self):
        linked = self.root / "linked-checkout"
        self.git("worktree", "add", "-b", "linked", str(linked))
        self.root = linked
        self.assertTrue((self.root / ".git").is_file())
        self.assert_build(False, self.revision())
        self.assert_build(True, self.revision())
        self.git("commit", "--allow-empty", "-m", "Advance linked worktree")
        self.assert_build(False, self.revision())
        self.assert_build(True, self.revision())

    def test_annotated_tag_updates_identity_without_source_changes(self):
        self.assert_build(False, self.revision())
        self.git("tag", "-a", "v0.0-fixture", "-m", "Build identity tag")
        self.assert_build(False, "v0.0-fixture")
        self.assert_build(True, "v0.0-fixture")

    def test_untracked_files_do_not_mark_identity_dirty(self):
        self.assert_build(False, self.revision())
        (self.root / "scenarios/untracked.txt").write_text("Not part of the build\n")
        artifact, _ = self.build()
        self.assertEqual(
            self.run_command(artifact["executable"]).stdout.strip(), self.revision()
        )
        self.assert_build(True, self.revision())

    def test_explicit_revision_override_updates_without_git_identity(self):
        env = self.env | {"SPACEWARS_BUILD_REVISION": "archive-build-a"}
        self.assert_build(False, "archive-build-a", env=env)
        self.assert_build(True, "archive-build-a", env=env)
        env["SPACEWARS_BUILD_REVISION"] = "archive-build-b"
        self.assert_build(False, "archive-build-b", env=env)
        self.assert_build(True, "archive-build-b", env=env)
        self.assert_build(False, self.revision())

    def test_archive_with_missing_optional_dirs_does_not_inherit_parent_git(self):
        archive = self.root / "archive"
        archive.mkdir()
        shutil.copytree(self.root / "crates", archive / "crates")
        for name in ["Cargo.toml", "Cargo.lock"]:
            shutil.copyfile(self.root / name, archive / name)
        self.root = archive
        self.assertFalse((self.root / ".git").exists())
        self.assert_build(False, "unknown (source archive)")
        self.assert_build(True, "unknown (source archive)")
        env = self.env | {"SPACEWARS_BUILD_REVISION": "archive-build"}
        self.assert_build(False, "archive-build", env=env)
        self.assert_build(True, "archive-build", env=env)


if __name__ == "__main__":
    unittest.main()
