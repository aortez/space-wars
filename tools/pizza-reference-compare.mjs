#!/usr/bin/env node
// CPU-only comparison against an unmodified allan.pizza checkout. See docs.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';

const [referenceArg, fixturePath, repeatsArg = '3', warmupsArg = '1'] = process.argv.slice(2);
if (!referenceArg || !fixturePath || process.argv.length > 6) {
  throw new Error('usage: pizza-reference-compare.mjs REFERENCE_CHECKOUT RUST_REPORT [REPEATS=3] [WARMUPS=1]');
}
const repeats = Number(repeatsArg);
const warmups = Number(warmupsArg);
assert(Number.isInteger(repeats) && repeats >= 1 && repeats <= 10);
assert(Number.isInteger(warmups) && warmups >= 0 && warmups <= 10);
const reference = await realpath(referenceArg);
const fixtureBytes = await readFile(fixturePath);
const fixture = JSON.parse(fixtureBytes);
assert.equal(fixture.fixture_version, 1);
assert.equal(fixture.ordinary, false, 'only exported mixed fixtures have matching initial bodies');
assert.equal(fixture.field, false, 'the reference has no matching directional event');
assert.notEqual(fixture.ccd_instrumented, true, 'query-instrumented Rust reports are not benchmarks');
assert(Number.isInteger(fixture.steps) && fixture.steps >= 1 && fixture.steps <= 3600);
assert.equal(fixture.fixed_dt_ns, 16_666_667);
assert.equal(fixture.initial_balls.length, fixture.initial.balls);
assert(fixture.initial_balls.length >= 64);
assert(fixture.initial_balls.every(b => b.moving && !b.invincible && b.hp > 0));
const git = (...args) => execFileSync('git', ['-C', reference, ...args], { encoding: 'utf8' }).trim();
assert.equal(git('status', '--porcelain', '--untracked-files=no'), '', 'reference tracked source must be clean');
const referenceCommit = git('rev-parse', 'HEAD');
const { chromium } = createRequire(path.join(reference, 'package.json'))('@playwright/test');

// Serve only reference JS modules, on loopback, without running its application
// loop or changing the checkout. Isolation headers retain fine timer resolution.
const scriptRoot = path.join(reference, 'src', 'scripts');
const server = createServer(async (request, response) => {
  response.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  response.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
  try {
    if (request.url === '/') {
      response.setHeader('Content-Type', 'text/html');
      response.end('<!doctype html><title>Pizza physics comparison</title>');
      return;
    }
    assert(request.url.startsWith('/src/scripts/') && request.url.endsWith('.js'));
    const file = await realpath(path.resolve(reference, `.${request.url}`));
    assert(file.startsWith(`${scriptRoot}${path.sep}`));
    response.setHeader('Content-Type', 'text/javascript');
    response.end(await readFile(file));
  } catch {
    response.writeHead(404);
    response.end();
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
let browser;
try {
  browser = await chromium.launch({
    headless: true,
    executablePath: process.env.PIZZA_CHROME || '/usr/bin/google-chrome',
  });
  const origin = `http://127.0.0.1:${server.address().port}`;
  const page = await browser.newPage({ viewport: { width: fixture.width, height: fixture.height } });
  await page.route('**/*', route => new URL(route.request().url()).origin === origin
    ? route.continue() : route.abort());
  await page.goto(origin);
  const result = await page.evaluate(async ({ fixture, repeats, warmups }) => {
    const { World } = await import('/src/scripts/world.js');
    const { Ball } = await import('/src/scripts/ball.js');
    const { vec3 } = await import('/src/scripts/vec3.js');
    const canvas = { width: fixture.width, height: fixture.height };
    // Match Rust's f32 timestep multiplication before converting to JS Number.
    const dt = Math.fround(Math.fround(fixture.fixed_dt_ns / 1e9) * Math.fround(0.9));
    const runs = [];
    const originalRandom = Math.random;
    let settings;
    try {
      for (let repeat = -warmups; repeat < repeats; repeat++) {
        let seed = fixture.seed >>> 0;
        // Deterministic reference-only RNG; deliberately not called the Rust RNG.
        Math.random = () => {
          let value = seed += 0x6d2b79f5;
          value = Math.imul(value ^ value >>> 15, value | 1);
          value ^= value + Math.imul(value ^ value >>> 7, value | 61);
          return ((value ^ value >>> 14) >>> 0) / 4294967296;
        };
        const world = new World();
        world.max_balls = fixture.initial.limit;
        world.desiredBallCount = 0;
        world.shouldDrawBackground = false;
        world.balls = fixture.initial_balls.map(item => {
          const f = Math.fround;
          const ball = new Ball(f(item.x), f(item.y), f(item.radius), new vec3(...item.color.map(c => f(c) * 255)));
          ball.id = item.id;
          ball.v.x = f(item.vx);
          ball.v.y = f(item.vy);
          ball.hp = f(item.hp);
          ball.is_moving = item.moving;
          ball.is_invincible = item.invincible;
          return ball;
        });
        settings = {
          dt, maxBalls: world.max_balls, maxParticles: world.max_particles,
          gravityMode: world.gravityMode, gravityImplementation: world.gravityImplementation,
          theta: world.barnesHutTheta, gravity: world.g, softening: world.gravitySoftening,
          explosionSizeFactor: world.EXPLODER_SIZE_FACTOR,
        };
        const samples = [];
        for (let tick = 1; tick <= fixture.steps; tick++) {
          const started = performance.now();
          world.advance(dt, canvas);
          const stepMs = performance.now() - started;
          // Capture outside the measured interval, including every real death.
          samples.push({
            tick, stepMs, balls: world.balls.length, particles: world.particles.length,
            ...world.lastPhysicsBreakdown,
          });
        }
        if (repeat >= 0) {
          runs.push({ samples, finalState: world.balls.map(b => [b.center.x, b.center.y, b.v.x, b.v.y, b.r, b.hp]) });
        }
      }
    } finally {
      Math.random = originalRandom;
    }
    return { settings, timerIsolated: crossOriginIsolated, runs };
  }, { fixture, repeats, warmups });
  // Different engines need not match. Repeated JS runs must reproduce work.
  for (const run of result.runs.slice(1)) {
    assert.deepEqual(run.finalState, result.runs[0].finalState);
    assert.deepEqual(run.samples.map(s => [s.balls, s.particles, s.collisions, s.lifecycle]),
      result.runs[0].samples.map(s => [s.balls, s.particles, s.collisions, s.lifecycle]));
  }
  for (const run of result.runs) {
    run.finalStateSha256 = createHash('sha256').update(JSON.stringify(run.finalState)).digest('hex');
    delete run.finalState;
  }
  console.log(JSON.stringify({
    schema: 1,
    scope: 'CPU-only World.advance in Chrome; no rendering, frame pacing, or FPS claim. Same initial bodies and dt; different solvers, damage, fragments, RNG and particles.',
    referenceCommit, chrome: browser.version(),
    host: { cpu: os.cpus()[0].model, platform: os.platform(), arch: os.arch() },
    fixtureSha256: createHash('sha256').update(fixtureBytes).digest('hex'),
    width: fixture.width, height: fixture.height, initial: fixture.initial,
    steps: fixture.steps, repeats, warmups, ...result,
  }));
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
