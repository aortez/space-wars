# Navigation reference law and composed bridge v1

LF-082, 2026-10-25. The feasibility step of the [navigation proposal](design/learned-navigation-v1.md):
before any navigation network is fitted, a hand-written law reading only the
flight policy observation must drive the qualified motor network through the
agreed task families. Exploratory under the two-tier process: saved task
sets, per-case results and summaries, no frozen protocol. Implementation:
`tools/flight_navigation/reference.py` and `route.py` with tests, and the
composed bridge `crates/spacewars-ai/examples/flight_navigation_bridge.rs`.

## Composed bridge

The bridge runs the two-level stack natively. A `step_command` request
carries a relative heading as a direction pair plus a speed, the bridge
converts it to an absolute heading at the decision tick, derives the motor
inputs through the same `motor::command_inputs` function the motor
diagnostic uses, runs the exported motor network for the held number of
ticks, and returns the flight policy observation, frame, progress and
evidence. It also exposes the C3 task generator. The motor network is the
LF-078 v4 fitted actor; the hold is six ticks. Rust tests cover the command
conversion and the shared motor-input derivation.

## Task sets

The C3 generator's four families at 24 admitted candidates each, generation
seed 7700002, with gravity turned off and the planet made stationary on every
candidate, which only loosens the generator's conservative admission.
Moving-target tasks are generated and reported but excluded from the v1
envelope by the agreed decision. Arrival is the agreed criterion: position
within 3, relative velocity within 1, spin within 0.2, for 30 ticks, with no
collision or damage, before the generator's deadline.

## The law and how it was reached

The law emits, every six ticks: a relative heading and a speed. Speed falls
linearly with goal distance at 0.6 per unit beyond a two-unit offset, capped
at 20, limited by what the clearance along the chosen heading allows for
braking at an assumed 20 units per second squared with an eight-unit margin,
and is zero inside a three-unit stop radius, where the nose is also held.
Clearance along a heading is the free distance of a corridor of half-width 9
read from the 128 rays. Route choice is A* over the observation's own polar
cell map, 23 bands by 64 sectors, with cells touched by geometry blocked and
inflated by a nine-unit hull margin; the heading candidates around the full
circle are scored toward the first waypoint beyond twelve units along that
route, by the distance from each candidate's free endpoint to the waypoint
plus a penalty for blocked corridors.

| Law version | Clear | Obstructed | Altered | What changed |
|---|---|---|---|---|
| naive pursuit, square-root approach | 1/24 | 0/24 | 0/24 | ship arrives too fast, spins chasing the goal direction, never dwells |
| linear approach, nose hold within 8 units, 25-degree clearance cone | 23/24 | 8/24 | 12/24 | arrival works; corridors and carved passages pass; walls, open U shapes and protrusions oscillate or collide |
| one-step lookahead scoring, 12-degree cone | 23/24 | 5/24 | 12/24 | commits to obstacle edges and clips them |
| corridor clearance of half-width 9 | 23/24 | 8/24 | 12/24 | collisions remain on walls and protrusions: straight rays cannot see around an end |
| A* on the sensed cell map | 23/24 | 24/24 | 24/24 | every wall, open U, corridor, passage and protrusion passes |
| nose hold only inside the stop radius | **24/24** | **24/24** | **24/24** | the last failure slid past a goal beside the ship with the nose locked |

Parameter sweeps of corridor width, margin, assumed braking and top speed did
not fix the obstacle families without routing: wider margins turned
collisions into deadlines. The rays do sense obstacle boxes and built
terrain; the check that showed it compared sensed hits against task geometry.

## Result

Final law, all four families, 154 seconds of wall time:

| Family | Passed | Mean ticks to arrival | Outcomes |
|---|---|---|---|
| clear | 24/24 | 634 | all success |
| obstructed | 24/24 | 819 | all success |
| altered | 24/24 | 731 | all success |
| moving | 0/24 | none | deadline; excluded from v1 |

The smallest corridor clearance on any successful run is 11.9 units and
every run uses the full 20-unit speed cap somewhere. Per-case results, task
sets and the summary are under
`data/training-experiments/learned-flight-navigation-reference-v3-20261025`,
with the summary copied to [docs data](data/flight-navigation-reference-v1-summary.json).

## Interpretation and next step

The agreed sensor configuration is sufficient for v1 routing: a law that
reads only the observation's cell map and rays, with the motor network
flying, passes every static task in the four-family generator's envelope.
That is the precondition the proposal set for fitting a navigation network,
which is the next step: supervised fit of this law into the C3 encoder with
the direction-pair-plus-speed action contract, refined on the network's own
visited states, verified closed loop, then polishing pilots at the command
rate. Limits: one generation seed with 24 cases per family, no held-out
seed yet, no moving targets, and a routing law whose cost per decision is a
few milliseconds of Python, fine for the reference and irrelevant to the
network that replaces it.
