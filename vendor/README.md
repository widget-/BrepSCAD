# Vendored dependencies

## truck-shapeops 0.4.0

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

## truck-geometry 0.5.0 (master)

Source: <https://github.com/ricosjp/truck>, vendored from master `1ca3214`.
Licensed under Apache-2.0; see [`LICENSE`](truck-geometry/LICENSE).

- `src/decorators/intersection_curve.rs` accepts a larger angular tolerance
  (1.0e-3 instead of `TOLERANCE`) in the coplanar/tangent fallback of
  `double_projection`. Coincident faces of two solids are computed
  independently, so their normals are only parallel to within accumulated
  rounding; the previous tolerance rejected them and failed the boolean.

## Other vendored Truck crates

`truck-base`, `truck-derivers`, `truck-geotrait`, `truck-topology`,
`truck-polymesh`, `truck-meshalgo`, and `truck-modeling` are vendored from the
same master revision. They carry the BrepSCAD patches needed to keep
boolean-derived shells (which master shapeops may leave open or non-simple)
renderable; see the `PATCH(BrepSCAD)` comments in those sources.

Remove the directories and the `[patch.crates-io]` entries in the workspace
manifest once released Truck versions contain equivalent behavior. Upstream
master at `1ca3214` still used `Face::debug_new` when checked on 2026-10-30.
