# Development image installs

A development install puts a locally built full image on an opted-in unit over
SSH. It uses the same inactive-slot writer, trial boot, early watchdog and
automatic return as an OTA install, but it is not signed, published or given a
generation number. It is the full-image counterpart of
[fast application updates](pi-kiosk.md#fast-application-updates), and the
intended loop for OS-level development on the Nightly test unit.

Status: design. Nothing described here is implemented yet.

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
./update.sh --target sw-picade-2.local
```

On a managed unit, the full-image path becomes a development install:

1. Build the image as today (`npm run build`).
2. Pack it with `image_candidate pack-development`. This runs the existing
   structural checks (filesystem, embedded records, executable headers, boot
   manifest) without the clean-source policy. It writes a single-frame
   `image.ext4.zst` and a `development.json` descriptor: the signed descriptor's
   fields without generation or channel, plus the source's dirty flag.
3. Copy both into a private root-owned staging directory over the owner's SSH
   connection, as fast mode does, after checking free space.
4. Run `sudo engine-os-manager maintenance development install STAGE PAYLOAD_SHA256`.
   The unit writes the inactive slot and requests the trial reboot.
5. Wait for the unit to return, then report whether the trial committed or the
   unit returned to the previous image.

App-only changes should keep using `--fast`. It takes seconds rather than a full
image write.

## Unit checks

The command runs only as root and only with development installs enabled. It
holds the shared install lock and refuses during bootstrap, a trial or any
unfinished transaction, as fast mode does. It also requires a confirmed normal
boot, or a committed trial whose watchdog has finished. Provisioning must be
complete, and the owner's install approval must validate, because a broken
image is only safe where tryboot and the watchdog were qualified.

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
root-owned marker under `/data/spacewars-system`, bound to this unit. Only
sw-picade-2 enables it. A mistyped target is refused by the unit.

`engine-os-manager status` reports whether development installs are enabled and
whether the running image is unpublished (generation 0). Update checks continue
as usual, and the owner's Install press still governs official images.

## Rollout

1. A PR with host tests for the record, checks, opt-in and workstation script.
2. The feature reaches sw-picade-2 in a normal signed Nightly generation that
   the owner installs. A failed development trial returns to that image, so it
   must already read schema 3.
3. Enable development installs on sw-picade-2.
4. Two supervised runs, each recorded: one image that commits, and one whose
   app fails health so the unit returns.

## Limits

- Kernel, firmware and boot files still need a USB reflash.
- A recovery protocol change can't be installed this way.
- Development images must be built from a source that includes this feature.
- Returning to an official image means the next official generation, or a
  development install of a build from main.
- An image that breaks the update service or this command, but still passes the
  trial, needs a USB reflash or repair over SSH.
- On the Picade, expect about fifteen minutes per cycle with warm caches. Most
  of that is the build and the slow USB flush.
