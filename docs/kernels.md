# Kernel capability matrix

Last updated: 2026-10-29
Status: working document. Cells marked **?** or **~** must be resolved by the conformance suite, not assumed from documentation.

## Purpose

The UI must not grey out an operation because a doc table says a kernel lacks it, nor offer an operation that silently fails. The reliable source of truth is a per-backend conformance run that probes each operation and emits a capability manifest. This document is the starting map; `kernel-conformance` is the authority.

## Candidate backends

| Backend                            | Language | Licence                 | Representation          | Current status                                                                                      |
| ---------------------------------- | -------- | ----------------------- | ----------------------- | --------------------------------------------------------------------------------------------------- |
| **OCCT** (Open CASCADE Technology) | C++      | LGPL-2.1 with exception | B-rep, analytic + NURBS | Mature; full CAD feature set; Rust bindings (`opencascade-rs`, `occt-sys`, `occt-wasm`)             |
| **SolidCore**                      | Rust     | Apache-2.0              | B-rep, analytic + NURBS | Unit port complete (278/278 units, 402,122/402,122 lines); API 0.0.x; integration still in progress |
| **Truck** (upstream)               | Rust     | Apache-2.0              | B-rep, NURBS            | Active; ~1.7k stars; booleans, STEP, assembly, meshing; no native fillet                            |
| **Truck** (`monstertruck` fork)    | Rust     | Verify                  | B-rep, NURBS            | Adds fillet, healing, STEP, assembly; verify provenance and maintenance                             |
| **Manifold**                       | C++      | Apache-2.0              | Mesh CSG                | **Not a candidate backend.** Optional internal mesh fallback for `hull`/`minkowski`/mesh imports    |

Legend: **✓** supported; **~** partial / experimental / verify; **✗** not supported; **?** unknown — must test.

## Geometry operations

| Operation                                 | OCCT | SolidCore | Truck (upstream)     | Truck (fork) | Notes                                                                                |
| ----------------------------------------- | ---- | --------- | -------------------- | ------------ | ------------------------------------------------------------------------------------ |
| Box / cuboid                              | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| Sphere                                    | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| Cylinder                                  | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| Cone                                      | ✓    | ✓         | ?                    | ?            | Verify primitive coverage                                                            |
| Torus                                     | ✓    | ✓         | ?                    | ?            | Verify primitive coverage                                                            |
| `polyhedron` / explicit vertex-face solid | ✓    | ?         | ?                    | ?            | Build from topology; verify API                                                      |
| 2D primitives / profiles                  | ✓    | ✗         | ~ (`truck-drafting`) | ~            | SolidCore is 3D solid B-rep only; SolidCraft sketches/profiles are an app layer, not a kernel API |
| Union                                     | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| Difference                                | ✓    | ✓         | ✓ (`and` + `not`)    | ✓            |                                                                                      |
| Intersection                              | ✓    | ✓         | ?                    | ?            | Verify                                                                               |
| Booleans on touching/coincident faces     | ~    | ~         | ~                    | ~            | All B-rep kernels are sensitive; healing/retry needed                                |
| Non-manifold / self-intersecting input    | ~    | ~         | ~                    | ~            | Strict; may fail or need repair                                                      |
| Translate / rotate / mirror               | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| Uniform scale                             | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| Non-uniform scale                         | ✓    | ✓         | ~                    | ~            | May approximate NURBS/analytic surfaces                                              |
| `multmatrix`                              | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| `linear_extrude`                          | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| `rotate_extrude`                          | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| Sweep / loft / pipe / skin                | ✓    | ✓         | ✓                    | ✓            | Verify exact API and failure modes                                                   |
| Fillet                                    | ✓    | ✓         | ✗ / ~                | ✓            | Upstream Truck has only experimental fillet work; the fork adds rolling-ball fillets |
| Chamfer                                   | ✓    | ✓         | ?                    | ?            | Verify                                                                               |
| Shell                                     | ✓    | ✓         | ?                    | ?            | Verify; scope says offset/shell                                                      |
| Offset                                    | ✓    | ✓         | ~                    | ✓            | Fork has offset curves/surfaces                                                      |
| Draft / thicken                           | ✓    | ?         | ?                    | ?            | Verify                                                                               |
| NURBS curves/surfaces                     | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| NURBS intersection / trimming             | ✓    | ✓         | ~                    | ~            | Verify                                                                               |
| `hull`                                    | ✗    | ✗         | ✗                    | ✗            | Mesh-only; needs internal fallback                                                   |
| `minkowski`                               | ✗    | ✗         | ✗                    | ✗            | Mesh-only; needs internal fallback                                                   |
| Face / edge / vertex topology             | ✓    | ✓         | ✓                    | ✓            |                                                                                      |
| Stable face/edge IDs across rebuild       | ~    | ?         | ~                    | ~            | Requires a naming/persistent-ID layer; verify                                        |
| Mass properties                           | ✓    | ✓         | ?                    | ?            | Verify                                                                               |
| Distance / interference                   | ✓    | ✓         | ?                    | ?            | Verify                                                                               |
| Ray casting / picking                     | ✓    | ✓         | ?                    | ?            | Verify                                                                               |
| Tessellation                              | ✓    | ✓         | ✓                    | ✓            | OCCT `BRepMesh`, SolidCore `query.tess`, Truck `truck-meshalgo`                      |

## I/O, platform, and operational

| Feature                       | OCCT                  | SolidCore                    | Truck (upstream)     | Truck (fork) | Notes                                                               |
| ----------------------------- | --------------------- | ---------------------------- | -------------------- | ------------ | ------------------------------------------------------------------- |
| STEP import                   | ✓                     | ? (not in scope)             | ✓ (`truck-stepio`)   | ✓            | SolidCore scope excludes OCCT translators; verify any native import |
| STEP export                   | ✓                     | ? (not in scope)             | ✓                    | ✓            |                                                                     |
| IGES                          | ✓                     | ? (not in scope)             | ?                    | ?            | Verify                                                              |
| STL / OBJ / 3MF / glTF        | ✓ / ~                 | ~                            | ~                    | ~            | Tessellation exists; file I/O is partly app-level; verify           |
| DXF / SVG                     | ✓ / ~                 | ?                            | ~                    | ~            | 2D semantics differ from OpenSCAD                                   |
| Assembly / instances          | ✓ (XCAF)              | ?                            | ✓ (`truck-assembly`) | ✓            | Verify mates/constraints; assembly is not OpenSCAD’s model          |
| wasm                          | ~ (`occt-wasm`)       | ✓ (`wasm32-unknown-unknown`) | ~ (`truck-js`)       | ~            | Verify actual browser build and memory                              |
| Crash/panic risk              | C++ exceptions/aborts | Rust panic (needs guard)     | Rust panic           | Rust panic   | Consider a sidecar process for OCCT                                 |
| API stability                 | Mature but large      | 0.0.x, experimental          | Young                | Young        | Pin versions; adapters isolate churn                                |
| Relative feature completeness | Highest               | High, integration gaps       | Medium; no fillet    | High, forked | Test against your corpus                                            |

## Per-backend notes

### OCCT

- Most complete: fillets, chamfers, shells, offsets, drafts, NURBS, STEP/IGES, XCAF assemblies, topology, mass properties, tessellation.
- LGPL-2.1 with exception; dynamic linking is the usual compliance path. A sidecar process also makes replacing the kernel easier.
- C++ kernel; Rust bindings are usable but the high-level wrapper is still evolving. `occt-wasm` (OCCT V8 via wasmtime) is convenient for prototyping and browser-adjacent use, but it is not a native `wasm32-unknown-unknown` build.
- Best default for production and as a conformance oracle for the Rust kernels.

### SolidCore

- Rust port of NVIDIA SMLib (`usd-brep` v1.0.1), Apache-2.0, safe Rust, wasm-buildable.
- Unit port is complete: **278/278 units, 402,122/402,122 upstream lines, all green** (2026-10-30), with difftests against the C++ for topology identity and `1e-9` relative values.
- Primitives are exercised end to end: **9/9 build, 9/9 pass**.
- Scope is 3D solid B-rep: NURBS, curves/surfaces, topology, intersections, solid booleans, fillets/chamfers, offset/shell, sweeps/skins, primitives, healing/stitching, mass properties and queries.
- **No 2D primitives or 2D boolean API.** The boolean units are solid booleans. SolidCraft's sketch/profile regions are implemented above the kernel, not exposed by SolidCore.
- Out of scope: upstream’s OCCT translators, USD/Hydra integration, Python bindings. Do not assume STEP/IGES; verify any file I/O.
- Integration is still in progress; the README keeps the API at 0.0.x until usable end to end. The external SSI corpus still has incomplete/wrong-topology cases.
- Best pure-Rust candidate for fillets/booleans/NURBS, but budget for API churn and integration testing.

### Truck

- Upstream `ricosjp/truck`: Rust, Apache-2.0, B-rep + NURBS, `truck-shapeops` booleans, `truck-stepio` STEP, `truck-assembly`, `truck-meshalgo` tessellation, `truck-rendimpl` wgpu.
- **No native fillet** in upstream; the changelog describes fillet-surface prototyping, not a production operation. SolidCraft reportedly implements fillets and boolean retries above Truck rather than relying on it.
- The `monstertruck` fork (crate, 0.4.1, Sep 2026) adds rolling-ball fillets, healing, STEP, and assembly. Verify its provenance, licence, and maintenance before depending on it.
- Best if you want a pure-Rust B-rep kernel with STEP and are willing to own fillets/blends or use the fork.

### Manifold

- Fast mesh CSG, Apache/MIT, wasm-capable; great for `hull`, `minkowski`, mesh imports, and mesh cleanup.
- No B-rep topology, fillets, STEP, or face/edge semantics.
- Keep it as an **internal mesh fallback**, not a user-selectable backend, unless the product explicitly offers a “mesh mode.”

## Capability probing

Build a `kernel-conformance` crate with one test group per capability. The test run generates a manifest like:

```json
{
  "backend": "solidcore",
  "version": "0.0.x",
  "capabilities": {
    "primitive.box": "pass",
    "boolean.difference": "pass",
    "blend.fillet.all_edges": "pass",
    "blend.fillet.selected_edges": "fail",
    "io.step.export": "unsupported",
    "topology.stable_ids": "unknown"
  }
}
```

The UI loads this manifest; it does not hardcode capability flags. Tests should cover:

- **Primitives:** minimal, typical, extreme proportions; low/high tessellation.
- **Booleans:** disjoint, touching, coincident faces, nested, coplanar, tangent.
- **Fillet/chamfer:** single edge, all edges, tangent edges, radius near/exceeding limits, selected-edge chains.
- **Sweeps:** straight, curved, closed, self-intersecting path, twisting profile.
- **NURBS:** trimmed surface, surface-surface intersection, degenerate knot vectors.
- **Topology:** face/edge counts, stable IDs across parameter changes, picking.
- **Queries:** volume/area/centroid, distance, interference, ray cast.
- **I/O:** STEP round-trip, mesh export/import, units.
- **Failure behavior:** invalid input, unsupported operation, panic/abort containment.

Record success/failure, error, wall time, topology counts, volume/area deltas, and mesh diff. Run per backend in CI.

## Packaging

- **Separate crates + features** is the right default: `kernel-api`, `kernel-occt`, `kernel-solidcore`, `kernel-truck`, each behind a feature flag. One app binary; choose a default backend in settings.
- **Separate binaries/processes** are worth it for OCCT crash isolation and LGPL replacement, via a small `kernel-host` sidecar and a stable IPC protocol. Not needed initially for the safe-Rust kernels.
- **Dynamic plugins** (`cdylib`) are not recommended: Rust has no stable ABI. If runtime loading is required, expose a C ABI or use IPC.
- **Separate builds per backend** fragment testing and support; prefer one binary with features plus a default. Ship experimental backends only in opt-in builds.

## Open questions

1. Is SolidCore + Truck enough, or is OCCT needed as a production fallback/oracle?
2. If Manifold is not a backend, how are `hull`, `minkowski`, and mesh imports handled? Internal mesh fallback, custom implementation, or “unsupported”?
3. Do we need STEP/IGES enough to make OCCT or Truck mandatory? SolidCore does not cover them in scope.
4. Do we need stable face/edge IDs for constraints/attachments, or only picking? This changes the adapter design.
5. Which Truck: upstream `truck` or the `monstertruck` fork? Fillets depend on the answer.

## Sources

Accessed 2026-10-28.

- OCCT — <https://dev.opencascade.org/>
- OCCT workbench / feature docs — <https://dev.opencascade.org/doc/overview/html/>
- `opencascade-rs` — <https://github.com/bschwind/opencascade-rs>
- `occt-sys` — <https://lib.rs/crates/occt-sys>
- `occt-wasm` — <https://crates.io/crates/occt-wasm>
- SolidCore — <https://github.com/tangency-labs/solidcore>
- SolidCore progress — <https://tangency-labs.github.io/solidcore/>
- SolidCore units — <https://tangency-labs.github.io/solidcore/units/index.html>
- SolidCore solids — <https://tangency-labs.github.io/solidcore/solids.html>
- Truck — <https://github.com/ricosjp/truck>
- Truck book — <https://truckkernel.com/>
- `monstertruck` crate — <https://crates.io/crates/monstertruck>
- Manifold — <https://github.com/elalish/manifold>
- `manifold-csg` — <https://crates.io/crates/manifold-csg>
