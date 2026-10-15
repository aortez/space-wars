# Organism perception for learned and hybrid brains

Draft perception design for the [evolutionary training plan](evolutionary-training.md).
An organism needs a meaningful view of its world without receiving the entire
simulation or an unbounded input vector. Use a shared, typed observation with
coarse world context, bounded object records, and detailed nearby geometry.
Whole brains and individual components choose declared views of those facts.

The nine proposed [arrival guidance inputs](guidance-component-v1.md) are one
small component view. They are not the whole pilot's senses. Review this broader
representation before freezing that encoder or a whole-brain encoder. The
block structure below is proposed; capacities and numerical layouts need the
specific checks at the end.

## Observation and encoding are separate contracts

The scenario supplies observed facts in physical units through a named sensor
policy. A versioned feature encoder selects blocks, transforms coordinates,
normalizes values, and presents the input expected by a particular model.
Handwritten components can consume the corresponding typed facts directly.

A whole-brain model can consume the wider observation. An arrival component
can consume only self motion and a supplied goal; an obstacle-aware flight
component would additionally need local geometry and moving contacts. Optional
blocks let the interface grow without making every component pay for every
sensor or forcing old genomes to accept a different input meaning.

Pin the sensor policy, selected blocks, capacities, frames, normalization,
selection rules, and update schedule in the brain/model compatibility record.
Adding a terrain channel or reinterpreting a feature produces a new encoder
version. New scenario layouts need not change that contract if they can supply
the same observations. NES and native organisms retain their own contracts;
this is not a universal observation forced on every physics engine.

## Extending to other bodies and worlds

The block structure is a framework convention. Individual profiles declare
the blocks, categories, fields, capacities, and action meanings they support;
they need not anticipate every future vehicle, weapon, material, or mechanic.
The [world and build contracts](evolutionary-training.md#compatibility-and-saved-records)
establish whether a particular brain can run in a selected configuration.
Its [action profile](brain-actions.md#control-profiles) defines the corresponding
controls, capability/readiness states, and body-transition behavior.

Describe relevant capabilities and physical properties as well as type. A
future flight profile spanning several ship builds may need body dimensions,
thrust and turning response, braking capability, and equipment state in its
observations. A build ID alone does not express those differences. Keep exact
build identity and configuration in episode provenance; expose only the facts
allowed by the sensor policy to the model. A property recorded in provenance
but absent from the inputs cannot tell the brain how to adapt during play.

An object's common fields can describe a new design when its relevant behavior
fits the existing contract. Add role-specific fields or blocks for new
mechanics when needed, with corresponding action contracts for new controls.
Do not append every possible ship's fields to one permanently growing vector.
Likewise, do not reinterpret a known category to stand for a new mechanic.
An unknown-category encoding is usable only when explicitly supported and the
remaining observations still supply the required semantics; it must not erase
an obstacle or imply that its hazards are understood.

Optional capabilities have declared masks and meanings. Distinguish a tool
that the body lacks, one temporarily unavailable, an unavailable measurement,
and a mechanic the profile cannot represent. A new schema or action binding
that changes feature dimensions, field meanings, or action semantics creates
a new model contract; existing genomes keep their original feature layout and
output meanings. If a body changes during play, its new capability state and
the coordinator's supported transition must agree before it receives control.

Training over different worlds and builds is an explicit recipe, not an
automatic consequence of providing these descriptors. Record the combinations
seen during training and evaluation separately from execution compatibility.
Specialist brains and brains trained across a broader range can coexist.

## Proposed observation blocks

| Block | Facts to represent | Decisions it enables |
| --- | --- | --- |
| Self and game state | Pilot versus vehicle identity, ship/pod/on-foot form, motion and gravity, body size, wing state, separate pilot/hull health, available controls, weapon/jetpack resources, support, interaction eligibility, and visible match time/status. | Control the current body, manage resources, recognize damage and mode changes. |
| Strategic landmarks | Bounded planet/objective records with relative position, motion, size/bounds, ownership, and observable claim state; sun and arena-boundary context. | Choose destinations and understand broad geography. |
| Moving contacts | Opponent, assigned vehicle when on foot, nearby actors, projectiles, asteroids, and fragments, with type, relation, relative motion, extent, and permitted status. | Pursue, aim, evade, and find the vehicle again. |
| Local geometry | Measured clearance around the body plus a small patch of nearby material when needed. | Distinguish open space, walls, pits, ledges, roofs, and blocked passages. |
| Task supplied by another component | Target identity, position/velocity or another explicitly typed request, and the component's current ownership/capability context. | Carry out a selected task inside a hybrid. |

A whole brain chooses its own goals. The task block does not give it a hidden
handwritten commander's answer. During a component experiment, the supplying
planner or practice target source is an explicit part of the saved brain.

Use facts with gameplay meaning. "Can interact now" can come from the same
eligibility check that governs the ordinary action. "This is the best planet"
or "this route will succeed" is planning advice, not a raw sense. Existing
route graphs, landing rankings, forecast plans, and value estimates may be
useful optional helper outputs in a hybrid, but their policy/version and cost
must be named rather than silently becoming part of a supposedly independent
learned brain's observation.

## Coarse world information and detailed nearby terrain

Planet records need enough information for strategic navigation, not the
material field for every planet. A planet center and radius are useful broad
context, but cannot describe a crater or certify a landing. Keep those coarse
bounds separate from actual local surface measurements.

The proposed local geometry combines two scales:

- A fixed set of directions measures clearance over a useful maneuvering
  range. Return whether a query completed, whether it hit, distance, surface
  normal, and relative closing motion where available. Ray or swept-shape
  choice is part of the profile; a clear center ray alone does not establish
  clearance for a ship or a spaceling capsule.
- A small patch near the controlled body preserves nearby material shape.
  Candidate channels are occupied coverage, whether the cell is known, and a
  material property relevant to destruction/mining. Cover a defined physical
  area rather than resizing the patch to the current obstacle. Include actual
  body dimensions and support state so the brain can relate gaps to its size.

Distance samples give reach; the patch gives local shape, including geometry
that a handful of rays might miss. Neither representation may label detached
debris as valid planet footing. Actual support and interaction eligibility stay
separate facts from the scenario's shared mechanics.

The patch is the largest design uncertainty. Sampling only cell centers can
miss a thin wall; averaging solid and empty space can erase a narrow passage.
Test occupied coverage or a bounded distance representation against real
craters, hatch openings, overhangs, and loose fragments before choosing cell
size and channels. A compact patch is not automatically sufficient for a whole
brain to learn recovery or mining.

Keep a larger semantic grid, or a coarse wide grid plus a fine local grid, as
candidate encodings. Object records and spatial maps complement each other;
the framework does not require all profiles to choose the same sparse/dense
balance. The 9 by 9 patch in the sizing example is not a resolution limit.

Use one declared actor-relative frame per encoded spatial block. Aboard, the
actor anchor follows the occupied vehicle; on foot it follows the spaceling,
while the assigned vehicle becomes a separate contact. Frame changes are
explicit in form/identity metadata. If a ground-oriented patch proves more
useful than a body-oriented patch, pin its transform and orientation features
in the encoder rather than changing frames invisibly between modes.

## Object counts and stable meaning

Keep records semantically consistent: an entity has a type, a relation to the
observer, local position/velocity, physical extent, and fields with explicit
availability. Presence masks distinguish unused slots from real objects at
zero distance. Encode categories explicitly, not as an arbitrary floating-point
type number whose ordering the network is expected to interpret.

Reserve meaningful slots or quotas for essential references such as the
opponent, occupied/support body, assigned vehicle, and a component's selected
target. Select the remaining contacts using a deterministic proximity/threat
rule that includes relative motion. Choosing only the nearest objects can
discard a fast incoming projectile behind slower debris.

Stable gameplay IDs support tracking and tie-breaking; raw IDs and vector
indices are not numeric neural features. Preserve identities in retained slots
when practical, and expose a slot-changed flag if a recurrent model might
otherwise confuse one contact with its replacement. Do not depend on physics
container iteration order.

Expose that contacts or landmarks were omitted when a cap is reached. Extra
planets in a compatible scenario do not become nonexistent because only a
subset fits; broad ownership counts and overflow/coverage information can
preserve that distinction. A changed capacity is an encoder version change.
The same bounded model may still try a larger world, with that limited view
and separate evaluation evidence.

## Unknown information and sensing permissions

Each expensive block carries a source tick, readiness, and validity against
the relevant world/terrain revision. Per-item masks mark missing fields. Treat
these states differently: observed empty, no hit within sensor range, not
requested, outside coverage, deferred, and invalidated by a terrain change.
Unknown space must never become free space through zero padding.

Metadata needed to validate a block remains typed. Encoders also provide the
learner with the availability, age, slot-change, or overflow signals that can
affect its decision; these count toward the input budget. Raw revision IDs are
not a substitute for a meaningful stale/valid indicator.

For the initial native-pilot profile, preserve strategic overview and explicit
radar-like actor awareness rather than introducing fog of war incidentally.
Local geometry is a declared bounded query, not automatically camera vision.
Radar knowledge, geometric line of sight, weapon readiness, and an unobstructed
shot are different facts. Any later limited-vision profile must specify its
range, occlusion, remembered contacts, and disappearance rules.

Use the same sensing policy and resource allowance when comparing learned and
handwritten alternatives. A model must not gain private opponent intentions,
future simulation states, world-generation seeds, fitness, or evaluator-only
deadlines. The visible match clock is legitimate game state and differs from
an artificial training cutoff. Old rule policies with a different information
contract remain labelled baselines, not evidence of a matched-input comparison.

## Compactness and sensing cost

Keep the world description bounded at the source and the neural representation
bounded by its encoder. A model may flatten the blocks or use separate internal
encoders; that network choice does not change what the organism is allowed to
observe. Avoid appending a large history of world snapshots by default. Measured
velocities, bounded contact tracking, and declared brain memory address temporal
information without multiplying every input block by an arbitrary frame count.

The following is an illustrative sizing worksheet, not a frozen field layout:

| Allocation | Example scalar budget |
| --- | ---: |
| Self, game, optional task, and block status fields | 64 |
| Four strategic records at 20 values each | 80 |
| Eight moving-contact records at 24 values each | 192 |
| Sixteen clearance samples at 6 values each | 96 |
| A 9 by 9 local patch with three channels | 243 |
| Total for this example | 675 |

Presence/category flags and relevant age/change information must fit within
these allocations or be counted explicitly when the field map is written.
This suggests a planning target of several hundred values for a broad pilot
view, while a component may use tens. The example does not prove that four
landmarks, eight contacts, or a 9 by 9 patch suffice. Prefer evidence about lost
decisions over an arbitrary requirement to fit a round input count.

Input count is only one cost. Eight returned contacts can still require an
unbounded scan, and a small patch can trigger expensive terrain queries. Bound
sensor traversal, query count, scratch memory, and update work as well as output
size. Share identical samples across components, and do not request terrain or
landing surveys for a component that only needs the arrival motion fields.

Refresh cheap self motion each physics tick. Choose a deterministic cadence for
costly blocks, invalidate affected terrain data after edits, and report deferred
or stale work explicitly. Sensing limits must be based on counted work/ticks,
not wall-clock cutoffs that change decisions across machines. Measure sensing,
encoding, and inference separately on the intended hardware.

## Existing facts and work still needed

Current material observations already expose useful self, form, gravity,
support, recovery, combat, match, and broad planet information. They also carry
large derived planning structures. The new perception must select factual
fields deliberately; serializing the existing nested mission observation into
a flat neural vector would mix perception and planner decisions.

The material observation's strategic/local split and survey validity are
reusable design foundations. A uniform compact moving-contact block, the
proposed clearance/patch combination, and a full neural encoder still need
design and implementation. The historical classic ship radar has bounded
hazards; that is a useful precedent, not proof that equivalent material-world
contacts are already implemented.

## Checks before freezing an encoder

Use paired situations where a correct decision changes and verify that the
representation preserves the difference:

- Identical target and ship motion with open space versus a wall across the
  approach. The nine arrival inputs can match; the broader geometry must differ.
- Similar planet bounds with a usable hatch corridor versus a thin obstruction,
  and a traversable ledge versus a pit. Compare patch/range readings at the
  actual ship and spaceling dimensions.
- Equal contact position with approaching versus receding motion, including a
  fast threat when debris exceeds the contact cap.
- Full ship, pod, and on-foot states with different available controls and
  separate pilot/hull health. A body change must not silently change a field's
  meaning.
- A freshly measured clear region versus an unavailable or stale query after
  bombardment. Identical padded zeros must not hide that difference.
- Reordered world storage and contact churn. Ensure stable selection, identity
  changes, and omitted-contact reporting remain deterministic.
- Two supported builds with different size or control response, and a world
  introducing an unsupported mechanic. Check that relevant build differences
  remain observable and that missing semantics produce a compatibility reason
  rather than a silently reinterpreted input. Exercise declared optional
  capabilities, including absent versus temporarily unavailable equipment.

These probes establish that the observation can distinguish important states;
they do not prove that a learner will use the information successfully. Then
compare simpler and richer encoders on fixed cases, measure sensor/inference
cost, and retain failures. Choose patch resolution, contact quotas, ranges,
cadences, and feature scales from those results before long training runs.

The immediate planning deliverable is a field-and-frame inventory for each
block and the distinction probes above. The arrival experiment can remain the
first learning task while this broader contract establishes its proper scope.

## Implementation references

- [Mission observations](../../scenarios/spacewars/src/surface_sortie/mission.rs)
  separate cheap world context from the local detailed observation and expose
  opponent, sun, boundary, and match facts.
- [Pilot observations](../../scenarios/spacewars/src/surface_sortie/pilot.rs),
  [flight observations](../../scenarios/spacewars/src/surface_sortie/flight.rs),
  and [combat observations](../../scenarios/spacewars/src/surface_sortie/combat.rs)
  provide the current actor and control-related fields.
- [Recovery sensors](../../scenarios/spacewars/src/surface_sortie/recovery_sensors.rs)
  and [ground navigation](../../scenarios/spacewars/src/surface_sortie/ground_navigation.rs)
  include derived plans as well as physical observations; those roles need to
  stay distinct in the learned interface.
- [Landing objective survey validity](../../scenarios/spacewars/src/surface_sortie/landing_objective.rs)
  records source/validated ticks and distinguishes revalidated routes from
  unknown omitted candidates.
- [Classic ship observations](../../scenarios/spacewars/src/lib.rs) define the
  historical full-map radar and bounded hazard list, separate from the current
  material-pilot observation path.
