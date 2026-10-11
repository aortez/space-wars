# Continue the original controller after a measured pod opportunity

The four-arm Angular comparison at `430eb8a` reproduced the retained existing
and full controls. Search/arrival alone rebuilds and boards at 21,849, returns
to combat, then loses its ship at 23,824. Placement/forecast alone never
recovers. The full candidate recovers at 21,917 with no further subject loss.
Search is sufficient for this recovery, not for uninterrupted survival.

Test a default-off policy through ordinary RulePilotV4 task creation and intent.
Use the search/arrival configuration that recovered. Native contact-frame and
settling-forecast selection remain off in this experiment. Do not replace the
recovery task or reset the surrounding combat controller.

After the original two-minute pod timeout, review at most 15 full surveys over
15 seconds, at most one per second. Require current clean measured geometry:
a different unrejected site, closer than the current measurement of the
abandoned approach by at least one unit (or that old site no longer exists).
A terrain revision change alone cannot trigger an attempt. Retain identity,
monotonic tick and query guards. Preserve the original clock, rejection history,
landing retry count, relocation budget, native rebuild history and scuttle count.

One accepted opportunity restarts approach control for at most 90 seconds.
Disarmed ticks consume this fixed window. Toggling the option cannot renew it.
The existing one-time boarding exception still requires a newly observed native
rebuild. The policy does not reconsider on-foot stalls, exhausted landing retries,
invalid observations, or late activation outside the original review window.

Compare unchanged blocked control with the opt-in original controller at all
four retained forks for the same 18,000 ticks. Assert the immediate Angular
prefix also follows from selecting the profile at tick zero: identical inputs
and physics through the fork, and identical controller memory at the fork.
Retain all failures, changed actions and new losses. Freeze source and executable
before running; do not tune deadlines or replace a failed outcome.

Check physical rebuild, settling, boarding and combat return, plus ordinary
recovery/navigation/gameplay regression tests and strict bot Clippy. Keep Round
as a separate unresolved on-foot problem. No bot default, deployment, or GitHub
PR change is part of this local experiment.
