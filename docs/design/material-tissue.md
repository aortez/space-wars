# Tissue integrity (#328)

`engine_material::TissueIntegrity` keeps the mechanical integrity of a plant
world's tissue: one material [region](material-damage.md) for each tissue,
sized by its role's material and its current size. Loads charge it through the
same laws as any other region, so safe loads cost nothing however often they
come, and the charge that exhausts a tissue reports it once. The host decides
what that failure means. Integrity is kept apart from the tissue's biological
health, and no rigid body is made for any tissue.

## Integrity and health

A tissue's `health` in [plant physiology](../plants.md) is its viability:
shortage and shade lower it, and sufficient resources and light restore it.
Mechanical integrity is the region's remaining work capacity. The two never
cross. Physiology does not read or restore integrity, and the adapter does not
read or change health. So:

- A tissue damaged by a hit and then fed back to full health keeps its damage.
- A tissue that starves or is shaded to death keeps its integrity, and as
  dead wood it still bears load and can still be crushed.
- Repairing integrity, or restoring a save made while the tissue lived, does
  not revive it. It stays dead and cannot develop or grow.

`TissueTable` gives each role, in the plant `Config` tables' order of root,
runner, shoot, moss and wood, a material and the represented volume each unit
of size holds. Every size up to the plant world's `max_size` must make a valid
region. Its `identity` is caller-assigned, and saves carry it.

`Plants::damage` stays the hosts' existing cut and crush path, as it is in
Clock today. Moving hosts' tissue damage onto this adapter belongs to their
own work: Spacewars and Scorched Earth grass (#302, #311), trees (#224) and
weapon deposition (#329).

## Loads

The host measures each load as it would for any region, and the adapter
supplies the tissue's region:

- `strike` evaluates the impact law with the striker as side `a` and the
  tissue as side `b`, and charges both.
- `press` evaluates a sustained load resting on the tissue and charges it.
- `charge` takes work that another response path allocated to the tissue,
  such as a weapon's deposition, and returns what it could not absorb.

`region(id)` reports the tissue's integrity, its 0 to 100
`normalized_health()` and its remaining capacity. A refused call changes
nothing: an unknown tissue, an invalid load, negative or non-finite work, or a
repair fraction outside 0 to 1.

## Failure and the host's boundary

The charge that exhausts a tissue records its failure. Later loads find
nothing left to charge and record nothing new, so simultaneous lethal sources
report a tissue once. `take_failures` returns the failures since its last call
in identity order. For plants, the documented host step is
`Plants::damage(id, 100)`, which kills the tissue through the plants' own
path: its stores are lost, and its cell stays until the host removes it. A host
can remove the tissue as debris instead, or keep it.

A host's boundary then runs in this order:

1. Charge the step's loads.
2. Take the failures and retire the failed tissue, then make any other edits.
3. Call `sync`, then tick the plants, then call `sync` again.

`sync` follows the plant world after any call that changes it, including each
tick. A failed tissue cannot grow: if one has, `sync` refuses with
`FailedGrowth` and changes nothing. The host removes it, or repairs it
explicitly, first. Settling failures before the tick admits growth keeps new
material from rescuing a tissue that has already failed.

`repair` is the only way capacity returns. It is explicit and host-authorized,
changes integrity only and withdraws a failure that has not been taken once
capacity returns. Nothing repairs or heals automatically.

## Size changes

- New tissue starts intact.
- Growth adds intact material: the capacity rises by the new size's share, and
  the damage already done stays.
- Shrinking keeps the integrity. The material shed takes its share of the
  damage, as a [region split](material-region-split.md) does.
- Removal drops the record, with any failure not yet taken. Tissue identities
  are never reused, so a record cannot pass to another tissue.
- Severing a connection between tissues changes no record.

## Persistence

`save` returns a `TissueSave`: the table's identity, each tissue's identity,
size and precise remaining capacity, and the failures not yet taken. It
serializes with serde. `restore` takes it with the plant world saved at the
same boundary and refuses it, with `Mismatch`, when:

- the table's identity differs;
- a tissue lacks its record, a record names no tissue, or the records are out
  of identity order;
- a record's size is not its tissue's current size;
- a remaining capacity is negative, not finite or above the tissue's capacity;
- the failures are out of order, repeated or name a tissue that has not failed.

`TissueIntegrity` also clones, and a restored or cloned adapter continues
exactly as the original does.

## Qualification

`crates/engine-material/tests/tissue.rs` runs a real plant world: a root whose
table material is wood at the reference branch's volume, and the soft shoot it
grows at the reference spaceling's volume. The reference bodies and loads are
the [#326 cases'](../data/material-model-v2.md).

- **Safe knocks:** a thousand stone hits on each tissue at 0.99 of the pair's
  safe-limit speed charge nothing.
- **Accumulated hits:** at 2 wu/s the stone fails the shoot on its sixth hit,
  as it fails the reference spaceling region, and each hit leaves the two
  regions identical. The failure is reported on the sixth hit only. A
  spaceling striking the root is charged as side `a`.
- **Tissue against wood:** one 10 wu/s stone fails the shoot outright and
  leaves the root 0.703 of its integrity, as in the #326 table.
- **Simultaneous failures:** the shoot and then the root fail at one boundary
  and come back once, in identity order. Later hits, presses and charges
  charge nothing and report nothing. Retiring both through
  `Plants::damage(id, 100)`, even with a repeated call, loses their stores
  once, and their cells stay.
- **Health:** five shaded ticks cost the damaged shoot 40 health, and four
  sunlit ticks restore it, with its integrity unchanged to the bit. Shaded to
  death in eight ticks, it keeps that integrity. Repairing it, or restoring
  the save made while it lived, leaves it dead, and it cannot develop.
- **Dead wood:** a root starved to death in eight ticks carries the reference
  tank's rest for a thousand ticks at no cost. A 1.05x overload then fails it
  on the twelfth tick, as it fails the reference plank.
- **Growth:** the shoot develops to size 2 and keeps its damage exactly. A
  failed shoot left alive that grows is refused until it is repaired; it then
  holds half its old capacity plus the new material intact.
- **Shrinking and removal:** shrinking keeps the integrity, removal drops the
  record and its untaken failure, and a new runner starts intact.
- **Saves:** a save taken with one untaken failure and a nearly failed shoot
  round-trips through JSON, restores equal to the original and continues to
  the same failures. A repair before the failure is taken withdraws it. Nine
  forged saves, a different table and a larger plant world are refused.
- **Refusals:** refused calls leave the adapter and the striker unchanged, and
  tables that fail validation are refused.

## Limits

- **Measured by the host:** the table holds only material and volume. The host
  measures each contact's width, sections, speed and force for its tissue.
- **One region per tissue:** a tissue's whole cell takes the load, with no
  partial cut within a cell.
- **Existing hosts unchanged:** only Material Lab uses the adapter, for its
  [shell through moss](material-lab.md#shell-through-moss), and
  `Plants::damage` keeps its current callers.
- **The law's limits:** shear, fatigue and rate effects are not modelled; see
  [material damage](material-damage.md#limits-and-verification).
