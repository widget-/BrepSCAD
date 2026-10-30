# Project status

This file tracks the current state. Dated snapshots live in
[`notes/`](notes/).

## What this project is

A B-rep-first visual editor for OpenSCAD-style `.scad` source. The `.scad`
text is the canonical document. The editor should present a source-shaped tree,
write edits back into the source immediately, and keep inspection state
(such as showing one intermediate state) session-only.

BrepSCAD is an independent project and is not affiliated with OpenSCAD.

Exact OpenSCAD output identity is explicitly out of scope. Users who need that
should use OpenSCAD itself. Manifold is not a selectable backend; at most it
could be an internal mesh fallback for inherently mesh operations such as
`hull`, `minkowski`, and mesh imports.

## Current workspace

```text
crates/
  cli/          CLI entry point (package `brepscad`)
  kernel-api/   kernel trait, ShapeId, capabilities, neutral mesh, matrix transform
  kernel-truck/ Truck B-rep adapter
  scad-eval/    evaluator and operand/parameter provenance
  scad-syntax/  lossless rowan CST, lexer, parser, text edits
  viewer/       eframe/egui/wgpu viewer
assets/
  sample.scad   built-in cube-with-hole source
docs/
  status.md     current capabilities and next steps
  notes/        dated session snapshots
  research.md   prior art, source-first architecture, kernel trade-offs
  kernels.md    OCCT / SolidCore / Truck capability matrix
scripts/
  screenshot-viewer.sh
```

## What works

### Source layer

- Lossless rowan CST for a small OpenSCAD subset.
- Trivia-preserving lexer.
- Tolerant recursive-descent parser with error recovery.
- `TextEdit` / `apply_edit` with bounds and UTF-8 boundary checks.
- Supported syntax: top-level assignments, module calls, positional and named
  arguments, numbers (including negative literals), identifiers, vectors,
  comments, whitespace.
- Expressions support `+`, `-`, `*`, `/`, and `%` with standard precedence,
  unary signs, and parentheses; the tree keeps every token.
- Transform and boolean calls support either a block or a single child module
  call.

### Evaluator

- Top-level assignments and variable lookup.
- `cube`, `cylinder`, `sphere`, `polyhedron`.
- `square`, `circle`, `polygon` sketches and `linear_extrude`.
- `rotate_extrude` with optional sweep and start angles.
- `translate`, `rotate`, `scale`, `multmatrix`, `mirror`, `resize`.
- Nodes carry a 2D/3D dimension; transforms preserve it, booleans require 3D,
  and `linear_extrude` turns exactly one 2D child into a solid.
- `difference`, `union`, `intersection` on 3D solids and on 2D profiles.
- 2D booleans run on polygonal profiles (clipper2-rust); results may contain
  holes or multiple disjoint regions.
- `offset(r=...)` and `offset(delta=...)` for 2D profiles. `r` uses round joins,
  `delta` uses miter joins.
- Positional and named arguments (`size=`, `r=`/`h=`, `v=`, `a=`, `m=`,
  `newsize=`); unknown names are rejected.
- Arithmetic on numbers and vectors: `+`, `-`, `*`, `/`, `%`, unary signs, and
  grouping. Division or modulo by zero is an expression error.
- Nested transforms compose into one matrix before reaching a primitive.
- Each evaluated top-level shape exposes:
  - its final mesh
  - its source range
  - numeric parameter bindings
  - selectable operand parts (leaf primitives) with their own meshes,
    source ranges, and parameter bindings
  - a source-shaped tree of groups and leaves; leaves index into `parts`
- Transform parameters are inherited by the leaf operands, so they appear in
  the properties panel and remain editable.
- Leaf meshes are cached across evaluations. The key covers the module, its
  resolved numeric parameters, the composed world transform, and the
  tessellation tolerance, so only leaves whose geometry changed are
  re-tessellated.

### Kernel

- `kernel-api` is a capability-based `Kernel` trait with opaque `ShapeId`s.
- `kernel-truck` supports:
  - cuboid, cylinder, sphere, and polyhedron primitives
  - square, circle, and polygon sketches
  - linear extrusion of a sketch along Z
  - rotational extrusion of a sketch around Z
  - matrix transforms on faces and solids
  - union, difference, intersection on solids
  - tessellation to a neutral triangle mesh
  - axis-aligned bounds for solids and sketches
- Truck is the only implemented backend. OCCT and SolidCore are candidates,
  not adapters yet.

### Viewer

- eframe/egui UI with an offscreen wgpu render target registered as an egui
  texture.
- Source editor + Evaluate button (Ctrl+Enter).
- Orbit camera: drag to orbit, scroll to zoom.
- CPU ray picking against operand meshes.
- Shapes panel showing the source-shaped tree of groups and leaf primitives.
  Clicking a leaf selects it, including cutters enclosed by other geometry.
- Properties panel with drag-editable numeric parameters.
- Parameter edits apply on drag release, Enter, or focus loss.
- Edits write back to the source, then re-evaluate.
- Built-in sample: `assets/sample.scad`, shared by the CLI, viewer, and
  `hello_cube` example.

- `scripts/screenshot-viewer.sh` captures the UI headlessly through Xvfb and
  ffmpeg, so verification does not open a window on the desktop.

## Important decisions

- Source-faithful editing: `.scad` text is canonical; inspection state is not
  written back.
- B-rep first. The kernel-neutral `Transform` is a column-major 4×4 affine
  matrix; `compose` applies the child first.
- OpenSCAD rotation order for `rotate([x, y, z])`: X, then Y, then Z.
- `multmatrix` rows are converted from OpenSCAD's row-major form into the
  internal column-major matrix.
- Singular or non-finite transforms are rejected before reaching the kernel.
  Reflections keep a negative determinant; the Truck adapter flips face
  orientation so mirrored solids stay outward-facing.
- `resize` fits the kernel bounds of its children in local space, then applies
  the accumulated parent transform. A zero dimension is left unchanged; the
  `auto` parameter is not supported.
- Truck is first because it is pure Rust, Apache-2.0, and needs no C++
  toolchain. OCCT is the likely C++ fallback; SolidCore is a watch item.
- `sphere` is built as a faceted UV B-rep with shared edges and planar
  triangles. Revolving a half-disc in Truck produced degenerate pole faces that
  broke tessellation and booleans, so the faceted shell is used instead.
- 2D profiles are centered in XY and extrusions are centered on Z, matching the
  centered 3D primitives rather than OpenSCAD's origin-at-a-corner convention.
  Profile children are filtered out of the selectable-part tree; the extrusion
  itself is the selectable part.
- `rotate_extrude` maps the profile's local X to radius and local Y to Z, then
  revolves around Z. `start` rotates the profile about Z before sweeping.
  Positive sweeps are flipped to keep outward orientation; profiles are
  required to lie at `x >= 0`.
- `polyhedron` builds shared edges from the input points and faces, then relies
  on `Solid::try_new` to validate the shell. Faces must be planar and the input
  must be closed and consistently wound; no repair is attempted.
- 2D CSG is a profile operation above the kernel: outer contours and holes are
  clipped with `clipper2-rust`, then rebuilt into one or more kernel faces.
  Analytic sketch faces remain the fast path for non-boolean extrusions;
  circles are flattened to 64 segments only when profiles enter 2D CSG.
- Multi-region profile results extrude or revolve per region and combine the
  resulting shells into one solid. Empty 2D boolean results are rejected because
  the model has no representation for empty geometry.
- `offset` inflates the profile with Clipper2, then runs an even-odd union to
  rebuild hole ownership. `r` selects round joins and `delta` miter joins; the
  offset distance is applied in local profile space before the parent transform.
- Truck boolean success is tolerance-sensitive, so failures retry at finer
  tolerances instead of raising a fixed clearance requirement.
- Backend adapters should remain separate crates behind the common trait.
- Native fast paths for common library constructs (for example, BOSL2 rounded
  boxes) are intended to avoid mesh-CSG workarounds; `scad-intrinsics` is not
  created yet.

## Commands

The project uses Nix/direnv because Rust is not installed system-wide.

```sh
cd /home/widget/code/misc/visual-openscad
direnv allow .
nix-shell --run 'cargo test --workspace'
nix-shell --run 'cargo run -p brepscad-viewer'
```

Full check:

```sh
nix-shell --run 'cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace'
```

CLI:

```sh
nix-shell --run 'cargo run -p brepscad -- path/to/model.scad'
```

The viewer needs a working wgpu adapter (Vulkan or GLES). On this machine it
runs under Wayland. A quick smoke test is:

```sh
nix-shell --run 'timeout 8s target/debug/brepscad-viewer'
```

Exit code `124` means it stayed alive until the timeout and did not panic.

For a screenshot without opening a window on the desktop, use the headless
capture script:

```sh
scripts/screenshot-viewer.sh /tmp/viewer.png
```

It starts `Xvfb` on a free display, runs the viewer, captures one frame with
ffmpeg's `x11grab`, and cleans up. Requires `Xvfb` and an ffmpeg with
`x11grab`; no window manager is needed.

## Known limitations

- Parser/evaluator subset is small:
  - no strings or booleans
  - no `module` / `function` declarations
  - no `for` / `if`
  - 2D booleans and offsets are polygonal; circles flatten to 64 segments and
    there is no curve intersection, chamfer, or `$fn` support
  - empty 2D boolean or offset results are reported as errors
  - `linear_extrude` supports `height` only; no `center`, twist, scale, or slices
  - `rotate_extrude` requires `x >= 0` and has no negative-side profiles
  - `polyhedron` requires closed, consistently wound planar faces
  - `sphere` is a fixed 32×16 facet approximation; no `$fn`, `$fa`, or `$fs`
- No color or `$fn` handling.
- `resize` supports a positional `newsize` only; `auto` and named arguments are
  not supported. It asks the kernel for bounds, which are tessellation-derived,
  so it is slower than a matrix-only transform.
- Truck booleans try tolerances `0.05`, `0.02`, then `0.01` and keep the first
  success. Some configurations still fail, and a retry can take seconds.
- Truck booleans can still fail on touching or coincident faces.
- Selection:
  - per leaf primitive, not per nested boolean group
  - no selected-part highlight
  - enclosed cutters are reachable from the Shapes tree, but there is no
    cycling between overlapping parts
- No file open/save, export, undo, or persistence beyond the in-memory source.
- Truck transitive dependencies `nom 3.2.1` and `quick-xml 0.22.0` emit
  future-incompatibility warnings.
- Licence is BSD-3-Clause with a placeholder copyright holder; replace before
  distributing.

## Recommended next steps

1. **Finish the 2D/extrusion layer**
   - Polygonal profile smoothing and `offset` chamfer/$fn options.
   - `linear_extrude` options: center, twist, scale, slices.
   - `rotate_extrude` negative-side profiles.
   - `for` / `if` and module/function definitions later.
2. **Native intrinsics**
   - Start with the common BOSL2 `cuboid(rounding=)` case.
   - Always fall back to ordinary evaluation.
   - Version intrinsics by source path/content hash.
3. **Kernel adapters**
   - Add OCCT and SolidCore behind feature flags/crates.
   - Build the conformance harness before claiming capabilities.
4. **Selection polish**
   - Highlight selected operand.
   - Ghost or cycle overlapping parts.

## Relevant docs

- `docs/status.md` — this file: current capabilities, limitations, and next
  steps.
- `docs/notes/2026-10-30.md` — first-session snapshot and checkout sync note.
- `docs/research.md` — prior art, source-first architecture, kernel trade-offs,
  native library fast paths.
- `docs/kernels.md` — per-operation OCCT/SolidCore/Truck capability matrix.
- `README.md` — current build/run instructions and supported subset.
