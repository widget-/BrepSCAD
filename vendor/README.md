# Vendored dependencies

## truck-shapeops 0.4.0

Source: <https://github.com/ricosjp/truck>, published crate `0.4.0`.
Licensed under Apache-2.0; see [`LICENSE`](truck-shapeops/LICENSE).

Local patch: `src/transversal/divide_face/mod.rs` rebuilds divided faces with
`Face::try_new(...).ok()?` instead of `Face::debug_new(...)`. A non-simple
generated wire now returns `None`, letting `brepscad-kernel-truck` retry the
boolean instead of panicking.

Remove this directory and the `[patch.crates-io]` entry in the workspace
manifest once a released Truck version contains equivalent behavior. Upstream
master at `1ca3214` still used `Face::debug_new` when checked on 2026-10-30.
