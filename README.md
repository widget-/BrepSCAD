# Visual OpenSCAD

Working title for a B-rep-first visual editor for OpenSCAD-style `.scad` source.

> **Status: scaffold.** Only a hello-world binary and the initial workspace layout exist. Nothing is implemented yet.

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
  kernel-api/   Kernel trait, ShapeId, capability flags (placeholder)
  scad-syntax/  lossless CST, lexer, parser, and text edits
```

The first implemented layer is `scad-syntax`: a lossless CST for assignments, module calls, numbers, identifiers, vectors, comments, and whitespace, with byte-for-byte round-trip and range edits.

The planned backend crates (`kernel-occt`, `kernel-solidcore`, `kernel-truck`) and the UI/render crates are not created yet.

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
