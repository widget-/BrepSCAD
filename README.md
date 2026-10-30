# BrepSCAD

An (extremely) experimental alternative to OpenSCAD, using B-rep geometry rather than OpenSCAD’s CGAL/Manifold CSG backends.

BrepSCAD is an independent project and is not affiliated with OpenSCAD.

## Goals

- Keep `.scad` text as the canonical document.
- Allow direct manipulation in the UI, saved back to the source.
- Use B-rep geometry as the canonical representation, rather than mesh tris.
- Support multiple kernel backends.
- Implement faster and/or more accurate versions of some common OpenSCAD geometry libraries.
- Render all OpenSCAD source files "close enough" to the original.

## Workspace

```text
crates/
  cli/          CLI entry point (package `brepscad`)
  kernel-api/   Kernel trait, ShapeId, capabilities, neutral mesh
  kernel-truck/ Truck adapter: cuboid, cylinder, sphere, polyhedron, sketch, extrude, revolve
  scad-eval/    evaluator, parameter discovery, 2D profile CSG
  scad-syntax/  lossless CST, lexer, parser, and text edits
  viewer/       egui + wgpu viewer with source editor and orbit camera
```

The planned backend crates (`kernel-occt`, `kernel-solidcore`) are not created yet; the viewer currently combines UI and rendering in one crate.

## Development

A `shell.nix` provides the Rust toolchain (cargo, rustc, rustfmt, clippy, rust-analyzer) and native build essentials.

### Nix / NixOS

```sh
# with direnv:
direnv allow
cargo run -p brepscad
cargo test --workspace

# without direnv:
nix-shell --run 'cargo test --workspace'
nix-shell --run 'cargo run -p brepscad'
```

### Everything else

With a Rust toolchain installed:

```sh
cargo run -p brepscad
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## CLI Usage

The CLI is mainly for evaluation and debugging. It prints the number
of top-level meshes and the triangle/vertex counts for each mesh:

```sh
# Evaluate a source file.
cargo run -p brepscad -- path/to/model.scad

# Use OpenSCAD's origin-at-corner/base-at-zero placement.
cargo run -p brepscad -- --legacy-centering path/to/model.scad
```

By default, BrepSCAD centers cubes and squares, and centers cylinders and
linear extrusions on Z. The legacy flag places cubes/squares at the origin and
cylinder/extrusion bases at `z = 0`.

```text
meshes: 1
mesh 0: 464 triangles, 232 vertices
```

## Language support matrix

Status: ✔ supported, ◐ partial, ✖ not implemented.

| Operation | Status | Notes |
| ----------- | -------- | ------- |
| Top-level assignments | ✔ | |
| Module calls | ✔ | |
| Expressions | ✔ | No strings. |
| Comments and whitespace | ✔ | |
| `module` / `function` declarations | ✖ | |
| `for` / `if` | ✔ | |
| `include` / `use` | ✖ | |
| `$fn`, `$fa`, `$fs` | ◐ | `$fn` ≤32 facets; ≥33 or unset uses analytic geometry. `$fa`/`$fs` only sample profiles. |
| `cube`, `cylinder`, `sphere` | ✔ | `sphere` is faceted; `$fn` selects coarser counts, default 32×16. |
| `polyhedron` | ✔ | Closed, consistently wound shell with planar faces. |
| `square`, `circle`, `polygon` | ✔ | `square`/`circle` centered in XY; `polygon` points used as given. |
| `translate`, `rotate`, `scale`, `multmatrix`, `mirror`, `resize` | ◐ | `rotate` vector form only; `resize` `newsize` only, no `auto`. |
| `linear_extrude` | ◐ | `height` only; centered on Z; no `center`, `twist`, `scale`, or `slices`. |
| `rotate_extrude` | ◐ | Profile must lie at `x >= 0`. |
| `difference`, `union`, `intersection` | ◐ | 2D CSG is polygonal; 3D booleans can fail on near-coincident or tangent geometry. |
| `offset` | ✔ | No `chamfer`; `$fn` only affects profile sampling. |
| `color` | ✖ | |
| `hull`, `minkowski` | ✖ | |
| `import`, `export`, `surface` | ✖ | No file I/O yet. |
| `projection` | ✖ | |

By default, `cube`, `square`, `cylinder`, and `linear_extrude` use BrepSCAD's
centered placement. The legacy option switches to OpenSCAD's origin-at-corner
and base-at-zero placement.

## Feature matrix

Status: ✔ supported, ◐ partial, ✖ not implemented.

| Feature | Status | Notes |
| --------- | -------- | ------- |
| Source editor | ✔ | In-memory basic textbox. |
| Source tree | ✔ | Groups and selectable leaf primitives. |
| UI parameter editing | ◐ | Numeric parameters only. |
| Legacy OpenSCAD centering | ✔ | CLI `--legacy-centering` or the viewer checkbox. |
| Preview-based selection | ✔ | |
| 2D profile CSG | ✔ | Via `clipper2-rust`. |
| Alternate kernels | ✖ | Truck only; OCCT and SolidCore are candidates. |
| File open/save | ✖ | |
| Export | ✖ | |
| Undo/redo | ✖ | |
| Persistence | ✖ | Source text is in memory only; no project/session persistence. |

## GUI

```sh
cargo run -p brepscad-viewer
```

## Documentation

- [`docs/status.md`](docs/status.md) — current capabilities, limitations, and next steps.
- [`docs/development.md`](docs/development.md) — development helpers
- [`docs/research.md`](docs/research.md) — prior art, source-first architecture, kernel trade-offs, native library fast paths.
- [`docs/kernels.md`](docs/kernels.md) — per-operation capability matrix for OCCT, SolidCore, and Truck.

## Licence

Licensed under the BSD 3-Clause license. See [`LICENSE`](LICENSE).

BrepSCAD depends on several third-party libraries. Of note:

- Truck is MIT/Apache-2.0 dual-licensed.
- clipper2-rust is BSL-1.0, the same license as Clipper2.

Planned backends:

- OCCT is LGPL-2.1 with an exception; if it is shipped, dynamic linking/relinkable distribution and notice obligations apply.
- SolidCore is Apache-2.0.
