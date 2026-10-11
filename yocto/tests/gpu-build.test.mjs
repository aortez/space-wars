import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { defaultBuildDir, gpuBuildEnabled, YOCTO_DIR } from '../scripts/paths.mjs';

test('normal builds and updates use the GPU bundle; software stays separate', () => {
  const software = defaultBuildDir({ gpu: false, env: {} });
  assert.equal(defaultBuildDir({ env: {} }), `${software}-gpu`);
  assert.equal(defaultBuildDir({ env: { SPACEWARS_GPU: '0' } }), software);
  for (const gpu of [true, false]) {
    assert.equal(defaultBuildDir({ gpu, env: { KAS_BUILD_DIR: '/explicit/build' } }), '/explicit/build');
  }
});

test('build selection defaults to GPU and rejects conflicting switches', () => {
  assert.equal(gpuBuildEnabled([], {}), true);
  assert.equal(gpuBuildEnabled([], { SPACEWARS_GPU: '0' }), false);
  assert.equal(gpuBuildEnabled(['--gpu'], { SPACEWARS_GPU: '0' }), true);
  assert.equal(gpuBuildEnabled(['--software'], {}), false);
  assert.throws(() => gpuBuildEnabled(['--gpu', '--software']), /not both/);
});

test('normal image includes GPU libraries and boots the build-selected renderer', () => {
  const base = readFileSync(join(YOCTO_DIR, 'kas-spacewars.yml'), 'utf8');
  const gpu = readFileSync(join(YOCTO_DIR, 'kas-spacewars-gpu.yml'), 'utf8');
  const software = readFileSync(join(YOCTO_DIR, 'kas-spacewars-software.yml'), 'utf8');
  const service = readFileSync(join(YOCTO_DIR,
    'meta-spacewars/recipes-spacewars/spacewars/files/spacewars-kiosk.service'), 'utf8');
  assert.match(base, /SPACEWARS_GPU\s*=\s*"1"/);
  assert.match(base, /PACKAGECONFIG:pn-mesa = "gallium opengl egl gles gbm"/);
  assert.match(gpu, /kas-spacewars\.yml/);
  assert.match(software, /SPACEWARS_GPU\s*=\s*"0"/);
  assert.match(software, /DISTRO_FEATURES:remove = "opengl"/);
  assert.match(service, /^Environment=SLINT_BACKEND=linuxkms$/m);
  assert.match(service, /^EnvironmentFile=-\/boot\/spacewars-kiosk\.env$/m);
});

test('shader-only edits invalidate the Pi application build', () => {
  const recipe = readFileSync(join(YOCTO_DIR,
    'meta-spacewars/recipes-spacewars/spacewars/spacewars_git.bb'), 'utf8');
  assert.match(recipe, /name\.endswith\(\([^\n]*"\.glsl"[^\n]*"\.wgsl"/);
  assert.match(recipe, /d\.appendVarFlag\("do_compile", "file-checksums"/);
});
