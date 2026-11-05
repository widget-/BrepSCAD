# Development

Helpers for working on BrepSCAD. User-facing usage lives in the
[README](../README.md); current capabilities and limitations live in
[status.md](status.md).

## Headless UI snapshot

The viewer normally needs a desktop session. To inspect its UI without opening
a window on the desktop, render it on a virtual X display:

```sh
cargo build -p brepscad-lsp -p brepscad-viewer
scripts/screenshot-viewer.sh /tmp/viewer.png
```

`scripts/screenshot-viewer.sh <output.png> [settle-seconds]` starts `Xvfb` on a
free display, runs `target/debug/brepscad-viewer` at 1280×800, waits four
seconds by default, captures one frame with ffmpeg's `x11grab`, and shuts
everything down. The output path is printed on success.

Run the script inside the dev shell (`nix-shell --run 'scripts/screenshot-viewer.sh ...'`)
so the viewer can find `libxkbcommon-x11` and the wgpu libraries through
`LD_LIBRARY_PATH`. The captured output path is absolute; pass a path outside
the shell's temporary directory if you need it to outlive the `nix-shell`.

Requirements:

- `Xvfb`
- ffmpeg built with `x11grab`
- a working wgpu adapter (Vulkan or GLES) for the viewer
- a built viewer binary; set `VIEWER_BIN` to override the default
  `target/debug/brepscad-viewer` path

Native file dialogs use `rfd`. On Linux they need an XDG desktop portal, with a
Zenity fallback for file dialogs. Headless Xvfb runs cannot open native dialogs,
so the screenshot helper exercises the viewer UI but not the file picker.

The script does not need a window manager or a visible desktop. It picks a
random free display number between `:100` and `:199`, so it can run alongside an
existing X session.

## Language server

Build the standalone server with:

```sh
cargo build -p brepscad-lsp
cargo test -p brepscad-lsp
```

`brepscad-lsp` speaks Content-Length JSON-RPC over stdin/stdout. It supports
full document sync, diagnostics, completion, hover, definition, and document
symbols, using `brepscad-syntax` and the shared built-in metadata in
`brepscad-eval`; it never runs geometry evaluation.

The viewer starts the server automatically when it finds `brepscad-lsp` next to
the viewer binary or on `PATH`. Set `BREPSCAD_LSP` to use another executable.
If the server is absent or exits, the editor keeps its local highlighting,
diagnostics, and completion. External clients can launch `brepscad-lsp`
directly on `.scad` files.

## SCAD corpus check

Real-file compatibility is measured against pinned BOSL2 and MCAD checkouts.
The revisions live in [`corpus/revisions.tsv`](../corpus/revisions.tsv); the
checkouts are fetched into `.corpus/`, which is ignored by git because MCAD is
LGPL-2.1 and is not vendored.

```sh
scripts/fetch-scad-corpus.sh
cargo build -p brepscad
scripts/check-scad-corpus.sh .corpus
```

Parse-only mode is the default and is the language-syntax signal. It runs
`brepscad --parse-only` on every `.scad` file and reports parser coverage
without invoking geometry. Use `--evaluate` to run the full evaluator and
separate language errors from missing geometry modules and kernel failures:

```sh
scripts/check-scad-corpus.sh --evaluate .corpus
```

When pointed at `.corpus` (or a directory inside it), the script exports
`OPENSCADPATH` to the corpus root so `<BOSL2/std.scad>` and `<MCAD/units.scad>`
resolve. Quoted includes still resolve relative to the including file.

Both modes can write checkable snapshots:

```sh
scripts/check-scad-corpus.sh --parse-only --write-baseline corpus/baseline-parse.tsv .corpus
scripts/check-scad-corpus.sh --parse-only --baseline corpus/baseline-parse.tsv .corpus
scripts/check-scad-corpus.sh --evaluate --write-baseline corpus/baseline-evaluate.tsv .corpus
scripts/check-scad-corpus.sh --evaluate --baseline corpus/baseline-evaluate.tsv .corpus
```

A baseline run exits non-zero on regressions, new files, or missing files.
`--results FILE` writes a per-file `PATH<TAB>STATUS<TAB>ERROR` table, and
`--verbose` prints every non-ok result. Set `BREPSCAD_BIN` to use another
binary and `CORPUS_TIMEOUT` to change the per-file timeout (default 8 seconds).

## Model metrics check

The corpus check answers whether a file evaluates. When changing the boolean
engine, also record what it built and how long it took. `check-model-metrics.sh`
runs the release CLI on model files and captures wall time, triangles, vertices,
volume, and welded boundary and non-manifold edges:

```sh
cargo build --release -p brepscad
scripts/check-model-metrics.sh --baseline corpus/baseline-metrics.tsv
```

With no arguments it measures the in-repo model fixtures; pass files or
directories to cover more models. Directories are searched recursively for
`.scad` files. Write a baseline for an external set (paths are recorded as
given) and compare against it after a change:

```sh
scripts/check-model-metrics.sh --write-baseline /tmp/opengrid-metrics.tsv \
    path/to/opengrid-bin.scad path/to/opengrid-snap.scad
scripts/check-model-metrics.sh --baseline /tmp/opengrid-metrics.tsv \
    path/to/opengrid-bin.scad path/to/opengrid-snap.scad
```

Status, triangle/vertex counts, and welded edge counts are compared exactly;
volume uses a relative tolerance (`METRICS_VOLUME_TOL`, default `1e-6`). Timing
is printed but never compared because it is machine-dependent. `--repeat N`
takes the fastest of N runs. Set `BREPSCAD_BIN` to use another binary and
`METRICS_TIMEOUT` (or `--timeout`) to change the per-run timeout (default 120
seconds).

## Toolchain

`shell.nix` provides `rustup`; `rust-toolchain.toml` pins Rust 1.95.0 with
clippy, rustfmt, and rust-analyzer. The first `nix-shell` or `direnv allow`
downloads that toolchain. Do not install `cargo` or `rustc` from nixpkgs
alongside it, or they will shadow the rustup proxies.

## Full check

```sh
cargo fmt --all --check &&
cargo clippy --workspace --all-targets -- -D warnings &&
cargo test --workspace
```
