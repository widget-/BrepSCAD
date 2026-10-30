# Session notes — 2026-10-30

## Snapshot

- Repository: `/home/widget/code/misc/visual-openscad`
- Branch: `main`
- HEAD: `4c6d29d refactor: use composed matrix transforms`
- Working tree: clean before this note was added
- Test suite: 41 tests passing

## What this project is

A B-rep-first visual editor for OpenSCAD-style `.scad` source. The `.scad`
text is the canonical document. The editor should present a source-shaped tree,
write edits back into the source immediately, and keep inspection state
(such as showing one intermediate state) session-only.

Exact OpenSCAD output identity is explicitly out of scope. Users who need that
should use OpenSCAD itself. Manifold is not a selectable backend; at most it
could be an internal mesh fallback for inherently mesh operations such as
`hull`, `minkowski`, and mesh imports.

## Current workspace

```text
crates/
  app/          CLI entry point
  core/         placeholder library
  kernel-api/   kernel trait, ShapeId, capabilities, neutral mesh, matrix transform
  kernel-truck/ Truck B-rep adapter
  scad-eval/    evaluator and operand/parameter provenance
  scad-syntax/  lossless rowan CST, lexer, parser, text edits
  viewer/       eframe/egui/wgpu viewer
docs/
  research.md   prior art, source-first architecture, kernel trade-offs
  kernels.md    OCCT / SolidCore / Truck capability matrix
```

## What works

### Source layer

- Lossless rowan CST for a small OpenSCAD subset.
- Trivia-preserving lexer.
- Tolerant recursive-descent parser with error recovery.
- `TextEdit` / `apply_edit` with bounds and UTF-8 boundary checks.
- Supported syntax: top-level assignments, module calls, numbers, identifiers,
  vectors, comments, whitespace.
- Transform calls support either a block or a single child module call.

### Evaluator

- Top-level assignments and variable lookup.
- `cube`, `cylinder`.
- `translate`, `rotate`, `scale`.
- `difference`, `union`, `intersection` with brace blocks.
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

### Kernel

- `kernel-api` is a capability-based `Kernel` trait with opaque `ShapeId`s.
- `kernel-truck` supports:
  - cuboid and cylinder primitives
  - matrix transforms
  - union, difference, intersection
  - tessellation to a neutral triangle mesh
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
- Built-in sample:

```scad
size = [2, 2, 2];
difference() {
    cube(size);
    translate([0.5, 0, 0]) cylinder(0.5, 4);
}
```

## Important decisions

- Source-faithful editing: `.scad` text is canonical; inspection state is not
  written back.
- B-rep first. The kernel-neutral `Transform` is a column-major 4×4 affine
  matrix; `compose` applies the child first.
- OpenSCAD rotation order for `rotate([x, y, z])`: X, then Y, then Z.
- Truck is first because it is pure Rust, Apache-2.0, and needs no C++
  toolchain. OCCT is the likely C++ fallback; SolidCore is a watch item.
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
nix-shell --run 'cargo run -p visual-openscad-viewer'
```

Full check:

```sh
nix-shell --run 'cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace'
```

CLI:

```sh
nix-shell --run 'cargo run -p visual-openscad -- path/to/model.scad'
```

The viewer needs a working wgpu adapter (Vulkan or GLES). On this machine it
runs under Wayland. A quick smoke test is:

```sh
nix-shell --run 'timeout 8s target/debug/visual-openscad-viewer'
```

Exit code `124` means it stayed alive until the timeout and did not panic.

## Known limitations

- Parser/evaluator subset is small:
  - no named arguments
  - no arithmetic or general expressions
  - no strings or booleans
  - no `module` / `function` declarations
  - no `for` / `if`
  - no `sphere`, `polyhedron`, extrusions, or 2D primitives
- Booleans currently require brace blocks; single-child boolean forms are not
  supported.
- No `multmatrix`, `mirror`, `resize`, color, or `$fn` handling. The matrix API
  makes `multmatrix`/`mirror` straightforward to add.
- Non-uniform scale and reflections still need kernel-specific validation;
  singular matrices are not rejected.
- Truck booleans use a fixed tolerance (`0.05`) and can fail on touching or
  coincident faces.
- No evaluation cache. Each reload re-evaluates and re-tessellates the final
  shape plus every selectable operand.
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

1. **Evaluation cache**
   - Cache parts by source range/content hash.
   - Avoid re-tessellating every operand on each parameter edit.
2. **Complete the transform set**
   - `multmatrix`, `mirror`, `resize`.
   - Validate singular matrices and reflections.
   - Decide on non-uniform scale policy per kernel.
3. **Expand the language subset**
   - Named arguments, expressions, `sphere`, extrusions.
   - Single-child boolean forms.
   - `for` / `if` and module/function definitions later.
4. **Native intrinsics**
   - Start with the common BOSL2 `cuboid(rounding=)` case.
   - Always fall back to ordinary evaluation.
   - Version intrinsics by source path/content hash.
5. **Kernel adapters**
   - Add OCCT and SolidCore behind feature flags/crates.
   - Build the conformance harness before claiming capabilities.
6. **Selection polish**
   - Highlight selected operand.
   - Ghost or cycle overlapping parts.

## Relevant docs

- `docs/research.md` — prior art, source-first architecture, kernel trade-offs,
  native library fast paths.
- `docs/kernels.md` — per-operation OCCT/SolidCore/Truck capability matrix.
- `README.md` — current build/run instructions and supported subset.

## Sync note

This checkout was copied from `widget-desktop` to
`laptop:/home/widget/code/misc/visual-openscad` with `rsync`, excluding
`target/`, `.direnv/`, and `result*`. The Git history is included. No remote
exists yet.
