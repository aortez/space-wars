# Crow flight: first motion slice of #152

The crow now integrates velocity at the Clock's fixed 60 Hz instead of assigning
positions along a timed lift/cross/descend path. This slice changes flying only:
the same seeded perch selection, nearby-hop priority, hop parabola, perch waits,
support invalidation, resident admission and 22-second visit limit remain.
Food, pecking, ground visits and water interactions are later work.

## Motion and boundaries

`scenarios/clock/src/crow/flight.rs` owns a small fixed-size state: velocity,
climb/cross/land stage, launch column and an optional active wing stroke.
It adds no allocation, Rapier body, second physics world or random-number draws.

- Lateral acceleration is bounded; the final approach uses a damped controller
  to brake without repeatedly overshooting and turning around.
- Gravity and vertical drag run every tick. Forward motion supplies limited
  passive lift (at most 55% of weight), so a glide alone cannot sustain height.
- Flapping is demand-driven. An active stroke takes 18 ticks: three to raise the
  wing, six for the powered downstroke, and nine to recover to a level glide.
  Between strokes the bird coasts until its vertical motion needs another lift.
  The downstroke delivers a bounded upward impulse spread smoothly across its
  six ticks. Stronger strokes also have a larger visible wing sweep; softer
  landing strokes use less impulse and less wing travel. Rendering consumes the
  same pose used to time the lift, with no separate body-bobbing animation.
- Vertical drag is 0.9/s, between the first prototype's 1.2/s and the strong
  experiment's 0.6/s. Each cruising stroke deliberately leaves some upward
  momentum for a visible rise and settling arc. The middle setting reduces the
  extra cruising kick from 2.4 to 1.2 cell pitches/s and the minimum stroke impulse
  from 3.8 to 2.8; landing retains its gentler impulse settings.
  Headroom bounds its impulse near the frame. This is an authored arcade model,
  not aerodynamics or a full momentum/energy simulation; horizontal steering
  remains independent.
- Cruising height follows the digit face rather than the top of the screen.
  This avoids disproportionate vertical travel in portrait layouts. The route
  still climbs above the possible digit tops before crossing to its target.
- The feet must cross the intended support downward, horizontally aligned and
  moving slowly, before the crow becomes perched. A hard touchdown is caught and
  damped rather than passing through that digit. The controller aims slightly
  through the contact plane so its wingbeat cannot leave it hovering just above
  the perch forever.
- A downward foot sweep also checks surviving exposed digit tops. This catches
  an old support when departure or retargeting interrupts a descending approach;
  a real contact restarts the climb from there. Destroyed/withdrawn supports do
  not remain as invisible landing planes. This fixed a case revealed by the
  stronger strokes (Classic, 4:3, seed 1, automatic departure at tick 1020).
- A perched departure starts with a small leg push. Airborne retargeting preserves
  velocity and wing phase. A scripted hop supplies no additional launch push when
  interrupted. Hops retain their existing trajectory, including their existing
  visual clearance limits in extreme wide layouts.
- Approach has an eight-second safety limit, after which the crow leaves. Five
  seconds are reserved for departure within the original 22-second envelope.
  Normal departure completes once the silhouette is offscreen. These limits
  bound failure; they do not snap the crow onto a missed perch.

There is no general body collision response against digits, moving debris, the
duck, water, or the decorative canopy. Route clearance and swept foot contacts
handle this slice. Crossing a newly changed arrangement is still a bounded
retargeting problem, not a general obstacle planner. Pause/control synchronization
does not integrate position, velocity or wing phase.

## Native before/after gallery

`crates/engine-client/src/crow_visual_tests.rs` runs the actual Clock and native
raster/vector renderers. An optional gallery plays the captures; its JavaScript
does not simulate flight. The baseline is merged commit
`79a9609c3488e4ba28c81cd66764dfba925eca02` with the capture harness added.

Three cases use seed 42 at 1024×768, 800×480 and 480×800. Each runs for 22 seconds.
At eight seconds three reading changes remove the old supports; at 15 seconds
Digit Slide withdraws the perches and prompts departure. Both use the same inputs.
Different travel times can change which perch is occupied when those inputs
arrive and when later random choices occur; this is not a comparison of identical
later destinations. Replay supports pause, seeking and quarter/half speed. The
default comparison uses the stronger-stroke prototype against the middle setting.
The first gentle-wingbeat prototype and original scripted path are also available
in the comparison selector.

Export the experimental captures from this branch (use an absolute output path):

```sh
SPACEWARS_CROW_FLIGHT_ARTIFACTS="$PWD/target/crow-flight/wingbeat" \
  cargo +1.89.0 test --locked --profile ci -p engine-client \
  crow_flight_renders_and_exports_playback -- --nocapture
python3 -m http.server 8766 --directory target/crow-flight
```

Open `http://localhost:8766/wingbeat/`. During local iteration the previous
prototypes' captures were preserved in `target/crow-flight/gentle/` and `strong/`,
with source snapshots in `gentle-source.tar.gz` and `strong-source.tar.gz`. The
viewer selects the first available comparison: strong, gentle, then original
scripted captures in `target/crow-flight/scripted/`.
To regenerate the original scripted captures, use a detached
worktree at the baseline commit and copy only the capture harness (the test module,
its HTML file, and the two-line module registration in engine-client's `main.rs`)
into it. Run the same command there with the output pointing to the original
checkout's `target/crow-flight/scripted/`. No production flight code or runtime
legacy-mode switch is required. Full-frame PNGs and periodic SVGs are retained
with each capture manifest; ordinary tests render only a small smoke sample.

## Verification

The normal suite covers:

- A deterministic 48-visit paired replay over six aspect ratios, including exact
  velocity/wing-state agreement, bounded rendering, hopping and zero physics
  resources. With variable travel time, an individual visit can spend its lifetime
  moving between isolated perches; hopping is required across the seeds for each
  layout rather than in every individual visit.
- 192 visits (four fonts × six aspect ratios × eight seeds): initial landing,
  departure beyond the visible frame, silhouette bounds, feet outside lit digit
  interiors and no retained physics resources.
- Downstroke lift and recovery, visible cruising rises and unpowered gaps,
  effort-scaled landing strokes, swept hard touchdown, airborne retarget momentum,
  pause, reading/format changes, destructive events, shared Rain/duck admission,
  duplicate admission, cooldown, resize and cleanup.
- Both native rendering paths and the actual Clock input sequence used by the
  gallery. Phase captures wait for landing instead of assuming tick 105.

Run the focused tests with `cargo +1.89.0 test --locked --profile ci -p
scenario-clock crow` and the renderer checks with `cargo +1.89.0 test --locked
--profile ci -p engine-client crow`. The complete Clock suite remains the wider
regression check. This study is a motion experiment, not a new device FPS claim.

First prototype checkpoint (2026-10-02): all 295 Clock tests passed (four existing opt-in
tests skipped), as did four selected client tests including both Crow render
fixtures. The 192-visit matrix had first landings at ticks 109–291 (1.82–4.85 s),
no ordinary approach timeouts, and all departures offscreen. Both gallery sets
contain 1,983 PNGs and 36 SVGs across the three layouts.

Formatting, diff checks and strict Clock Clippy passed. Strict engine-client
Clippy is blocked by 29 existing warnings; all 29 primary source spans were
verified present in merged main, with none in the new flight/capture code.
Normal client Clippy completed. The display-driven Crow functional workflow
was not run: this session has neither an active display nor Xvfb. No browser
surface was available for interactive gallery testing; native stills, gallery
assets, HTTP responses and JavaScript syntax were checked directly.

Stronger-stroke checkpoint (same day): all 297 Clock tests and four selected
client checks passed, along with strict Clock Clippy, formatting and diff checks.
The 192 visits still land and leave offscreen without ordinary approach timeouts;
first landings range from ticks 123–313 (2.05–5.22 s). In a separate eight-second
settled-cruise sample, the crow uses nine strokes, rises/falls through 0.404 cell
pitches, and has unpowered gliding gaps up to 0.567 s. Those are controlled motion
measurements, not a fixed flap rate for all flight. The prior gentle and original
scripted captures are both retained for visual comparison.

Middle-setting checkpoint (same day): all 297 Clock tests and four selected
client checks passed, along with strict Clock Clippy, formatting and diff checks.
The 192 visits land at ticks 119–307 (1.98–5.12 s) and depart offscreen without
ordinary approach timeouts. In the same eight-second settled-cruise sample,
there are 13 strokes, 0.210 cell pitches of rise/fall, and unpowered gliding gaps
up to 0.350 s. This roughly halves the strong version's vertical excursion while
keeping distinct lift pulses. All three gallery layouts were regenerated, and
the strong, gentle and scripted captures remain available for comparison. Native
stills, complete frame sets, served manifests/viewer and JavaScript syntax were
checked; the display/browser limitations above still apply.
