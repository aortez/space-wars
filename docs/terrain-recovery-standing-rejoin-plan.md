# Bounded standing-ground rejoin experiment

The retained Angular/off mission has a supported, balanced pilot outside the
standing graph. Rebuild search never starts the existing ground-rejoin
controller, despite measured crawl openings. A fresh recovery task repeats
the same inputs and fails. Test one explicit opt-in connection before adding
any on-foot reset policy or changing native placement cost.

Use seed 20202, Angular, collapse off, candidate P2 from the frozen mission
matrix (`e36ee55`, results `cc8413f`). Prespecify two next-decision forks:

* 8025: the first transition into failed-placement search, with both short
  crawl directions measured as open.
* 8325: the last scheduled full ground observation before the original block
  at 8326; this tests the later, narrower opportunity independently.

Reconstruct and verify every original input, decision checksum and scenario
checksum. Clone the entire native world, bots and host queues. Compare the
unchanged mission with the same current recovery task having only its new
ground-rejoin option enabled. Do not reset controller memory, edit physics,
feed unscheduled diagnostic maps to the bot or change other profile options.
Continue both reactive arms to native match end, at most tick 36000. Retain both
forks and all outcomes; do not select another fork after seeing a result.

The opt-in attempt requires current owned support, balanced/stable posture,
obstructed upright clearance, a measured crawl opening and a fresh ground map
whose nearest standing node is 3–8 units from the foot. It may run once per
recovery, bounded by eight seconds and six units, using the existing ground
rejoin controller's short horizontal controls. No jump or flight is authorized
by this attempt. Missing observations and unavailable queries do not renew the
deadline. Existing overall recovery deadlines and replacement counts persist.

Only fresh, valid ground measurements within the ordinary strict 3-unit start
threshold, together with current stable support and upright clearance, establish
a successful rejoin. Then ordinary native rebuild and boarding rules resume.
Pause the original five-second missing-site timer only for the elapsed rejoin
duration, preserving its remaining time. A new available ship or lost ownership
cancels the attempt. Failure does not grant another attempt.

Record every attempted control with posture, support, ground map, controller
state and resulting native physics. Separate movement, standing-ground rejoin,
native rebuild and complete aboard recovery. The acceptance criterion for
recoverability is a complete native recovery, not merely a clear probe or a
changed action. Verify unchanged controls against the entire retained trace.
Run contract tests for bounds, stale/malformed observations, cancellation, clone,
reset, overall deadlines and disabled selection, then the ordinary recovery and
gameplay checks. Freeze source and executable before native comparisons.

This experiment does not enable the option in the mission host, launcher or bot
defaults. Native cost reduction, the Round hatch executor and the Angular/on
replacement distance gate remain separate. Any useful result still needs wider
mission and final-terrain integration before promotion.
