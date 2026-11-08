# Retained learned flight stack v3

The two exported networks of the match-envelope flight stack whose commands
carry a turn sense, embedded by
`spacewars_ai::flight_stack::FlightStack::retained_v3`:

- `navigation.json`: the [LF-100 navigation refit](../../../../docs/flight-turn-sense-v1.md)
  of the [LF-099 navigation network](../../../../docs/flight-motor-contact-v1.md),
  the LF-099 recipe (the [LF-093](../../../../docs/flight-long-transfers-v1.md)
  recipe of the [LF-088](../../../../docs/flight-navigation-v2.md) law with
  the [LF-090](../../../../docs/flight-mission-polish-v1.md) side commitment
  and edge-aware detours, the [LF-091](../../../../docs/flight-motor-steering-v1.md)
  straight-line planner waypoint, the [LF-095](../../../../docs/flight-sun-detour-v1.md)
  lead on the sun's detour waypoint and heat-free momentum brake, and long
  transfers as a sixth family) with climbs from rest on the ground as a
  seventh family, fitted with the motor network below in the loop. It is
  the C3 flight encoder with four heads read under the
  `navigation-command-v3` contract (format version 6): a body-relative
  direction pair and a speed, the velocity relative to the goal, which the
  host completes with the goal's velocity in the pilot's planet frame, speeds
  under half a unit being stops, and a turn sense, read as -1, 0 or +1 from
  the fourth head's tanh at half, which is the first element of the
  command's advisory vector of four. The sense imitates the v3 law's rule:
  with the ground against the hull and the command's heading beyond the
  thrust gate, a turn under way is kept, a turn the ground has stopped is
  reversed, and otherwise the nose turns away from the ground beside it.
- `motor.json`: the [LF-100 v8 teacher-fitted motor network](../../../../docs/flight-turn-sense-v1.md),
  the [LF-087](../../../../docs/flight-motor-teacher-v5.md) recipe on the
  [LF-099](../../../../docs/flight-motor-contact-v1.md) v7 law with the
  sense's override, a turn at the limit the sense's way that slows into the
  attitude vector with the brake off, fitted with the LF-099 contact tasks
  and a blocked family, ships on the ground whose shorter turn goes through
  it, the 64-wide actor under the `planet-frame-heading-speed-sense-v3`
  contract: heading and speed relative to the pilot's planet frame, speeds
  to 60, under gravity and spin, through surface contact, and the command's
  advisory vector as four more inputs, of which the turn sense is the first.

`provenance.json` records each file's hash, source experiment and
qualification. The stack holds every navigation command for six ticks while
the motor network flies at 60 Hz, as the training bridge did, and the
motor's environment read its reference sense at the same cadence. Its
envelope is the match world: gravity, orbiting and spinning planets, the
sun's heat, the boundary, goals resting in a planet's spinning frame,
practice tasks of the four C3 families with their seeded gravity and spin,
transfers between the planets of a generated arena, starting up to 3,000
units from their entry, climbs from the ground and through contact with it,
and turns on the ground that the ground blocks one way.

The [v2 stack](../learned-flight-stack-v2/README.md) stays retained beside
it: its exports lifted into the v3 formats with the advisory weights zero fly
exactly as they did, which gates the v3 plumbing.
