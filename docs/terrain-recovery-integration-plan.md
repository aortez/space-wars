# Compare recovery on the frozen terrain handoff

Use the complete terrain package dated 2026-10-09, archive SHA-256
`cc8df0c516ab948690ce0ba704745266b4600705544e2acec5fac63ee86750e9`,
tested-source manifest `095c9d5e331c44ad401dd18d06ba5252a39ee40529b2649ee26d3744a27b3763`.
The published #174 head alone omits the tested terrain changes. Preserve that
package and its original controls, failures, diagnostics, and source identities.

The supplied frozen source is reconstructed as local commit `f5c368b`; merging
it into bot checkpoint `cada1eb` produces `6083226`. The sole text conflict is
the engine-rapier module declarations; retain both sides. The local merge is
for evaluation and is not a merge of either GitHub PR.

First repeat all four supplied controls using its retained executable and
compare every output file. Build the combined source and run its unchanged
diagnostic separately, retaining any changed prefix or later outcome as new
combined-runtime evidence. Never silently update an old replay expectation.

The candidate comparison uses the same four fixed cases, seeds, seats, fork
ticks and 18,000-tick continuation as the handoff. Reconstruct each common
prefix from tick zero, then clone into three arms: blocked existing control,
fresh existing recovery, and fresh candidate recovery. Both fresh tasks have
identical initialization before candidate options are enabled. Both surrounding
RulePilotV4 controllers retain combat RNG, clocks, loss history and caches;
the opponent remains reactive. World, health, input latches, native history,
terrain and environmental pressure are not reset.

Explicit profile `terrain_recovery_contact_preview_v1` uses the previous
successful contact-handoff settings: completed contact frame, bounded refinement
and staging, continuous staging with measured-map handoff, precise arrival,
settling forecast selection, selected-preview normal capture/arrival, and
bounded high-terrain flight sensing. Radial placement, the old case-specific
radial-veto switch, footing holding/rechecking, and support-alignment rejection
remain disabled. The combat observation adapter performs one recovery survey,
not an ordinary survey followed by a replacement survey.

Only the fresh subject task receives these options. Native candidate options
are disabled after that task completes or the diagnostic observes a new ship
loss. Normal capture/arrival rejection remains a rejection: record it, clear
the unusable context, and retain ordinary native placement guards. A staging
walk cannot capture a construction-site normal. Log native selection work,
normal lifecycle, surveys and actual terrain-forecast provision to distinguish
configured features from features that execute.

No retry eligibility or production default changes are part of this comparison.
Keep native timers, search/relocation caps, forecast horizons and per-update
work bounds. No outcome tuning, seed replacement or simulation reruns after
freezing. An interrupted or failed run remains evidence; any harness repair
must be documented separately. Success requires actual subject recovery and
no new ship loss, with native rebuild, settling and boarding evidence. A preview,
claim, improved diagnostic reason or changed telemetry is not full recovery.

Compare old and new runtime controls separately, then fresh existing versus
fresh candidate on identical combined physics. Keep pod timeout and unsupported
claim failures distinct. Report actual progress and unresolved causes even if
all cases fail. Run focused native/AI checks and retain command, source and
binary hashes before execution. All integration work remains local; #174 owns
demonstrated terrain/support defects and #178 owns recovery-policy decisions.
