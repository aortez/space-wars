# Strategy checkpoint review against current main

Local review found no blocking correctness issue in checkpoint
`0e1dae65aa53fb09624ca601e64074691b4de6a0` (`bot-integrated-candidate`). The
checkpoint merges cleanly with main `b2f297ed7c692980e34529a810f9735263e5885f`.
The combined tree is `12d291d8a1557c6facff51ca8cf5893792a624b5`.
Publication and GitHub checks remain separate from this local review.

## What was reviewed

The runtime diff is confined to twelve files in `crates/spacewars-ai`, including
the headless harness and tests. Review covered the shared recovery state machine,
the two optional combat controllers, first-site commitment, mission/boundary
integration, configuration and reset behavior, and their focused tests. The
[checkpoint](bot-strategy-checkpoint.md) maps the implementation and preceding
experiments; the [corrected-runtime comparison](recovery-strategies-results.md)
is the current strategy decision.

- Recovery requires a newly observed native rebuild, binds the vehicle identity,
  preserves the rebuild high-water mark, and grants at most one bounded boarding
  opportunity per task. It preserves the original clock and replacement history,
  recognizes physical boarding, and does not reopen invalid observations or
  authorize another scuttle.
- Pursuit-climb laser is disabled by default and limited to v13. An admitted
  request preserves native flight and cannon controls, respects existing weapon
  breaks and task priorities, and uses the present observation's firing window.
- Acquisition defense requires a fresh hostile hit before the first capture
  site or surface commitment. It has a fixed deadline and yields to recovery and
  surface tasks. Clearance rejection preserves the native intent and task and
  starts neither a defense deadline nor a source-planet cooldown.
- Reset preserves selected options while clearing episode state. The optional
  features do not alter interactive selections or defaults. The shared recovery
  correction does affect ordinary recovery, including opponents; the final
  strategy comparison explicitly includes that effect.

The evidence review checked the final comparison's frozen inputs and runtime,
complete case/contrast counts, failed criteria, denominator accounting,
post-victory departure handling, recovery receipts and archive verification.
This is not a claim of exhaustive manual review of every historical trace or
every line of the evaluation tools.

## Compatibility checks

Checks ran in an isolated detached worktree containing the checkpoint plus a
no-commit merge of the exact main above, using Rust 1.89.0 and the repository
lockfile. The worktree was removed after verification; logs are retained.

| Check | Result |
| --- | --- |
| `cargo test --locked -p spacewars-ai --all-targets` | 616 passed, zero failed; includes physical mission and recovery integration suites. |
| `python3 -m unittest discover -s tools/tests -p 'test_*.py'` | 889 passed. |
| `cargo clippy --locked -p spacewars-ai --all-targets --no-deps -- -D warnings` | Passed. |
| `cargo fmt --all -- --check` | Passed. |

Rust tests used `RUST_MIN_STACK=16777216`. The logs and exact check manifest are
under `target/bot-checkpoint-review/`; a copy is included with the separate
integration-factor evidence bundle. These checks do not include the full
interactive client, deployment or hardware timing.

Main now includes terrain and asteroid physics changes. Passing compatibility
checks does not transfer the old binary's 72-game strategy results to that new
physics. The [factor diagnosis](integration-factors-plan.md) therefore retains
the original qualified binary and runs on a separate branch. A future candidate
needs an independent comparison on its intended current runtime before any
selection or default promotion.
