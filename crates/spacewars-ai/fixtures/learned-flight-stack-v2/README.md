# Retained learned flight stack v2

The two exported networks of the match-envelope flight stack, embedded by
`spacewars_ai::flight_stack::FlightStack::retained_v2`:

- `navigation.json`: the [LF-091 refitted navigation network](../../../../docs/flight-motor-steering-v1.md)
  of the [LF-088](../../../../docs/flight-navigation-v2.md) law with the
  [LF-090](../../../../docs/flight-mission-polish-v1.md) side commitment and
  edge-aware detours and the LF-091 straight-line planner waypoint, fitted
  with the motor network below in the loop. It is the C3 flight encoder with
  its heads read as a body-relative direction pair and a speed under the
  `navigation-command-v2` contract (format version 5): the velocity relative
  to the goal, which the host completes with the goal's velocity in the
  pilot's planet frame; speeds under half a unit are stops.
- `motor.json`: the [LF-091 v6 teacher-fitted motor network](../../../../docs/flight-motor-steering-v1.md),
  the [LF-087](../../../../docs/flight-motor-teacher-v5.md) recipe on the v5
  law with the brake's share of the error left out of the attitude, the
  64-wide actor under the `planet-frame-heading-speed-v2` contract: heading
  and speed relative to the pilot's planet frame, speeds to 60, under
  gravity and spin.

`provenance.json` records each file's hash, source experiment and
qualification. The stack holds every navigation command for six ticks while
the motor network flies at 60 Hz, as the training bridge did. Its envelope
is the match world: gravity, orbiting and spinning planets, the sun's heat,
the boundary, goals resting in a planet's spinning frame, practice tasks of
the four C3 families with their seeded gravity and spin, and transfers
between the planets of a generated arena.
