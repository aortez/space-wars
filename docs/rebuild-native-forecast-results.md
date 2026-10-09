# A short native forecast reproduces all three retained settling outcomes

The two-second native rollout correctly distinguishes the failed coarse build
from both successful handoffs, including the corner landing rejected by the
earlier ray-normal and round-foot checks. It reproduces the measured ship motion,
landing telemetry and contact geometry at native precision across all **362
shared tick samples**. Maximum ship-position error is **zero** in each case.

This establishes a useful reference for settling prediction. It does not add a
placement gate: the probe starts immediately after a real native build, clones
the complete world and neutralizes every seat's controls. The full-world model
also costs too much to evaluate across placement candidates in a single frame.

## Forecast versus actual native events

| Build | First contact, forecast / actual | Two supported feet, forecast / actual | Settled, forecast / actual |
| --- | --- | --- | --- |
| Live handoff, 27114 | 27173 / 27173 | 27174 / 27174 | 27188 / 27188 |
| Live coarse, 25373 | 25421 / 25421 | Neither | Neither |
| Recorded handoff, 27459 | 27510 / 27510 | 27543 / 27543 | 27557 / 27557 |

The [frozen implementation](rebuild-native-forecast-plan.md) runs 120 ordinary
native steps of 16,666,667 ns on a clone of the current state. Every seat receives
explicit neutral movement, wing, weapon, mining and impact actions. It reads no
future replay controls and uses the existing gravity, landing assist, collisions,
terrain updates, hazards and two-foot settling rules. There are no pose edits,
timer resets or relaxed landing requirements.

The valid recorded handoff therefore includes the pre-contact sideways drift,
first contact on the corner and subsequent rotation that the straight sweeps
missed. Both successful forecasts earn native settling on the exact original
ticks, 74 and 98 steps after construction. The failed coarse forecast never
has two supported feet or settled time during its 120 steps. That negative
means **not settled within the horizon**, not physical impossibility forever;
the longer retained recovery independently still times out and scuttles it.

All three forecasts run the full horizon, with no vehicle loss, early round end
or terrain revision change. Ship position, velocity, orientation, spin, form,
health, planet motion/revision, landing telemetry and hull/foot contacts match
their actual counterparts at native f32 precision. The live handoff has 120
shared samples because its actual recovery ends at 27233; the forecast continues
one more tick to 27234. The other two comparisons each contain 121 samples,
including the initial build state. This is a comparison of those projected
fields, not a claim that every actor or all full-world state matches the future.

## Cost of the complete-world reference

These are single release-build cost samples on this workstation, one per build:

| Forecast | Clone | Native steps | Clone + native steps | Total diagnostic API |
| --- | ---: | ---: | ---: | ---: |
| Live handoff | 2.08 ms | 60.69 ms | 62.77 ms | 67.22 ms |
| Live coarse | 2.28 ms | 63.76 ms | 66.05 ms | 71.38 ms |
| Recorded handoff | 2.80 ms | 60.37 ms | 63.17 ms | 67.22 ms |

Diagnostic sampling takes another 1.93–2.98 ms and live-state verification
0.90–1.43 ms. Native stepping dominates the cost; removing the clone alone
would not make this suitable for a 16.67 ms frame. The worlds contain 46–51
bodies and 205–210 colliders, with physics snapshots of approximately 2.63–2.65
MB. These numbers are not repeated benchmarks or Raspberry Pi measurements.

The model uses privileged complete-world state, including existing hazards and
the ordinary future world updates. It does not yet satisfy the information and
work limits of a normal bot sensor. It also has not constructed a hypothetical
replacement from an unbuilt placement candidate. Passing these three known
post-build cases is evidence for the model, not production qualification.

## Preservation and validation

Every preceding raw file and all **41 handoff / 40 coarse archive files** remain
byte-identical. Their existing audits were explicitly reused only after that
verification. The new forecast audit independently compares the projected rows
with retained native observations. The successful live recovery still completes
at 27233; the coarse recovery remains blocked at 27349, with its original extra
ship loss. The recorded handoff still settles without boarding by the fixed end,
and the recorded coarse fork still builds nothing.

Runtime source **343392d**, **1,010 input hashes**, six changed inputs and a
copied release executable were frozen before execution. Two prefixes and four
continuations produced three forecast rollouts, totaling **360 projected native
steps**. There were **zero simulation retries and zero audit resumes**. Each
forecast verifies unchanged live physics, pilot observations and standing
contacts around the complete operation.

Passed: **187 Rust tests** (33 native rebuild, 41 ground navigation, 50 recovery,
63 harness), **992 Python tests**, bot Clippy, formatting, locked Rust 1.89
release build and default-feature check. Native Clippy retains its existing 16
diagnostic identities. A full-library build caught a test-only snapshot helper;
the probe was corrected to use the existing public engine API and validation
was rerun before freezing. The meaningful native fixture test also independently
advances the original world under neutral actions and verifies forecast poses,
landing and contacts, repeatability and live-state retention.

The [manifest](data/rebuild-native-forecast-v1.json) and
[portable bundle](data/rebuild-native-forecast-v1.json.gz) contain all forecasts,
actual comparison windows, cost samples, preceding audits, frozen source,
validation logs and the exporter. Verified full archives remain under
`target/rebuild-native-forecast/v1/archives/`.

The next step is to use this native rollout as the reference for a forecast with
explicit limits on information and work per frame, then test hypothetical
placements before construction. Preserve these positive and negative cases
while reducing or scheduling the native work. Do not add a synchronous 120-step
full-world loop to each placement offset. Complete coarse-site recovery,
precise-position holding through construction and the bearing-343 query mismatch
remain open; no bot default or frontier choice changes here.
