import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import {
  existsSync, linkSync, mkdirSync, mkdtempSync, readFileSync, rmSync,
  statSync, symlinkSync, unlinkSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import {
  buildCachePreflight, maintainBuildCache, planReason, preflightMode, validatePreview,
} from '../scripts/build-cache.mjs';

const GIB = 1024 ** 3;
const force = { minFreeGiB: 0, maxProfileGiB: 0 };
const available = { minFreeGiB: 0, maxProfileGiB: 1000 };

function fixture(t) {
  const repo = mkdtempSync(join(tmpdir(), 'spacewars-cache-test-'));
  t.after(() => rmSync(repo, { recursive: true, force: true }));
  const profile = join(repo, 'target/debug');
  mkdirSync(profile, { recursive: true });
  writeFileSync(join(profile, 'fixture'), 'cache contents');
  writeFileSync(join(repo, 'Cargo.toml'), '[workspace]\nmembers = []\n');
  return { repo, profile, buildDir: join(repo, 'not-created/build'), policy: force };
}

function fakeCargo(ctx, calls, { preview, metadata, apply } = {}) {
  return (args, repo) => {
    calls.push(args);
    assert.equal(repo, ctx.repo);
    if (args.includes('metadata')) {
      return { stdout: JSON.stringify(metadata ?? {
        workspace_root: repo, workspace_members: ['local-id'],
        packages: [{ id: 'local-id', name: 'local' }, { id: 'dep-id', name: 'dependency' }],
      }) };
    }
    assert.ok(args.includes('--package'));
    assert.ok(args.includes('local'));
    assert.ok(!args.includes('dependency'));
    assert.ok(args.includes('--offline') && args.includes('--locked'));
    if (args.includes('--dry-run')) {
      return { stdout: preview ?? `${join(ctx.profile, 'fixture')}\n`, stderr: 'Summary of fixture' };
    }
    if (apply) apply();
    else rmSync(join(ctx.profile, 'fixture'));
    return { stdout: '', stderr: 'Removed fixture' };
  };
}

test('pressure and budget policy has explicit boundaries', () => {
  const policy = { minFreeGiB: 40, maxProfileGiB: 60 };
  assert.equal(planReason(40 * GIB, 60 * GIB, policy), null);
  assert.equal(planReason(39 * GIB, 1, policy), 'low disk space');
  assert.equal(planReason(100 * GIB, 61 * GIB, policy), 'debug profile exceeds budget');
  assert.equal(preflightMode(), 'auto');
  for (const mode of ['auto', 'report', 'off']) assert.equal(preflightMode(mode), mode);
  assert.throws(() => preflightMode('yes'), /auto, report, or off/);
});

test('healthy or missing profiles never invoke Cargo', t => {
  const ctx = fixture(t);
  const never = () => assert.fail('Cargo must not run');
  assert.equal(maintainBuildCache({ ...ctx, policy: available, runCargo: never }).action, 'none');
  rmSync(ctx.profile, { recursive: true });
  assert.equal(maintainBuildCache({ ...ctx, runCargo: never }).reason, 'no local debug profile');
  buildCachePreflight({ mode: 'off', repo: '/not/a/repo', runCargo: never });
});

test('default dry run selects workspace packages but preserves every file', t => {
  const ctx = fixture(t);
  const calls = [];
  const report = maintainBuildCache({ ...ctx, runCargo: fakeCargo(ctx, calls) });
  assert.equal(report.action, 'would-clean');
  assert.equal(report.paths, 1);
  assert.deepEqual(report.packages, ['local']);
  assert.equal(calls.length, 2);
  assert.ok(calls[1].includes('--dry-run'));
  assert.ok(existsSync(join(ctx.profile, 'fixture')));
  assert.ok(!existsSync(join(ctx.repo, 'target/build-cache-maintenance.json')));
});

test('apply delegates deletion to Cargo and writes one audit report', t => {
  const ctx = fixture(t);
  const calls = [];
  const evidence = join(ctx.repo, 'target/experiment');
  writeFileSync(evidence, 'irreplaceable raw recording');
  const report = maintainBuildCache({ ...ctx, apply: true, runCargo: fakeCargo(ctx, calls) });
  assert.equal(report.action, 'cleaned');
  assert.ok(report.profileAfter < report.profileBefore);
  assert.ok(!calls[2].includes('--dry-run'));
  assert.equal(readFileSync(evidence, 'utf8'), 'irreplaceable raw recording');
  assert.deepEqual(JSON.parse(readFileSync(join(ctx.repo, 'target/build-cache-maintenance.json'))), report);
});

test('empty package selection can never become a whole-target clean', t => {
  const ctx = fixture(t);
  for (const members of [[], ['missing']]) {
    const calls = [];
    assert.throws(() => maintainBuildCache({ ...ctx, apply: true,
      runCargo: fakeCargo(ctx, calls, { metadata: {
        workspace_root: ctx.repo, workspace_members: members, packages: [],
      } }),
    }), /workspace package selection/);
    assert.equal(calls.length, 1);
    assert.ok(existsSync(join(ctx.profile, 'fixture')));
  }
});

test('unexpected workspace, preview path, and Cargo failure prevent apply', t => {
  const ctx = fixture(t);
  for (const path of [ctx.profile, `${ctx.profile}-other/file`, join(ctx.repo, 'target/experiment'),
    join(ctx.profile, '../release/program'), '/outside/file', 'relative/file']) {
    const calls = [];
    assert.throws(() => maintainBuildCache({ ...ctx, apply: true,
      runCargo: fakeCargo(ctx, calls, { preview: `${path}\n` }),
    }), /outside the managed debug profile/);
    assert.equal(calls.length, 2);
  }
  assert.equal(validatePreview('', ctx.profile), 0);
  assert.throws(() => maintainBuildCache({ ...ctx, apply: true,
    runCargo: () => { throw new Error('Cargo failed'); },
  }), /Cargo failed/);
  assert.throws(() => maintainBuildCache({ ...ctx, apply: true,
    runCargo: fakeCargo(ctx, [], { metadata: { workspace_root: dirname(ctx.repo) } }),
  }), /workspace root/);
  assert.ok(existsSync(join(ctx.profile, 'fixture')));
});

test('pin opts out of automatic cleanup', t => {
  const ctx = fixture(t);
  writeFileSync(join(ctx.repo, 'target/.spacewars-cache-keep'), '');
  const report = maintainBuildCache({ ...ctx, apply: true,
    runCargo: () => assert.fail('pinned cache must not invoke Cargo'),
  });
  assert.equal(report.action, 'none');
  assert.match(report.reason, /pinned/);
});

test('pre-build report mode is non-destructive and auto applies the same policy', t => {
  const ctx = fixture(t);
  const calls = [];
  t.mock.method(console, 'log', () => {});
  const runCargo = fakeCargo(ctx, calls);
  buildCachePreflight({ ...ctx, mode: 'report', runCargo });
  assert.equal(calls.length, 2);
  assert.ok(existsSync(join(ctx.profile, 'fixture')));
  buildCachePreflight({ ...ctx, mode: 'auto', runCargo });
  assert.equal(calls.length, 5);
  assert.ok(!existsSync(join(ctx.profile, 'fixture')));
});

test('a different build filesystem never causes unrelated cache cleanup', t => {
  const ctx = fixture(t);
  if (!existsSync('/dev/shm') || statSync('/dev/shm').dev === statSync(ctx.repo).dev) {
    t.skip('needs a second filesystem');
    return;
  }
  const report = maintainBuildCache({ ...ctx, apply: true, buildDir: '/dev/shm',
    runCargo: () => assert.fail('different filesystem must not invoke Cargo'),
  });
  assert.equal(report.action, 'none');
  assert.match(report.reason, /different filesystems/);
});

test('help and update dry-run bypass even an invalid maintenance configuration', () => {
  for (const args of [
    ['yocto/scripts/build.mjs', '--help'],
    ['yocto/scripts/update.mjs', '--fast', '--dry-run'],
  ]) {
    const result = spawnSync(process.execPath, args, {
      cwd: new URL('../..', import.meta.url), encoding: 'utf8',
      env: { ...process.env, SPACEWARS_BUILD_CACHE: 'invalid' },
    });
    assert.equal(result.status, 0, result.stderr);
    assert.ok(!result.stdout.includes('Build cache:'));
  }
});

test('reject symlinks, including a relocated target and a pin introduced during preview', t => {
  const ctx = fixture(t);
  const link = join(ctx.profile, 'escape');
  symlinkSync(ctx.repo, link);
  assert.throws(() => maintainBuildCache(ctx), /Refusing symlink/);
  unlinkSync(link);
  const calls = [];
  const fake = fakeCargo(ctx, calls);
  assert.throws(() => maintainBuildCache({ ...ctx, apply: true, runCargo: (args, repo) => {
    const result = fake(args, repo);
    if (args.includes('--dry-run')) writeFileSync(join(ctx.repo, 'target/.spacewars-cache-keep'), '');
    return result;
  } }), /pinned after preview/);
  assert.equal(calls.length, 2);
  rmSync(join(ctx.repo, 'target/.spacewars-cache-keep'));
  rmSync(ctx.profile, { recursive: true });
  symlinkSync(ctx.repo, ctx.profile);
  assert.throws(() => maintainBuildCache(ctx), /relocated\/symlinked/);
});

test('allocated-byte count deduplicates hard links and invalid limits fail closed', t => {
  const ctx = fixture(t);
  const before = maintainBuildCache({ ...ctx, policy: available }).profileBefore;
  linkSync(join(ctx.profile, 'fixture'), join(ctx.profile, 'alias'));
  assert.equal(maintainBuildCache({ ...ctx, policy: available }).profileBefore, before);
  for (const value of [NaN, Infinity, -1]) {
    assert.throws(() => maintainBuildCache({ ...ctx, policy: { ...force, minFreeGiB: value } }), /finite/);
  }
});

const cargoAvailable = spawnSync('cargo', ['--version']).status === 0;
test('real Cargo preserves dependencies/evidence and honors the build lock', {
  skip: !cargoAvailable || process.platform !== 'linux', timeout: 30_000,
}, async t => {
  const ctx = fixture(t);
  mkdirSync(join(ctx.repo, 'app/src'), { recursive: true });
  mkdirSync(join(ctx.repo, 'helper/src'), { recursive: true });
  writeFileSync(join(ctx.repo, 'Cargo.toml'), '[workspace]\nmembers=["app"]\nexclude=["helper"]\nresolver="2"\n');
  writeFileSync(join(ctx.repo, 'app/Cargo.toml'), '[package]\nname="cache-fixture"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nhelper={path="../helper"}\n');
  writeFileSync(join(ctx.repo, 'app/src/main.rs'), 'fn main() { println!("{}", helper::value()); }\n');
  writeFileSync(join(ctx.repo, 'helper/Cargo.toml'), '[package]\nname="helper"\nversion="0.1.0"\nedition="2021"\n');
  writeFileSync(join(ctx.repo, 'helper/src/lib.rs'), 'pub fn value() -> u32 { 42 }\n');
  const build = spawnSync('cargo', ['build', '--offline', '--manifest-path', join(ctx.repo, 'Cargo.toml')],
    { cwd: ctx.repo, encoding: 'utf8', env: { ...process.env, CARGO_TARGET_DIR: join(ctx.repo, 'target') } });
  assert.equal(build.status, 0, build.stderr);
  const binary = join(ctx.profile, 'cache-fixture');
  const evidence = join(ctx.repo, 'target/experiment.jsonl');
  writeFileSync(evidence, 'must survive');
  const preview = maintainBuildCache(ctx);
  assert.equal(preview.action, 'would-clean');
  assert.ok(existsSync(binary));

  // Cargo's package-scoped clean waits for this real native Cargo lock. Unlike
  // deleting directories ourselves, it cannot race a Cargo build holding it.
  const locker = spawn('python3', ['-u', '-c',
    'import fcntl,sys; f=open(sys.argv[1],"a"); fcntl.flock(f,fcntl.LOCK_EX); print("locked",flush=True); sys.stdin.read()',
    join(ctx.profile, '.cargo-lock')], { stdio: ['pipe', 'pipe', 'pipe'] });
  t.after(() => locker.kill());
  await once(locker.stdout, 'data');
  const cleaner = spawn('cargo', ['clean', '--profile', 'dev', '--package', 'cache-fixture', '--offline'],
    { cwd: ctx.repo, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, CARGO_TARGET_DIR: join(ctx.repo, 'target') } });
  t.after(() => cleaner.kill());
  let waiting = false;
  for await (const chunk of cleaner.stderr) {
    if (chunk.toString().includes('Blocking')) { waiting = true; break; }
  }
  assert.ok(waiting, 'Cargo clean must wait for the held build lock');
  assert.ok(existsSync(binary));
  // Stop the direct probe, release the lock, then test the actual wrapper.
  const stopped = once(cleaner, 'exit');
  cleaner.kill();
  await stopped;
  locker.stdin.end();
  await once(locker, 'exit');
  const result = maintainBuildCache({ ...ctx, apply: true });
  assert.equal(result.action, 'cleaned');
  assert.ok(!existsSync(binary));
  assert.equal(readFileSync(evidence, 'utf8'), 'must survive');
  assert.equal(readFileSync(join(ctx.profile, 'fixture'), 'utf8'), 'cache contents');
  const check = spawnSync('cargo', ['build', '--offline', '--message-format=json'],
    { cwd: ctx.repo, encoding: 'utf8', env: { ...process.env, CARGO_TARGET_DIR: join(ctx.repo, 'target') } });
  assert.equal(check.status, 0, check.stderr);
  const helper = check.stdout.trim().split('\n').map(line => JSON.parse(line))
    .find(message => message.reason === 'compiler-artifact' && message.target.name === 'helper');
  assert.equal(helper.fresh, true, 'third-party dependency was reused, not cleaned');
});
