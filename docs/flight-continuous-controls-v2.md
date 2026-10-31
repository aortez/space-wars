# Continuous flight controls v2

**Status: implemented and verified, 2026-10-24.** The user chose genuine
continuous throttle and braking after the [update-attribution audit](flight-update-attribution-v1.md).
This supersedes the proposed binary-button likelihood comparison. It changes the
action and physics contract; historical binary-model scores remain historical.

## Control and physics contract

At each 60 Hz decision, the skill owns three independent controls:

| Control | Range | Effect |
|---|---|---|
| Steering | −1 to 1 | Existing desired angular-rate control. |
| Throttle (`thrust`) | 0 to 1 | Proportional forward thrust, with the existing speed governor. |
| Brake | 0 to 1 | Proportional existing velocity-opposing brake acceleration. |

Throttle and brake may be applied simultaneously. No hidden mutual exclusion,
automatic switching, pulse generator, native pilot or fallback is introduced.
A throttle of 0.03 commands 3% force each tick; it no longer means a 3% chance of
full thrust. Force limits, gravity, terrain/collision handling, sensor geometry,
60 Hz cadence, reward formula and physical arrival requirements are unchanged.

With neutral steering, braking now interpolates rotational damping from the normal
unbraked strength to full strength. In learned mode this is `0.25 + 0.75 * brake`.
Standard mode retains its landing-assist baseline, interpolating that baseline toward
one. Active steering retains full turn authority. Thus a tiny brake input no longer
activates full braking or full neutral spin damping. The existing zero/full brake
endpoints are preserved.

The public `SurfaceFlightAction { thrust, brake }` scenario message carries fractional
inputs through ordinary game stepping, using action kind `0x5355_0006`. Existing
`SurfaceSortieAction` remains the keyboard/digital, steering and interaction interface.
A digital message clears any previous analog override; an analog message in the same
tick takes precedence for throttle/brake regardless of message ordering. With no new
message, held input persists as before. Death, disarmed controls and being on foot
suppress aboard actuators. Re-arming requires analog controls to be released too.
Keyboard buttons still command zero/full power. No new physical gamepad binding or
user-interface slider is assigned by this change; the analog game API is available
for callers and the learned environment uses it directly.

The learned `FlightAction` now carries finite numeric throttle/brake, validates their
ranges and records the actual fractions. Invalid actions terminate as controller
errors without advancing physics. The scenario message decoder rejects malformed,
nonfinite and out-of-range analog payloads. Previous-action observation features
retain fractions, rather than boolean encodings.

## Policy and learner

The polar/set encoder, shared trunk, three actor outputs and value head keep their
existing dimensions. The actor outputs are interpreted as three Gaussian latent
means. Execute `tanh(z_steering)`, `sigmoid(z_thrust)` and `sigmoid(z_brake)`.
The native mean decoder applies those same transforms to the network outputs.
These are transformed latent means, not the expectations of sampled control values.

All three channels have learned global log standard deviations, initially −0.5 and
clamped to [−5, 2]. This adds two parameters: **39,447 total**, versus 39,445 in v1.
PPO scores the joint transformed continuous density, including stable tanh/sigmoid
Jacobian terms. Its entropy estimate uses reparameterized samples of that same
executed distribution. There are no auxiliary button magnitudes discarded by physics,
Bernoulli actions, button thresholds or probability-to-duty-cycle conversion.

Finite unsaturated sigmoid outputs lie inside (0,1); the game API itself accepts
both exact endpoints. There is no new dead zone or quantization step. Numerical
saturation is handled with stable log-Jacobian expressions evaluated from retained
latents, without inverting rounded control values.

The optional exploration descriptor is now `continuous-gaussian-ar1-v2`; independent
rho=0 and the existing conditional AR mechanism operate on all three continuous
latents. Old `gaussian-latent-ar1-v1` descriptors are rejected. This implementation
supports correlation but does not select it as a new learning hypothesis.

Evaluation explicitly distinguishes:

- `continuous-mean-v2` (default): native Rust transformed latent means, no sampling.
- `continuous-sampled-v2`: all three channels sampled independently using saved
  standard deviations, isolated CPU RNG and declared repeat/episode seeds.

The [sampled execution descriptor](../tools/flight_ppo/continuous-execution-v2.json)
is pinned by hash. Sampled evaluation retains actor means, all three deviations,
latents and executed fractions alongside native traces. Both modes are tools for
future experiments, not flight-qualified candidates. No old mixed decoder remains
silently active. Collector summaries now record `thrust_amount_sum` and
`brake_amount_sum`: sums of applied fractions, whose tick-normalized values are mean
actuator demand, not counts of button presses.

## Compatibility and use

Bridge protocol, environment trace, policy observation schema, policy export and
checkpoint format are **version 2**. The schema and training configuration explicitly
identify `continuous-flight-v2`. Old binary exports, old observation schemas, boolean
action payloads, v1 traces and old execution identifiers are rejected. Exact resume
still requires the full source/runtime/configuration identity. There is no automatic
migration or warm start from binary-policy weights.

The supplied smoke and clear-control configurations include the new action contract.
A new training experiment must declare its own task distribution, reward, duration,
evaluation mode and gates before running; updating a template does not retroactively
change any archived configuration. Example sampled-evaluation configuration:

```json
{
  "action_contract": "continuous-flight-v2",
  "exploration": {"kind": "continuous-gaussian-ar1-v2", "rho": 0.0},
  "evaluation": {"mode": "continuous-sampled-v2", "repeats": 4, "seed_base": 5310000}
}
```

Historical audit scripts and the old mixed execution descriptor are retained for
provenance. Replay v1 experiments using their sealed source/runtime snapshots, not
the current v2 bridge or learner. No old archive or recorded qualification is rewritten.

## Verification and next step

Validation covers real fractional thrust/brake velocity changes, neutral rotational
damping interpolation, unchanged digital endpoints, release/re-arming, fractional
trace replay and invalid/old action rejection. Learner checks cover analytical
transformed density, extreme-latent finite gradients, native/PyTorch action parity,
set/circular-encoder invariants, terminal-aware advantages, evaluation RNG isolation
and exact fresh-process continuation of short two-update fixtures. Those fixtures
validate implementation, not flight competence.

The next proposed step is a separately frozen, bounded fresh-model experiment on the
existing aligned-approach tasks, beginning with short continuous-control trajectory
inspection. Keep arrival criteria and the current reward fixed initially, inspect
throttle/brake magnitude and approach/settling separately, and declare mean versus
sampled evaluation before scoring. Do not infer improved flight from a smoother
control interface or compare old/new success rates as if physics/actions were identical.
Obstacle navigation and reserved confirmation remain pending.


### Completed validation and retention

All **413 surface-sortie tests** and **463 AI library tests** passed (two and one
pre-existing ignored tests respectively). All scenario examples compile. The final
Python suite passed **42 tests**, including exact fresh-process restart for continuous
independent/correlated exploration, correct transformed likelihoods, native action
parity and evaluation-between-updates leaving complete training state unchanged.
After rebuilding the final formatted source, native export parity and exact resume
passed again in two targeted checks. Python compilation and whitespace checks passed.

The initial Python pass exposed four fixture errors: one test still sent binary
actions and three evaluation tests used an eight-tick deadline shorter than the
thirty-tick dwell. Correct the fixtures to numeric endpoint actions and thirty-tick
deadlines; do not relax the action or physical contract. Retain the failed and
corrected logs. Remove an incidental duplicate import of the base test class from
test discovery and add the continuous evaluation/training-state invariant test;
the final suite count is 42 unique tests.

[Validation results](data/flight-continuous-controls-v2-results.json) and the
[archive receipt](data/flight-continuous-controls-v2-archive.json) identify 1,533 source
files (30 modified and one added relative to LF-025), both test executables, all logs,
98 retained short-fixture checkpoints and 99 policy exports. The full-suite executable
and final rebuild have different debug/source identities; both are retained, and
exact-resume checks use their recorded identities. Earlier development fixtures remain
diagnostic evidence, not candidate policies. All 21 preceding manifests are unchanged.
Local retention is not off-machine backup; no commit, new flight qualification or
reserved confirmation use is implied.
