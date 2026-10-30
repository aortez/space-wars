# Development image installs

A development install puts a locally built full image on an opted-in unit over
SSH. It uses the same inactive-slot writer, trial boot, early watchdog and
automatic return as an OTA install, but it is not signed, published or given a
generation number. It is the full-image counterpart of
[fast application updates](pi-kiosk.md#fast-application-updates), and the
intended loop for OS-level development on the Nightly test unit.

Status: implemented, and not yet installed on a unit. See [Rollout](#rollout).

## Why a separate path

Every full-image route on a managed unit currently goes through the public
signed feed. The legacy `./update.sh` image path stops when it finds
`engine-os-manager`, the supervised SSH tools stage only from the unit's feed,
and bootstrap accepts only a freshly flashed unit. Publishing each test build to
Nightly would make it public and permanent and would use up a generation.

The recovery record ties every transaction to a positive generation that
becomes the unit's floor. A development image given a generation would block the
next real Nightly with the same or a lower number. The alternatives considered
were:

- **A signed Nightly generation per build:** public, permanent and one-way.
- **Pulling from the workstation:** the feed address is fixed with the trust
  root at setup and must be HTTPS with a certificate the Pi trusts, and signing
  would have to stay in step with the public feed's metadata versions.
- **Renumbering public generations to leave gaps for local builds:** changes
  the public feed for every unit to suit one.

Root over SSH can already write the disk directly, so an unsigned
root-initiated install adds no new authority. Signing protects the network
path, which this route doesn't use.

## Workflow

```sh
./update.sh --development --target sw-picade-2.local
```

The flag and the target are explicit; there is no default unit. A unit that
hasn't enabled development installs refuses before anything is built.

1. Ask the unit whether development installs are enabled.
2. Build the image as for any full update (`npm run build`).
3. Pack it with `image_candidate pack-development`. This runs the release
   packer's structural checks (filesystem, embedded records, executable headers,
   boot manifest) without the release policy, so any source state, builder and
   provenance is accepted. It writes a single-frame `image.ext4.zst` and a
   `development.json` descriptor: the signed descriptor's fields without
   generation or channel, plus the source's dirty flag.
4. Copy both into a private root-owned directory in the unit's `/tmp` through
   `swadmin`'s sudo, as fast mode does, after checking free space. The trial
   reboot clears it.
5. Run `sudo engine-os-manager maintenance development install STAGE PAYLOAD_SHA256`.
   The unit writes the inactive slot and requests the trial reboot.
6. Wait for the unit, then report from its recovery record whether the trial
   committed or the unit returned to the previous image. A return exits with an
   error.

Every command, including the deployment network hold, shares one SSH connection
per boot. A fresh login makes the unit record a network admission, which a slow
USB stick can stall, so a command that finds no shared connection fails instead
of logging in again. If the connection drops during the write, the script waits
for the reboot. If the install stopped with the connection, the next boot
records it as interrupted before the trial, and the previous image keeps
running.

App-only changes should keep using `--fast`. It takes seconds rather than a full
image write.

## Unit checks

The command runs only as root and only with development installs enabled. It
holds the shared install lock and refuses during bootstrap, a trial or any
unfinished transaction, as fast mode does. Like an official install, it starts
only from a confirmed normal boot, so a normal boot has proven the promoted
default before another trial relies on it. After a committed trial, the script
reboots the unit once before the next install. Provisioning must be complete,
and the owner's install approval must validate, because a broken image is only
safe where tryboot and the watchdog were qualified.

Before writing, the unit checks the descriptor and payload against itself:

- the payload's length and SHA-256, and a bounded single-frame decode;
- machine, architecture, layout and boot identity (rootfs only: an image with
  other boot assets is refused);
- recovery protocol equal to the running one, and an updater protocol it meets;
- an expanded size that fits the inactive slot;
- data compatibility in both directions with the good image;
- the image's declaration that it understands development records.

The existing writer then writes and rereads the slot and checks its filesystem.
It also compares the embedded image record with the descriptor. The trial
reboot uses the existing handshake without the signature and expiry checks,
which have nothing to check. The trial is unchanged. The new image must report
continuous health inside the deadline, the early watchdog covers hangs, and any
failure returns the unit to the previous slot.

## Recovery record

A development transaction has candidate generation 0. Generation 0 already
means an unpublished image, as for the bootstrap baseline.

- It doesn't raise the generation floor, so the next official image installs
  over it as usual.
- A failure is kept in the transaction only. It doesn't enter the quarantine
  map, so development failures can't fill the map or block official images.
  The same payload may be installed again.
- After commit, the development image is the good image at generation 0, and
  the previous image stays in the retained slot.

The record uses schema 3 while its transaction is a development install and
schema 2 otherwise. Units that never use development installs keep records
older generations can read. The fallback image can always read schema 3,
because the command runs from it. The candidate declares the same support,
checked before writing. Older images refuse schema 3 outright instead of
misreading it.

## Opt-in and status

`sudo engine-os-manager maintenance development enable|disable|status` manages a
root-owned marker in `/data/spacewars-system`. The marker is bound to the unit's
device, layout, boot files and hardware profile, so a USB reflash with other
boot files turns it off. Enabling needs a valid install approval, as installing
does. Only sw-picade-2 enables it.

`status` reports whether development installs are enabled, whether the unit is
on a normal boot, and the good image's build and generation. The recovery
report in `engine-os-manager status` also carries the good and candidate
generations, where generation 0 is an unpublished image. Update checks continue
as usual, and the owner's Install press still governs official images.

## Rollout

1. A PR with host tests for the record, checks, opt-in, packer and workstation
   script.
2. The feature reaches sw-picade-2 in a normal signed Nightly generation that
   the owner installs. A failed development trial returns to that image, so it
   must already read schema 3.
3. Enable development installs on sw-picade-2.
4. Two supervised runs, each recorded: one image that commits, and one whose
   app fails health so the unit returns. The first run also confirms that the
   kiosk's update screen clears after the commit.

## Limits

- Kernel, firmware and boot files still need a USB reflash.
- A recovery protocol change can't be installed this way.
- Development images must be built from a source that includes this feature.
- Returning to an official image means the next official generation, or a
  development install of a build from main.
- An image that breaks the update service or this command, but still passes the
  trial, needs a USB reflash or repair over SSH.
- While a development trial runs, the kiosk's update screen waits for the device
  installation service, as during supervised installs. It clears when the trial
  commits or returns.
- On the Picade, expect about fifteen minutes per cycle with warm caches. Most
  of that is the build and the slow USB flush.
