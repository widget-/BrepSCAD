# Vendored dependencies

This branch (`wip/monstertruck`) evaluates the `monstertruck` fork in place of
the upstream Truck stack. `monstertruck-solid` and `monstertruck-topology` are
vendored because the published 0.4.1 boolean backend is "classic 0.3.2 verbatim"
and lacks fixes the BrepSCAD adapter relies on. The remaining `monstertruck-*`
crates come from crates.io 0.4.1.

## monstertruck-solid 0.4.1

Source: <https://github.com/virtualritz/monstertruck>, published crate `0.4.1`
(commit `e6520c2`). Licensed under Apache-2.0; see
[`LICENSE`](monstertruck-solid/LICENSE).

Local patches, all in `src/transversal/classic/`:

- `divide_face.rs` keeps a divided face when `Face::try_new` rejects its wires
  (boolean-generated wires are often non-simple) by falling back to
  `Face::new_unchecked`, instead of failing the whole boolean.
- `loops_store.rs` snaps a projected vertex to the nearer end of its curve when
  the intersection parameter lands on, or a rounding error outside, the curve's
  range. `Edge::cut_with_parameter` rejects such a parameter, which used to fail
  the boolean when an intersection endpoint coincided with an existing vertex.
- `faces_classification.rs` skips a component whose faces share every edge, so
  `extract_boundaries()` returning no wires no longer indexes `boundary[0][0]`
  out of bounds.
- `mod.rs` ports upstream Truck 0.4.0's `InsideJudge`: three-ray majority with
  face orientation applied, and a threshold of 0 for an inverted shell. The
  0.3.2 single-ray test misclassifies every face when `difference` runs as
  `and(a, not(b))`, leaving the result open.
- `mod.rs` returns `Solid::new_unchecked` from `finalize`, matching upstream
  0.4.0. The adapter accepts open boolean-derived shells and tessellates the
  surviving faces; `Solid::try_new` here rejected them and lost the result.

## monstertruck-topology 0.4.1

Source: same crate release. Licensed under Apache-2.0; see
[`LICENSE`](monstertruck-topology/LICENSE).

- `src/shell.rs` and `src/solid/mod.rs` use `Face::new_unchecked` /
  `Solid::new_unchecked` in `try_mapped`, so mapping a boolean result does not
  reject non-simple wires or open shells. This mirrors the
  `PATCH(BrepSCAD)` topology patches on the Truck stack.

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
