# Navigation polishing pilots v1

LF-084, 2026-10-26. The polishing step of the
[navigation plan](design/learned-navigation-v1.md), piloted before any frozen
comparison: PPO at the command rate from the
[LF-083 fitted export](flight-navigation-fit-v1.md), with the v4 motor
network flying underneath. Exploratory under the two-tier process: two rounds
of cells, per-update histories, closed-loop evaluations of the mean policy on
the development and held-out sets, traces of every evaluated case, no frozen
protocol. Implementation: `tools/flight_navigation/pilot.py` with tests, and
the exploration distributions on `NavigationModel` in `model.py`. The result
is negative: no cell arrives faster than the fitted export, and every cell
loses arrivals, most by forgetting how to stop inside the arrival radius.

## Recipe

Sixteen persistent slots cycle through 360 static tasks generated at seed
7720001, never used by the fit. Each decision is one six-tick command, a
macro-step whose discount is the per-tick gamma 0.998 to the power of the
ticks taken, so the per-tick horizon is preserved as in the motor pilots. The
reward of a decision is a time cost of 0.001 per tick, a bounded potential on
goal distance, minus tanh of distance over 60, applied as potential-based
shaping so that any route to the goal earns the same total, and the C3
physical terminals: plus ten on arrival, minus ten on collision or loss,
minus two on the deadline. Ninety-six decisions per slot per update, GAE
lambda 0.95, four epochs of minibatch 256, Adam at 1e-4, clip 0.2, KL stop at
0.03, entropy weight 0.001. Eight critic-only warm-up updates precede the
first actor update because the fitted export's critic is untrained. The
optional anchor is the fit loss toward the law's command on a quarter of the
visited states, labelled by the law during collection. Evaluation runs the
exported mean policy through native inference on the 72 development and 72
held-out static tasks of LF-083. The release-profile bridge was used after a
preflight showed it bitwise identical to the ci build on generation,
inference and closed-loop runs.

## The first launch, aborted

With a Gaussian of deviation 0.3 on all three latents, no sampled episode
could finish: the arrival dwell needs spin under 0.2 for thirty ticks, the
motor turns the nose at twice the heading error, and a heading jitter above
about six degrees at any of five consecutive decisions breaks it, so every
window ended in deadlines. Four of five cells then crashed on a refused
command: a sampled pair near zero was rescaled to the bridge's minimum norm
in double precision and parsed below it in single precision, which happens
for half of all such pairs. Both fixes are in the code: the heading
deviation scales with the commanded speed through a detached sigmoid floored
at a tenth, and the pair floor sits at 0.125. The aborted cells are kept
under `aborted-unscaled-noise`.

## Round one: Gaussian heading and speed

| Cell | Anchor | Development | Held out | Development mean ticks, clear / obstructed / altered | Training failures | KL stops | Minibatches per update |
|---|---|---|---|---|---|---|---|
| fitted export | | 72/72 | 71/72 | 668 / 864 / 735 | | | |
| seed 17, no anchor | 0 | 65/72 | 65/72 | 789 / 1151 / 921 | 111 | 119 of 120 | 7.2 of 24 |
| seed 29, no anchor | 0 | 23/72 | 18/72 | 1060 / 1043 / 1229 | 50 | 120 of 120 | 3.7 of 24 |
| seed 17, anchor 1 | command | 65/72 | 66/72 | 939 / 1184 / 1002 | 25 | 119 of 120 | 8.2 of 24 |
| seed 29, anchor 1 | command | 67/72 | 66/72 | 801 / 967 / 775 | 33 | 119 of 120 | 7.3 of 24 |

The KL stop fired in nearly every update, often after a single minibatch
step. The speed-scaled heading deviation makes the likelihood hypersensitive
wherever the policy commands a stop: a deviation of 0.03 in latent space
turns a mean shift of 0.007 into the whole KL budget, so one Adam step at
1e-4 both ends the update and moves the policy where it is most fragile. The
anchor roughly halves the training failures and keeps the sampled episodes
arriving, but the mean policy still drifts.

## Round two: deterministic heading, speed exploration, heading anchor

The motor stage's answer to the same problem, LF-080: keep the output that
must not jitter at its mean, explore the one that can improve, and leash the
deterministic output to the law. Here the heading pair stays at its mean, the
speed latent is sampled with deviation 0.3, the likelihood covers the speed
only, and the anchor is the squared error of the pair alone, so the speed
schedule is free to change.

| Cell | Anchor | Development | Held out | Development mean ticks | Training failures | KL stops | Window mean ticks, first ten to last ten updates |
|---|---|---|---|---|---|---|---|
| seed 17, no anchor | 0 | 0/72 | 0/72 | none | 25 | 69 of 120 | 917 to 1356 |
| seed 17, anchor 1 | heading | 70/72 | 68/72 | 972 / 909 / 800 | 13 | 42 of 120 | 822 to 882 |
| seed 29, anchor 1 | heading | 39/72 | 41/72 | 1218 / 1059 / 1005 | 7 | 35 of 120 | 799 to 902 |

Updates now run most of their minibatches, and the anchored cells train
almost without failures: 13 and 7 physical failures in about 1,300 episodes
each, with every episode of most windows arriving. The mean policy still
loses arrivals, and the sampled windows never get faster: the mean ticks of
arriving episodes rise over training in every cell. Without the anchor the
heading drifts through the shared encoder although it has no likelihood
term, exactly the steering drift of LF-079, and the policy stops arriving
entirely by update 40.

## How the policies fail

Every failing evaluation case was traced. The most common mode, 157 of the
175 development failures across the seven completed cells and the only mode
in three of them, is the same: the ship reaches the goal and parks or slowly
orbits at about three units, around 0.9 to 1.1 units per second, with the
nose turning at 0.2 to 0.5 radians per second, never inside all three
tolerances for thirty ticks. Of the seed-29 anchored cell's 33 development
failures, every one is this mode, and 31 of 31 on the held-out set. The law
commands heading zero and speed zero inside the stop radius; that hold is a
discontinuity the fit reproduced and polishing erased first, because the
speed exploration there costs nothing in the sampled episodes, where noise
jiggles the ship inside the radius, and the critic cannot see the
difference. The other modes are rare: collisions on walls at full speed in
the unanchored round-one cells, and a collapsed heading pair at some episode
starts, refused as an invalid command.

The learning signal is the reason nothing improves. A finish one hundred
ticks sooner is worth about half a unit of return, through the discount on
the terminal and the time cost, spread across some 130 decisions, while the
critic's error after warm-up stays between 0.3 and 0.9 in the same units.
The advantage of a faster speed schedule is below the noise floor, and the
updates that do pass the KL guard spend their budget on the stop region.

## Interpretation and next step

Polishing at the command rate, as piloted, is not a path to a faster or
safer navigation policy than the fitted export, and it is not retention-safe
in any of the four recipes. The pilots reproduce the motor stage's findings
in order: the exploration noise must not touch the output that gates the
success criterion, the deterministic output drifts without a supervised
leash, and the leash preserves behavior without producing improvement. The
fitted LF-083 export remains the navigation controller.

Three directions remain, and the choice is a plan decision rather than
another pilot: integrate the fitted stack as it stands and revisit polishing
only against a measured deficiency; tune the law's approach schedule and
refit, the deterministic route to faster arrival that the two supervised
fits have made cheap; or change the learning problem, for instance a search
over the law's few gains by closed-loop evaluation, or a denser objective
on time to go, before any further PPO. Per-cell histories, exports,
checkpoints, evaluation traces and grid summaries are under
`data/training-experiments/learned-flight-navigation-polish-pilots-v1-20261026`
and `-v2-20261026`, with both grids copied to
[docs data](data/flight-navigation-polish-pilots-v1-summary.json).
