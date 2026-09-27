#!/usr/bin/env node
// Only Cargo decides which files to delete. Never clean the entire target tree:
// it also contains experiment evidence and binaries pinned by those experiments.
import { spawnSync } from 'node:child_process';
import {
  existsSync, lstatSync, readdirSync, realpathSync, renameSync, statfsSync,
  statSync, writeFileSync,
} from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { REPO_DIR, defaultBuildDir } from './paths.mjs';

const GIB = 1024 ** 3;
export const DEFAULT_POLICY = { minFreeGiB: 40, maxProfileGiB: 60 };

function inside(path, root) {
  const suffix = relative(root, path);
  return suffix !== '' && suffix !== '..' && !suffix.startsWith(`..${sep}`)
    && !isAbsolute(suffix);
}

function existingAncestor(path) {
  let current = resolve(path);
  while (!existsSync(current)) current = dirname(current);
  return current;
}

function freeBytes(path) {
  const stats = statfsSync(existingAncestor(path));
  return stats.bavail * stats.bsize;
}

// Count allocated space once for hard links, like du; do not follow symlinks.
// Refuse unusual layouts instead of guessing which tree Cargo would touch.
function profileBytes(profile) {
  const pending = [profile];
  const seen = new Set();
  let bytes = 0;
  while (pending.length) {
    const path = pending.pop();
    const stats = lstatSync(path);
    if (stats.isSymbolicLink()) throw new Error(`Refusing symlink in build cache: ${path}`);
    const inode = `${stats.dev}:${stats.ino}`;
    if (seen.has(inode)) continue;
    seen.add(inode);
    bytes += stats.blocks * 512;
    if (stats.isDirectory()) {
      for (const name of readdirSync(path)) pending.push(join(path, name));
    }
  }
  return bytes;
}

function cargo(args, repo) {
  const result = spawnSync('cargo', args, {
    cwd: repo, encoding: 'utf8', timeout: 120_000, maxBuffer: 64 * 1024 ** 2,
    env: { ...process.env, CARGO_TERM_COLOR: 'never' },
  });
  if (result.error || result.status !== 0) {
    throw new Error(`Cargo cache maintenance failed: ${result.error?.message ?? result.stderr.trim()}`);
  }
  return result;
}

export function validatePreview(output, profile) {
  const paths = output.split(/\r?\n/).filter(line => line.length);
  for (const path of paths) {
    if (!isAbsolute(path) || !inside(resolve(path), profile)) {
      throw new Error(`Cargo proposed a path outside the managed debug profile: ${path}`);
    }
  }
  return paths.length;
}

export function planReason(free, size, { minFreeGiB, maxProfileGiB }) {
  if (free < minFreeGiB * GIB) return 'low disk space';
  if (size > maxProfileGiB * GIB) return 'debug profile exceeds budget';
  return null;
}

export function maintainBuildCache({
  repo = REPO_DIR, buildDir = defaultBuildDir(), apply = false,
  policy = DEFAULT_POLICY, runCargo = cargo,
} = {}) {
  for (const value of [policy.minFreeGiB, policy.maxProfileGiB]) {
    if (!Number.isFinite(value) || value < 0) throw new Error('Cache limits must be finite and nonnegative');
  }
  repo = realpathSync(repo);
  const target = join(repo, 'target');
  const profile = join(target, 'debug');
  const report = {
    version: 1, repo, profile, apply, policy, freeBefore: freeBytes(buildDir),
    profileBefore: 0, action: 'none', reason: null,
  };
  if (existsSync(join(target, '.spacewars-cache-keep'))) {
    report.reason = 'cache is pinned by target/.spacewars-cache-keep';
    return report;
  }
  if (!existsSync(profile)) {
    report.reason = 'no local debug profile';
    return report;
  }
  if (realpathSync(target) !== target || realpathSync(profile) !== profile) {
    throw new Error('Refusing a relocated/symlinked target or debug directory');
  }
  report.profileBefore = profileBytes(profile);
  report.reason = planReason(report.freeBefore, report.profileBefore, policy);
  if (!report.reason) return report;
  if (statSync(target).dev !== statSync(existingAncestor(buildDir)).dev) {
    report.reason = 'build and cache are on different filesystems; no automatic cleanup';
    return report;
  }

  // Explicit paths override CARGO_TARGET_DIR and newer separate build-dir
  // configuration. Preview validation also rejects configured cross targets.
  const common = [
    '--config', `build.target-dir=${JSON.stringify(target)}`,
    '--config', `build.build-dir=${JSON.stringify(target)}`,
  ];
  const metadata = JSON.parse(runCargo([
    ...common, 'metadata', '--manifest-path', join(repo, 'Cargo.toml'),
    '--no-deps', '--format-version', '1', '--locked', '--offline',
  ], repo).stdout);
  if (realpathSync(metadata.workspace_root) !== repo) throw new Error('Expected this checkout to be the workspace root');
  const members = new Set(metadata.workspace_members);
  const names = metadata.packages.filter(pkg => members.has(pkg.id)).map(pkg => pkg.name).sort();
  if (!names.length || names.length !== members.size) throw new Error('No complete workspace package selection');
  // -p is supported by the project's Rust 1.89 minimum; clean --workspace is
  // newer. Package selection also makes Cargo take its build-directory lock.
  const args = [
    ...common, 'clean', '--manifest-path', join(repo, 'Cargo.toml'),
    '--profile', 'dev', '--locked', '--offline',
    ...names.flatMap(name => ['--package', name]),
  ];
  const preview = runCargo([...args, '--dry-run', '--verbose'], repo);
  report.paths = validatePreview(preview.stdout, profile);
  report.packages = names;
  report.preview = preview.stderr.trim().slice(-8192);
  report.action = report.paths ? 'would-clean' : 'none';
  if (!apply || !report.paths) return report;

  // Recheck the allowlisted tree after the preview. Cargo reacquires its own
  // lock for the actual package-scoped clean, waiting for active builds.
  if (realpathSync(target) !== target || realpathSync(profile) !== profile) {
    throw new Error('Build-cache path changed after preview');
  }
  if (existsSync(join(target, '.spacewars-cache-keep'))) throw new Error('Cache was pinned after preview');
  profileBytes(profile);
  report.result = runCargo(args, repo).stderr.trim().slice(-8192);
  report.profileAfter = profileBytes(profile);
  report.freeAfter = freeBytes(buildDir);
  report.action = 'cleaned';
  report.completedAt = new Date().toISOString();
  // One bounded audit record, not another unbounded history directory.
  const pending = join(target, `.build-cache-maintenance-${process.pid}.json`);
  writeFileSync(pending, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  renameSync(pending, join(target, 'build-cache-maintenance.json'));
  return report;
}

export function describe(report) {
  const gib = value => (value / GIB).toFixed(1);
  const lines = [
    `Build cache: ${report.profile}`,
    `Free: ${gib(report.freeBefore)} GiB; local debug profile: ${gib(report.profileBefore)} GiB`,
    `Policy: ${report.policy.minFreeGiB} GiB free-space floor; ${report.policy.maxProfileGiB} GiB debug budget`,
    `${report.action}: ${report.reason ?? 'within budget'}`,
  ];
  if (report.preview) lines.push(report.preview);
  if (report.action === 'cleaned') {
    lines.push(report.result,
      `Profile shrank by ${gib(Math.max(0, report.profileBefore - report.profileAfter))} GiB; free now ${gib(report.freeAfter)} GiB.`);
  } else if (report.action === 'would-clean') {
    lines.push('Dry run only. Use --apply to clean these workspace packages.');
  }
  lines.push('Third-party dependencies, release/CI builds, experiments, and Yocto data are not cleanup targets.');
  return lines.join('\n');
}

export function preflightMode(value = 'auto') {
  if (!['auto', 'report', 'off'].includes(value)) {
    throw new Error('SPACEWARS_BUILD_CACHE must be auto, report, or off');
  }
  return value;
}

export function buildCachePreflight(options = {}) {
  const mode = preflightMode(options.mode ?? process.env.SPACEWARS_BUILD_CACHE);
  if (mode === 'off') return;
  const report = maintainBuildCache({ ...options, apply: mode === 'auto' });
  console.log(describe(report));
}

function main(args) {
  const policy = { ...DEFAULT_POLICY };
  let apply = false;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--help' || args[i] === '-h') {
      console.log('Usage: node yocto/scripts/build-cache.mjs [--apply] [--min-free-gib 40] [--max-profile-gib 60]\nDefaults to dry-run. Cleans only this checkout\'s workspace packages in target/debug.');
      return;
    }
    if (args[i] === '--apply') apply = true;
    else if (args[i] === '--min-free-gib' || args[i] === '--max-profile-gib') {
      const key = args[i] === '--min-free-gib' ? 'minFreeGiB' : 'maxProfileGiB';
      const value = args[++i];
      if (!value || !Number.isFinite(Number(value)) || Number(value) < 0) throw new Error('Expected a nonnegative GiB limit');
      policy[key] = Number(value);
    } else throw new Error(`Unknown option: ${args[i]}`);
  }
  console.log(describe(maintainBuildCache({ apply, policy })));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(process.argv.slice(2)); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
