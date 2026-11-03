# Vendored dependencies

This branch (`wip/monstertruck`) evaluates the `monstertruck` fork in place of
the upstream Truck stack. The whole kernel stack is vendored from PR #31
(`pr/boolean-robustness`, commit `79ef869bc2`, based on master `d2ad1ed7`)
because the published 0.4.1 boolean backend is "classic 0.3.2 verbatim": it
amputates coplanar unions and rejects boolean-derived shells the adapter needs.
PR #31 supplies the coplanar pipeline, 5-ray classification, normalized boolean
entry points, parallel face division, and the meshing collision-culling fix.
The patches below are the remaining BrepSCAD fallbacks on top of it.

## monstertruck (PR #31)

Source: <https://github.com/virtualritz/monstertruck>, PR #31 head
`79ef869bc2`. Licensed under Apache-2.0; see
[`LICENSE`](monstertruck/LICENSE).

`vendor/monstertruck/` is a nested workspace holding the nine crates this
project builds: `core`, `derive`, `geometry`, `mesh`, `meshing`, `modeling`,
`solid`, `topology`, and `traits`. The crates' `workspace = true` dependencies
resolve inside that nested workspace. Test-only dependencies are stripped (we
never build the vendored workspace's tests), and `monstertruck-modeling` keeps a
defined-but-empty `fillet` feature because the fillet crate is not vendored and
the source gates code on it.

Local patches:

- `monstertruck-solid/src/transversal/classic/divide_face.rs` keeps a divided
  face when `Face::try_new` rejects its wires (boolean-generated wires are often
  non-simple) by falling back to `Face::new_unchecked`, instead of failing the
  whole boolean.
- `monstertruck-solid/src/transversal/classic/loops_store.rs` snaps a projected
  vertex to the nearer end of its curve when the intersection parameter lands
  on, or a rounding error outside, the curve's range.
  `Edge::cut_with_parameter` rejects such a parameter, which used to fail the
  boolean when an intersection endpoint coincided with an existing vertex.
- `monstertruck-solid/src/transversal/classic/mod.rs` returns
  `Solid::new_unchecked` from `finalize`, so open boolean-derived shells reach
  the adapter's retry ladder and the tessellator. It also applies the
  inverted-shell crossing shift even when the shell pair is not coplanar;
  without that, a difference's subject faces are classified as outside and the
  engine returns the tool instead of the cut.
- `monstertruck-solid/src/transversal/classic/coplanar/sew.rs` splits boundary
  edges at boundary vertices that lie on them, then re-merges the resulting
  edges. This closes split-edge topology left by independently divided faces.
- `monstertruck-solid/src/transversal/classic/coplanar/arrangement.rs` probes
  several small offsets when assigning a hole to an outer loop. The previous
  fixed `tol * 4` offset could miss the owning loop of a hole in a thin
  annulus and abort the whole coplanar imprint.
- `monstertruck-solid/src/transversal/classic/coplanar/mod.rs` and
  `.../loops_store.rs` add an exact planar plane-plane intersection fallback
  when the tessellated triangle interference finds no segment. The line is
  clipped against both trimmed face outlines in 2D and fed back as a normal
  `IntersectionCurve`, so a cutter crossing a face boundary or convex edge
  still splits the adjacent faces.
- `monstertruck-solid/src/transversal/classic/mod.rs` also runs that sew for
  non-coplanar pairs when both operands are entirely planar. Mixed
  planar/curved pairs stay untouched; sewing them regresses the cube/cylinder
  corner case.
- `monstertruck-topology/src/shell.rs` and `src/solid/mod.rs` use
  `Face::new_unchecked` / `Solid::new_unchecked` in `try_mapped`, so mapping a
  boolean result does not reject non-simple wires or open shells.
- `monstertruck-geometry` carries PR #32 (`1b6f1de61e`), which cuts B-splines
  onto a knot just above the cut parameter instead of inserting a
  multiplicity-`degree + 2` knot beside it.

The adapter uses `monstertruck_solid::difference` directly and, after the exact
tolerance ladder fails, falls back to `and_normalized` / `or_normalized` /
`difference_normalized`. Those recover cases the unscaled ladder misses, such as
a cylinder a hair taller than the cube it pierces.

Difference results are validated before they are accepted. A result must keep
the slabs of the subject that the tool cannot reach and enclose positive
volume; this rejects the amputated and inverted shells the classic backend
returns for nested or coplanar cutters. Rejected results are clipped to the
subject's bounds, then retried at the other tolerances and through the
normalized entry point. If the engine still cannot produce a valid shell, the
adapter falls back to an axis-aligned slab clip, keeps the subject for a
sub-millimetre sliver, or keeps the subject rather than rendering an inside-out
cavity. Set `BREPSCAD_DEBUG_BOOLEAN=1` to log each attempt and rejection.

## Truck stack (retained, unused on this branch)

The `truck-*` directories and the patches documented below are kept for
comparison and for switching back to `main`; the workspace manifest no longer
patches them on this branch.

### truck-shapeops 0.4.0

Source: <https://github.com/ricosjp/truck>, published crate `0.4.0`.
Licensed under Apache-2.0; see [`LICENSE`](truck-shapeops/LICENSE).

Local patches:

- `src/transversal/divide_face/mod.rs` rebuilds divided faces with
  `Face::try_new(...).ok()?` instead of `Face::debug_new(...)`. A non-simple
  generated wire now returns `None`, letting `brepscad-kernel-truck` retry the
  boolean instead of panicking. `src/alternative.rs` also allows the unused
  `Alternative` helper under the crate's release-mode `deny(warnings)`.
- `src/transversal/loops_store/mod.rs` snaps a projected vertex to the nearer
  end of its curve when the intersection parameter lands on, or a rounding
  error outside, the curve's range. `Edge::cut_with_parameter` rejects such a
  parameter, which used to fail the whole boolean when an intersection endpoint
  coincided with an existing vertex.
- `src/transversal/faces_classification/mod.rs` skips a component whose faces
  share every edge, so `extract_boundaries()` returning no wires no longer
  indexes `boundary[0][0]` out of bounds.

### truck-geometry 0.5.0 (master)

Source: <https://github.com/ricosjp/truck>, vendored from master `1ca3214`.
Licensed under Apache-2.0; see [`LICENSE`](truck-geometry/LICENSE).

- `src/decorators/intersection_curve.rs` accepts a larger angular tolerance
  (1.0e-3 instead of `TOLERANCE`) in the coplanar/tangent fallback of
  `double_projection`. Coincident faces of two solids are computed
  independently, so their normals are only parallel to within accumulated
  rounding; the previous tolerance rejected them and failed the boolean.
- `src/decorators/revolved_curve.rs` keeps the seam at angle 0 rather than 2π
  when `seam_side` has no continuity hint. `proj_angle` decides that side from a
  cross-product sign that is rounding noise on the seam; trimming a surface with
  the other name folds the parameter polygon over itself.

### truck-modeling 0.6.0 (master)

Source: <https://github.com/ricosjp/truck>, vendored from master `1ca3214`.
Licensed under Apache-2.0; see [`LICENSE`](truck-modeling/LICENSE).

- `src/geometry.rs` delegates `Surface::RevolutedCurve`'s nearest-parameter
  search to the surface's analytic meridian search. It previously sampled a
  101×101 grid only to seed the Newton solve, which made every intersection
  evaluation against a sphere cost ten thousand surface evaluations.

### Other vendored Truck crates

`truck-base`, `truck-derivers`, `truck-geotrait`, `truck-topology`,
`truck-polymesh`, and `truck-meshalgo` are vendored from the same master
revision. They carry the BrepSCAD patches needed to keep boolean-derived shells
(which master shapeops may leave open or non-simple) renderable; see the
`PATCH(BrepSCAD)` comments in those sources.

Remove the directories and the `[patch.crates-io]` entries in the workspace
manifest once released versions contain equivalent behavior. Upstream master at
`1ca3214` still used `Face::debug_new` when checked on 2026-10-30.
