# Visual OpenSCAD

Working title for a B-rep-first visual editor for OpenSCAD-style `.scad` source.

> **Status: prototype.** The source parser, evaluator, Truck kernel, and viewer work for a small OpenSCAD subset.

## Goals

- Keep `.scad` text as the canonical document.
- Present a source-shaped tree; edits write back into the source immediately.
- Use B-rep as the canonical geometry representation.
- Support multiple kernel backends behind a common capability API: OCCT, SolidCore, and Truck.
- Add native fast paths for common library constructs (for example, BOSL2 rounded boxes) instead of forcing them through mesh CSG.
- Exact OpenSCAD output identity is *not* a goal. Use OpenSCAD itself when that is required.

Manifold is not a selectable backend. It may survive only as an internal mesh fallback for `hull`, `minkowski`, and mesh imports.

## Workspace

```text
crates/
  app/          binary entry point
  core/         HIR, evaluator, model tree (placeholder)
  kernel-api/   Kernel trait, ShapeId, capabilities, neutral mesh
  kernel-truck/ Truck adapter: cuboid, cylinder, boolean, tessellation
  scad-eval/    minimal evaluator: assignments, cube/cylinder, difference
  scad-syntax/  lossless CST, lexer, parser, and text edits
  viewer/       egui + wgpu viewer with source editor and orbit camera
```

The first implemented source layer is `scad-syntax`: a lossless CST for assignments, module calls, numbers (including negative literals), identifiers, nested vectors, comments, and whitespace, with byte-for-byte round-trip and range edits. The first kernel adapter is `kernel-truck`, covering cuboids, cylinders, transforms, booleans, and tessellation. A minimal `scad-eval` connects them for assignments, `cube`, `cylinder`, `translate`, `rotate`, `scale`, `multmatrix`, `mirror`, `resize`, and the boolean modules. The `viewer` binary renders the evaluated mesh with an egui source editor and a wgpu viewport.

The planned backend crates (`kernel-occt`, `kernel-solidcore`) and the UI/render crates are not created yet.

## Build

A `shell.nix` provides the Rust toolchain (cargo, rustc, rustfmt, clippy, rust-analyzer) and native build essentials.

### Nix + direnv

On NixOS, enable direnv and nix-direnv:

```nix
programs.direnv.enable = true;
programs.direnv.nix-direnv.enable = true;
```

Then, once per checkout:

```sh
direnv allow
cargo run -p visual-openscad
cargo test --workspace
```

Without direnv:

```sh
nix-shell --run 'cargo test --workspace'
nix-shell --run 'cargo run -p visual-openscad'
```

### Direct toolchain

With a Rust toolchain installed:

```sh
cargo run -p visual-openscad
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

`Cargo.lock` should be committed once generated; this is a binary workspace, not a library published to crates.io.

## Run

The CLI evaluates a `.scad` file through the Truck kernel and prints mesh statistics. With no path it uses a built-in cube-with-hole sample.

```sh
cargo run -p visual-openscad -- path/to/model.scad
```

The current subset supports top-level assignments, positional and named arguments, arithmetic expressions (`+`, `-`, `*`, `/`, `%`, unary signs, and parentheses), `cube`, `cylinder`, `translate` / `rotate` / `scale` / `multmatrix` / `mirror` / `resize` with a single child or block, and the `difference` / `union` / `intersection` modules with a block or a single child. `multmatrix` accepts a 4×3 or 4×4 matrix; `resize` accepts a `newsize` vector.

## Viewer

The first GUI prototype renders the evaluated mesh with a depth-tested wgpu viewport, an editable source panel, and an Evaluate button.

```sh
cargo run -p visual-openscad-viewer
```

Drag to orbit and scroll to zoom. Click the model to select an operand, or use the Shapes panel, which shows the source-shaped tree of booleans, transforms, and leaf primitives. Clicking a leaf selects it and fills the Properties panel, including cutters enclosed by the base shape. The Properties panel lists numeric literals in that operand and in the assignments it references. Drag a value, or click it and type a new number, then press Enter or click away. The edit is written back to the source and the model re-evaluates, reusing tessellated leaf meshes whose module, parameters, and transform did not change. The selected operand is not highlighted yet. The source panel remains the general edit loop. The viewer requires a working wgpu adapter (Vulkan or GLES); the Nix shell provides the runtime libraries.

### Headless UI snapshot

To inspect the UI without opening a window on the desktop, render it on a virtual X display:

```sh
cargo build -p visual-openscad-viewer
scripts/screenshot-viewer.sh /tmp/viewer.png
```

The script starts `Xvfb`, runs the viewer at 1280×800, captures one frame with ffmpeg's `x11grab`, and shuts everything down. It requires `Xvfb` and an ffmpeg build with `x11grab`; it does not need a window manager or a visible desktop.

## Documentation

- [`docs/research.md`](docs/research.md) — prior art, source-first architecture, kernel trade-offs, native library fast paths.
- [`docs/kernels.md`](docs/kernels.md) — per-operation capability matrix for OCCT, SolidCore, and Truck.

## Licence

BSD-3-Clause. See [`LICENSE`](LICENSE).

The copyright line in `LICENSE` is a placeholder; replace it with your legal name or entity before distributing.

Dependencies keep their own licences. In particular:

- OCCT is LGPL-2.1 with an exception; if it is shipped, dynamic linking/relinkable distribution and notice obligations apply.
- SolidCore and Truck are Apache-2.0.
- Manifold, if used internally for mesh-only operations, is Apache-2.0.

Do not add AGPL dependencies without changing the project licence.
