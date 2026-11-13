# Splitting and moving material regions (#328)

`PhysicsWorld::split_loading_region` divides a registered material region
between itself and a new region, and `PhysicsWorld::transfer_loading_region`
moves one to another body. Volume, capacity and accumulated damage go with the
material: neither operation repairs a region, and a split creates no capacity.
Hosts use them for cuts, fragments, ownership changes and rebuilds, so a
damaged region never returns at full health because its geometry or owner
changed.

## Identity and bindings

A split keeps the region's token for the part that stays and issues a new token
for the piece. `RegionPiece` describes the piece: its `RegionId`, its body, its
share of the volume, its section and the colliders that move with it. The piece
may live on the region's own body or on another body, such as a fragment the
host has just created. Listed colliders must be bound to the region and belong
to the piece's body; they keep their patch IDs, and the others stay with the
region. The host gives both sections, because the cut's geometry is its own:
`kept` for the region and `RegionPiece::section` for the piece.

A transfer moves a region's stored material, balance and section unchanged to
a new `RegionId` on another body, or on another part of the same body. The old
token goes stale and its colliders are unbound. A rebuild creates the
replacement body, transfers, binds the replacement's colliders and then removes
the old body, all between steps. Removing the old body first would retire its
regions and discard their balances. Replacing colliders on the same body keeps
its regions, as before, and needs no transfer.

## Balance

`Region::split(fraction)` gives the piece `fraction` of the volume, both parts
at the region's integrity. Rounded part volumes can hold up to one f32 ulp more
capacity than the whole, so the larger part now takes the remainder of the f64
balance rather than its own share, and the parts never hold more than the whole.
Before this change, 7,862 of 20,000 randomized splits over the six reference
materials gained capacity, up to 1.3e-7 of it. Now none do: 19,199 conserve the
balance exactly, and the other 801 lose at most one f32 ulp of capacity to
rounding. Each part stays within two f32 epsilons of the original integrity.

## Refusals and runs

Both operations check every input before changing anything, and a refused
request leaves saves unchanged byte for byte. A contact episode involving the
region refuses either operation with `LoadingError::RegionInContact`, because
the changed region could otherwise meet its contact again as a new impact with
a fresh budget. The host steps until the episode ends, so a body resting on a
support cannot be split until it separates. Other refusals cover a stale token,
a region without registered material, an unknown body or a `RegionId` whose
entity is not the body's, an existing `RegionId`, a full region registry for a
split (a transfer adds no region), fractions outside `(0, 1)`, invalid sections,
and colliders that are unknown, repeated, bound elsewhere or on another body.

Like creating a region or registering a material, a completed split or transfer
is an external edit. It ends any active compression or gravity run and clears
arrival proofs. A run pressing the region always has a contact episode, so an
edit never ends one mid-compression.

## Persistence

Neither operation adds saved state. Regions, bindings, stored materials and the
incarnation counter already belong to the capture payload and the response
tail, so worlds saved after an edit restore and replay byte for byte.

## Qualification

`world::loading::response::split::tests` strike a fixed wood ball, volume 20
and capacity 1,200, with an armor ball at 5 wu/s. The strike charges 249.47 and
leaves the wood 950.53.

- A 0.25 split leaves the wood volume 15 with 712.90 of 900 and gives the piece
  volume 5 with 237.63 of 300, together exactly the 950.53. The named colliders
  move to the piece and the others stay. Wood made of one, two or four equal
  colliders splits to identical balances, and saves restore and replay 30 steps
  byte for byte.
- A half moved to a new fragment body takes half the volume and balance.
  Removing the fragment retires only its region.
- A rebuild transfers the region to a replacement body and removes the old one.
  A second strike then leaves 701.06, exactly what it leaves in an unchanged
  copy of the world.
- Each refusal listed above leaves the save unchanged, both during contact and
  after separation.

`gravity::tests::region_splits_and_transfers_end_the_run_like_other_edits`
checks that a split and a transfer each end a gravity run that has not yet
touched, as other edits do. `engine-material` repeats the randomized splits
described under Balance.

## Limits

Splitting or moving a region in contact, connection loads and automatic
fracture on failure remain future work; hosts keep debris admission. The engine
records no lineage between a region and its pieces. A Material Lab case will
follow.
