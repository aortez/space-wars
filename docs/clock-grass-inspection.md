# Inspecting Clock grass

Start the client with the read-only cutaway enabled, then use Clock Controls to
preview Grass (or let the periodic event plant it):

```sh
SPACEWARS_CLOCK_GRASS_INSPECT=1 cargo run -p engine-client -- --scenario clock
```

The normal Clock keeps its usual presentation unless this variable is set to
`1`. No saved settings are changed. To return to the ordinary presentation,
launch without the variable. This is an inspection view, not a plant editor.

The inset fits between the floor and the digits. It magnifies the first living
organism in stable tissue-ID order, showing its **actual material cells**:

- Pale cells are roots, gold cells are underground runners, green cells are
  shoots, and gray lines are existing tissue connections.
- Brown cells are retained eligible dirt around that organism. Opaque soil in
  the ordinary scene stays intact; the inset is a cutaway of material space,
  not a second physical habitat or a depiction of world-space movement.
- Cell marker size follows tissue size and paid development toward its next
  size. Missing tissue and broken connections disappear after actual loss.
- The organism number in the caption identifies the focus. If it retires, the
  next living organism is selected. An empty inset means no living patch;
  it does not mean the retained soil has been deleted.

Only one organism is magnified, bounded by the existing 64-tissue cap and finite
soil field. Widely separated patches are not combined into a misleading single
network. A changing focus can change the magnification; compare matching tissue
IDs/cells, not only the screen footprint.

`spacewars-cli clock state` exposes the matching optional
`grass.inspection` snapshot when the view is enabled. It includes tissue IDs,
roles, cells, sizes, health, development/investment, support identity, actual
connections and retained soil cells. These are copied from the same authoritative
plant/material state used by the inset. The view never advances physiology,
credits resources, changes support or accelerates time. Pausing freezes it with
the rest of the Clock.

Default room light remains dim. The 3–5 minute establishment target requires
favorable conditions; an inspection view alone supplies no light or nutrients.
The named favorable and shortage fixtures used by the registered real-client
workflows are separate from this user-facing inspection switch.

Both renderer progress workflows enable the cutaway and keep ordered PNG/state
pairs plus `grass-<renderer>-underground-history.json`. They assert real root and
runner presence and spread beyond initial underground cells at normal speed.
The shortage workflows retain the same view and assert that it empties on actual
plant retirement. New screenshots must be collected by the trusted launcher;
historical captures without the cutaway do not prove this visualization.

Automated real-client workflows launch with `--ignore-physical-controllers`.
This per-process option skips hardware enumeration and polling; keyboard and
bounded public control-API input remain available. It does not change support,
physics, biology, or saved controller settings. Intentional duck/player interaction
coverage still uses the normal public API and deterministic actor fixtures.
The short grass admission smoke cases use default room light and ordinary views;
only the long named growth/shortage cases enable the diagnostic cutaway and their
explicit environmental fixtures. Smoke captures are retained even on success.
