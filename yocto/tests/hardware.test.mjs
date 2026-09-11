import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { assertBootCompatibility, deviceProfileText, readBootIdentity, validateProfile } from '../scripts/hardware.mjs';
import { defaultBuildDir, defaultImageDir, MACHINE, preferredImages, YOCTO_DIR } from '../scripts/paths.mjs';

const hardwareFiles = join(YOCTO_DIR, 'meta-spacewars/recipes-spacewars/spacewars-hardware/files');

function fixture(t, profile) {
  const root = mkdtempSync(join(tmpdir(), 'spacewars-hardware-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const boot = join(root, 'boot');
  const sys = join(root, 'sys');
  const run = join(root, 'run');
  mkdirSync(boot);
  mkdirSync(sys);
  mkdirSync(join(sys, 'firmware/devicetree/base'), { recursive: true });
  writeFileSync(join(sys, 'firmware/devicetree/base/model'), 'Raspberry Pi 4 Model B Rev 1.5\0');
  if (profile) writeFileSync(join(boot, 'spacewars-device.txt'), deviceProfileText(profile));
  return {
    root, boot, sys, run,
    launch() {
      return spawnSync('sh', [join(hardwareFiles, 'spacewars-hardware.sh')], {
        encoding: 'utf8',
        env: { ...process.env, SPACEWARS_BOOT_DIR: boot, SPACEWARS_SYS_DIR: sys, SPACEWARS_HARDWARE_RUN_DIR: run },
      });
    },
    environment() { return readFileSync(join(run, 'hardware.env'), 'utf8'); },
  };
}

test('build and artifact paths isolate the unified machine from older checkouts', () => {
  assert.match(readFileSync(join(YOCTO_DIR, 'kas-spacewars.yml'), 'utf8'), new RegExp(`machine: ${MACHINE}\\n`));
  if (!process.env.KAS_BUILD_DIR) assert.ok(defaultBuildDir().endsWith(`-yocto-build-${MACHINE}`));
  assert.equal(defaultImageDir(), join(defaultBuildDir(), 'tmp/deploy/images', MACHINE));
  assert.equal(preferredImages('wic.gz')[0], `spacewars-image-${MACHINE}.rootfs.wic.gz`);
});

test('only explicit known hardware profiles can be flashed', () => {
  for (const name of ['hdmi', 'picade', 'hyperpixel']) {
    assert.equal(validateProfile(name), name);
    assert.ok(deviceProfileText(name).includes(`include spacewars-profiles/${name}.txt`));
  }
  for (const name of [null, '', 'pi4', '../picade', 'picade\ninclude other.txt']) {
    assert.throws(() => validateProfile(name), /Select --profile/);
  }
});

test('HDMI selection uses the connected connector, not the first DRM card', t => {
  const f = fixture(t, 'hdmi');
  for (const [name, status] of [['card1-HDMI-A-1', 'disconnected'], ['card1-HDMI-A-2', 'connected']]) {
    const connector = join(f.sys, 'class/drm', name);
    mkdirSync(connector, { recursive: true });
    writeFileSync(join(connector, 'status'), `${status}\n`);
  }
  const result = f.launch();
  assert.equal(result.status, 0, result.stderr);
  assert.match(f.environment(), /SLINT_DRM_OUTPUT=HDMI-A-2\n/);
  assert.match(f.environment(), /SLINT_KMS_ROTATION=0\n/);
  assert.match(f.environment(), /ALSA_CARD=vc4hdmi1\n/);
  assert.doesNotMatch(f.environment(), /DPI-1/);
});

test('missing HDMI detection leaves output selection to Slint', t => {
  const f = fixture(t, 'hdmi');
  assert.equal(f.launch().status, 0);
  assert.doesNotMatch(f.environment(), /SLINT_DRM_OUTPUT|ALSA_CARD/);
});

test('Picade routes audio to the HAT without touching backlight files', t => {
  const f = fixture(t, 'picade');
  const backlight = join(f.sys, 'class/backlight/backlight');
  mkdirSync(backlight, { recursive: true });
  writeFileSync(join(backlight, 'bl_power'), '4\n');
  assert.equal(f.launch().status, 0);
  assert.match(f.environment(), /ALSA_CARD=sndrpihifiberry\n/);
  assert.equal(readFileSync(join(backlight, 'bl_power'), 'utf8'), '4\n');
});

test('HyperPixel retains its existing rotation, USB audio and backlight workaround', t => {
  const f = fixture(t, 'hyperpixel');
  const backlight = join(f.sys, 'class/backlight/backlight');
  mkdirSync(backlight, { recursive: true });
  writeFileSync(join(backlight, 'bl_power'), '4\n');
  writeFileSync(join(backlight, 'brightness'), '0\n');
  assert.equal(f.launch().status, 0);
  assert.match(f.environment(), /SLINT_DRM_OUTPUT=DPI-1\nSLINT_KMS_ROTATION=90\nALSA_CARD=Audio\n/);
  assert.equal(readFileSync(join(backlight, 'bl_power'), 'utf8'), '0\n');
  assert.equal(readFileSync(join(backlight, 'brightness'), 'utf8'), '1\n');
});

test('Picade refuses unsupported boards instead of silently starting without its HAT', t => {
  const f = fixture(t, 'picade');
  writeFileSync(join(f.sys, 'firmware/devicetree/base/model'), 'Raspberry Pi 5 Model B Rev 1.0\0');
  const result = f.launch();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /currently targets Pi 4/);
});

test('missing, invalid and ambiguous selections fail without executing boot-file content', t => {
  const f = fixture(t);
  assert.notEqual(f.launch().status, 0);
  writeFileSync(join(f.boot, 'spacewars-device.txt'), 'include spacewars-profiles/unknown.txt\n');
  assert.notEqual(f.launch().status, 0);
  writeFileSync(join(f.boot, 'spacewars-device.txt'), deviceProfileText('hdmi') + deviceProfileText('picade'));
  assert.notEqual(f.launch().status, 0);
  writeFileSync(join(f.boot, 'spacewars-device.txt'), 'exit 99\n' + deviceProfileText('hdmi'));
  assert.equal(f.launch().status, 1);
  writeFileSync(join(f.boot, 'spacewars-device.txt'), '[pi4]\ninclude spacewars-profiles/hdmi.txt\n');
  assert.equal(f.launch().status, 1);
});

test('Windows line endings are accepted in boot profile selection', t => {
  const f = fixture(t, 'picade');
  writeFileSync(join(f.boot, 'spacewars-device.txt'), deviceProfileText('picade').replaceAll('\n', '\r\n'));
  assert.equal(f.launch().status, 0);
});

test('rootfs-only updates require valid, identical boot identities', t => {
  const f = fixture(t);
  const image = join(f.root, 'image.ext4.gz');
  const identity = 'a'.repeat(64);
  assert.throws(() => readBootIdentity(image), /Missing/);
  writeFileSync(`${image}.boot-id`, 'invalid\n');
  assert.throws(() => readBootIdentity(image), /Invalid/);
  writeFileSync(`${image}.boot-id`, `${identity}\n`);
  assert.equal(readBootIdentity(image), identity);
  assert.doesNotThrow(() => assertBootCompatibility(identity, `${identity}\n`));
  for (const actual of [null, '', 'b'.repeat(64), `${identity}\nextra`]) {
    assert.throws(() => assertBootCompatibility(identity, actual), /Flash the full/);
  }
  assert.throws(() => assertBootCompatibility('', ''), /Flash the full/);
});

test('profile files keep HAT and HyperPixel GPIO overlays mutually exclusive', () => {
  const picade = readFileSync(join(hardwareFiles, 'picade.txt'), 'utf8');
  const hyperpixel = readFileSync(join(hardwareFiles, 'hyperpixel.txt'), 'utf8');
  const hdmi = readFileSync(join(hardwareFiles, 'hdmi.txt'), 'utf8');
  assert.match(picade, /\[pi4\]\ndtparam=audio=off\ndtoverlay=picade,noactled/);
  assert.doesNotMatch(picade, /dtoverlay=vc4-kms-dpi/);
  assert.match(hyperpixel, /dtoverlay=vc4-kms-dpi-hyperpixel4/);
  assert.doesNotMatch(hyperpixel, /dtoverlay=picade/);
  assert.doesNotMatch(hdmi, /dtoverlay=/);
  for (const [name, code] of Object.entries({ button1: 304, button2: 305, button3: 308, button4: 307, button5: 310, button6: 311, coin: 314, start: 315 })) {
    assert.ok(picade.includes(`dtparam=${name}=${code}\n`));
  }
});

test('Picade udev identities separate the gamepad from utility and power keys', () => {
  const rules = readFileSync(join(hardwareFiles, '70-spacewars-picade.rules'), 'utf8')
    .split('\n').filter(line => line.startsWith('SUBSYSTEM'));
  assert.equal(rules.length, 2);
  const gamepad = rules.find(line => line.includes('ATTRS{name}=="Space-Wars Picade"'));
  const keyboard = rules.find(line => line.includes('ATTRS{name}=="Space-Wars Picade Keys"'));
  assert.ok(gamepad && keyboard);
  assert.ok(gamepad.includes('ENV{ID_INPUT_JOYSTICK}="1"'));
  assert.ok(gamepad.includes('ENV{ID_INPUT_KEYBOARD}=""'));
  assert.ok(gamepad.includes('ENV{ID_INPUT_KEY}=""'));
  assert.ok(!gamepad.includes('power-switch'));
  assert.ok(keyboard.includes('ENV{ID_INPUT_KEYBOARD}="1"'));
  assert.ok(keyboard.includes('ENV{ID_INPUT_JOYSTICK}=""'));
  assert.ok(keyboard.includes('TAG+="power-switch"'));
});

test('flash help documents explicit hardware selection without accessing disks or saved settings', () => {
  const output = execFileSync(process.execPath, [join(YOCTO_DIR, 'scripts/flash.mjs'), '--help'], { encoding: 'utf8' });
  assert.match(output, /--profile/);
  assert.match(output, /picade/);
});
