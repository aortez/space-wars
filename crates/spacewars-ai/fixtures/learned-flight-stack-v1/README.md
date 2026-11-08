# Retained learned flight stack v1

The two exported networks the composed flight stack runs in gameplay,
embedded by `spacewars_ai::flight_stack::FlightStack::retained`:

- `navigation.json`: the [LF-083 fitted navigation network](../../../../docs/flight-navigation-fit-v1.md),
  the C3 flight encoder with its heads read as a body-relative direction pair
  and a speed under the `navigation-command-v1` contract (format version 4).
- `motor.json`: the [LF-078 v4 teacher-fitted motor network](../../../../docs/flight-motor-teacher-v4.md),
  the unpolished fit that is the motor controller for navigation work.

`provenance.json` records each file's hash, source experiment and
qualification. The stack holds every navigation command for six ticks while
the motor network flies at 60 Hz, as the training bridge did; the
`flight_stack_parity` test replays an archived LF-083 evaluation case through
the gameplay driver and requires the same actions and frames. Both files are
retained artifacts of one build and fixture under the stack's qualified
envelope: zero gravity, a stationary planet, open wings and static geometry.
