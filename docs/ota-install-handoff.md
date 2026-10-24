# Manual installation handoff

The manual handoff for #264/#266 now includes shared messages, the service
coordinator, bounded Unix sockets, the running-app adapter and an Updates panel
in App Settings. Preparation uses the real ordered settings writer and completed
kiosk page flips. The production service deliberately reports **unavailable**:
the native verified installer, final admission gates and startup reconciliation
are not connected. A durable handoff journal and recovery-association library
now support that integration. There is no runtime switch to bypass the gate.

The contract lives in
[`spacewars-control::install`](../crates/spacewars-control/src/install.rs) and
[`engine-os-manager::install_handoff`](../crates/engine-os-manager/src/install_handoff.rs).
The model does not perform disk writes, HTTP, radio or reboot operations. Its
adapters live in `install_service.rs`, `install_transport.rs` and the client's
`updates_linux.rs`. The coordinator reads random bytes once for its incarnation.

## Transaction and authority

The user approves one exact verified image and its restart consequence. An
active or paused game additionally requires explicit end-session consent.
Checking for updates, downloading, selecting a channel or reading status cannot
create that consent.

```mermaid
sequenceDiagram
    participant U as User / Updates panel
    participant S as Device service
    participant H as App UI thread
    participant W as Verified installer
    U->>S: Request offer for currently ready candidate
    S-->>U: Candidate, restart/game consequence, expiring ticket
    U->>S: Confirm exact ticket and game termination when needed
    S->>H: Prepare exact host/settings revision
    H->>H: Fence launches, save settings, present updater
    H-->>S: Acknowledge exact ticket and saved revision
    S->>S: Revalidate current context and consume consent once
    Note over S,W: Native adapter still required: locks, holds, trust and recovery gates
    S->>W: Begin durable transaction, write inactive root, request trial
    W-->>S: Confirmed commit or rollback after recovery
```

The last two steps remain existing backend responsibilities and future adapter
work. `ManualAuthorization` is a non-cloneable, non-serializable consent value;
it is **not** a write permit or a `RecoveryCapability`. It cannot enable the
owner-only supervised admission bypass. Normal service installation must retain
the qualified boot/watchdog capability gate and all signed-image checks.

## Binding and lifetime

An offer binds the verifier's complete candidate identity, channel-selection
generation, boot, kiosk PID/start time/systemd invocation, host revision and
settings-document revision. `Ready` can only be constructed from a
`VerifiedCandidate`; request JSON cannot supply a trusted descriptor.

Tickets contain a random service-incarnation identity and an increasing sequence.
They are public correlation identities, not bearer secrets. Service restart,
including within the same boot, requires fresh consent. Host observations must
be no older than two seconds, with no future or backwards clock observation.

An offer lasts at most five minutes. An unchanged repeated offer returns the same
ticket and deadline. Confirmation starts a preparation allowance of at most
thirty seconds, capped by the offer's original deadline. Retries never renew it;
expiry is inclusive. These are handoff bounds, not a timeout for a root write.

Any observed candidate, selection-generation, host, revision or maintenance
change before consumption aborts that ticket permanently. Restoring the old
conditions cannot revive it. A stale ticket or wrong peer cannot consume or
cancel a newer handoff. The coordinator keeps an abort available while the host
reports its fence, so losing an abort response does not force a new transaction.

## App serialization and uncertain outcomes

The UI adapter must serialize every scenario launch, autostart, setup transition,
settings mutation and unrelated input with `HostGate`. Relevant changes advance
the host revision. The confirmation input and handoff's own updater transition
retain the selected revision. A paused game remains a game.

`prepare` establishes the fence before saving. A successful receipt for the exact
settings-document revision and a presented updater screen are both required
before acknowledgement. The adapter must associate the actual `SettingsWriter`
receipt with that document revision; queueing a save or receiving an older
receipt is insufficient. A failed save, pre-acknowledgement input or expired
preparation releases the fence and prevents acknowledgement. Retired tickets
cannot be revived by delayed prepare messages.

Acknowledgement is the app-side handoff boundary. From then on, a lost response,
timeout or service disconnect cannot release the fence: the service may already
have consumed consent and started a transaction. Live cancellation can release
it only through an authenticated exact-ticket abort before consumption. After
consumption, cancellation returns too-late. Restart reconciliation can also
produce a durable abort when the unchanged recovery baseline proves no
transaction began; otherwise the existing recovery transaction determines the
outcome. Reboot requested, SSH lost and write finished are not success signals.

After service or app restart, the future native adapter must reconcile the
durable recovery journal and any acknowledged app fence before accepting new
work or admitting launcher/autostart gameplay. A fresh in-memory gate does not
prove that a previously acknowledged installation ended.
Unknown old tickets are not evidence of cancellation. The in-memory coordinator
does not persist, replay, resume or retry an installation. The separate journal
below records an association and decisions, never replayable consent.
Each `HostGate` is bound to one service incarnation. Replace it for a new service
only after reconciling and releasing any old fence; reconnecting alone cannot
discard retired-ticket history or an uncertain acknowledged handoff.

## Connected service and app adapters

The service creates `/run/spacewars-system/install.sock` beside its status socket
under the existing exclusive service lock. The kiosk creates
`/run/spacewars-kiosk/install.sock` in its systemd runtime directory. Host eligibility
uses the `pi-kiosk` build feature and a valid process/systemd invocation identity;
it is independent of the direct-launch `--kiosk` CLI flag. A full UI request
queue reports a visible busy result so reopening Updates cannot wait forever.
No new privileged helper, owner-only installation bypass or device path is exposed.

Frames are newline-delimited JSON, capped at 4 KiB, with strict schemas and a
two-second request-frame deadline. Each listener admits at most four clients;
the UI and service work queues are bounded. Clients authenticate `SO_PEERCRED`
before sending command bytes and bound the complete exchange, including connect.
The app accepts root only. The service accepts root and the kiosk UID. Its host
adapter samples the active systemd PID/invocation and `/proc` start ticks before
and after the exchange, and requires the connected peer to be that PID/UID.
Test fixtures inject private temporary sockets and their test-process identity;
production paths and credentials are fixed.

Offer/confirmation work runs off the request handler. A confirmation returns
`preparing` before waiting for the app. Status and cancellation can run during
that wait. The coordinator retains the exact abort decision for a lost reply;
the worker delivers it to the host after an unsuccessful preparation. Consumed
consent cannot be cancelled or consumed again. The production backend currently
returns unavailable without issuing an offer or preparing the app.

Updates is reachable from launcher and paused-game App Settings using touch,
keyboard, gamepad or the existing guarded UI control interface. It displays the
exact channel, generation, build and image identity, with an explicit
**Install & restart** or **End game, install & restart** confirmation. Desktop
installation remains unavailable. The UI does not claim write progress or
success from an in-memory acknowledgement.

The app listener hands requests to the UI thread. It records the full settings
snapshot, scene and input revisions, establishes a launch/input fence, and asks
`SettingsWriter::save_exact` for an ordered barrier receipt. Ordinary saves may
still coalesce; this exact document cannot be replaced by a newer queued save.
A valid launcher draft is included and retained in memory when preparation
starts. Settings changes, other queued saves, scene changes, focus loss and
preparation expiry revoke an unacknowledged handoff. A failed save cannot ack.

Acknowledgement additionally requires two completed DRM page flips after the
updater screen was published. This excludes a frame already in flight at
publication. UI timers and redraw requests alone do not satisfy presentation;
a renderer without that observation reports the host unavailable. During the
fence the scenario loop discards pending controls/input, manual and automatic
launches are rejected, and diagnostic mutation routes are blocked. The Updates
controls and read-only health/status/screenshot routes remain available.

After acknowledgement, a lost reply or service disconnect leaves the updater
and fence in place. Only the authenticated exact-ticket abort can release it.
The existing standalone app starts are not yet recovery-journal aware, so the
native writer must remain disabled until restart reconciliation is connected.

## Durable association library

[`install_journal.rs`](../crates/engine-os-manager/src/install_journal.rs)
implements a separate root-owned journal at
`/data/spacewars-system/install-handoff`. It is not yet called by the running
service or app. It does not change the existing recovery journal format, so
fallback images retain their current recovery protocol.

The native worker must retain the shared installation lock through these steps:

1. Persist the exact Prepare message, boot, channel-selection generation and
   recovery-record fingerprint before asking the app to fence and acknowledge.
2. Revalidate the candidate and native admission gates, consume the one-use
   authorization, and obtain an ordinary qualified recovery plan.
3. Call `Journal::begin`. It first persists the exact planned transaction ID,
   then commits the existing recovery plan. Only a successful recovery commit
   returns a write action. The read-only `Plan::installation` association does
   not itself authorize a write.
4. Retain the lock and all native guards while the existing writer and trial
   protocol execute. Status handling must not take this worker's journal locks
   or hold the in-memory coordinator mutex across filesystem or host I/O.

After a crash, the replacement worker must acquire the same installation lock,
open both journals, and validate native boot evidence before reconciling. A
surviving worker that still holds the lock prevents a conflicting abort decision.
An unchanged recovery fingerprint proves a prepared or armed handoff never
started its recovery transaction; the journal durably records an exact-ticket
abort. This also covers consumed consent lost before recovery commit. A matching
active transaction remains in progress. Only its recorded Committed or RolledBack
phase produces a finished result; the existing recovery worker resolves an
interrupted write, including quarantine. No write or reboot is retried here.

Missing/corrupt state in an existing directory, a different transaction, changed
candidate/origin boot, regressed outcome or uncertain persistence blocks a
decision. Both journals must be reopened after a failed commit. The shared
bounded store checks ownership, permissions, lock/file identity and checksum,
and uses file sync, rename and directory sync. Terminal decisions remain until
a new authenticated host observation made after the decision proves the fence
is gone; stale tickets and cached pre-decision observations cannot clear them.

The ordinary native recovery-capability provider is also still absent. The
supervised owner's separate admission path must not be used to manufacture
tryboot/watchdog qualification for this service.

## Integration still required

1. Connect verified-ready/channel invalidation and the final handoff to the
   native worker. Hold the shared install lock and reboot-capable maintenance
   protection, recheck signatures/expiry, staged file identity and hashes,
   boot/layout, generation, recovery capability and data compatibility. Begin
   the existing durable journal transaction before disk mutation; keep slow
   I/O outside the coordinator lock and status/UI handling.
2. Connect the durable association library to the worker and reconcile it before
   app launch/autostart or a new service offer after restart. Add authenticated
   startup/outcome messages and expose durable progress and confirmed
   commit/rollback. Unknown old tickets cannot release a fence.
3. Add the equally explicit candidate-bound install CLI. The current diagnostic
   UI interface can navigate Updates, but is not a privileged install shortcut.
4. Qualify real systemd permissions and renderer presentation, worker failure,
   service/app restart and a supervised native installation on the device.
   Local socket/Slint tests do not replace those checks.

Ask/Automatic preferences and overnight admission follow this manual path; they
must reuse the same writer, locks, recovery and final app handoff.

## Validation scope

Contract tests cover exact consent, games, save/presentation gates, cancellation,
candidate/channel/host/maintenance changes, one-use consumption, lost replies,
deadlines, clock discontinuities and service restart.

Socket tests exercise actual Unix listeners and peer credentials, rejection
before request disclosure, bounded frames/exchanges, delayed acknowledgement,
responsive cancellation, lost abort replies and stale consent after restart.
The client tests run the actual Slint panel and app socket with the real ordered
settings writer, a temporary settings file, blocked/failed saves, rendered
software frames, changed settings, expiry and a lost service reply. The software
renderer supplies the test presentation counter; physical DRM flips and native
installation are not claimed by those tests.

Journal tests reopen real files at prepare/acknowledgement/admission/outcome
boundaries and inject failures before and after rename/directory sync. A signed
feed fixture exercises real verification and both durable journals: association
or recovery persistence failure returns no write action, while an interrupted
committed admission is quarantined by the existing recovery protocol. Synthetic
recovery views cover each nonterminal/terminal phase and mismatched identities.
These tests do not open native block devices, change boot selection or qualify
tryboot/watchdog hardware.
