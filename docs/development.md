# Development

Helpers for working on BrepSCAD. User-facing usage lives in the
[README](../README.md); current capabilities and limitations live in
[status.md](status.md).

## Headless UI snapshot

The viewer normally needs a desktop session. To inspect its UI without opening
a window on the desktop, render it on a virtual X display:

```sh
cargo build -p brepscad-viewer
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

## SCAD corpus check

To measure real-file compatibility, point the corpus helper at any directory of
`.scad` files:

```sh
cargo build -p brepscad
scripts/check-scad-corpus.sh /path/to/BOSL2 /path/to/MCAD
```

For each file it runs `target/debug/brepscad`, records the first error, and
prints totals for parsed files, parse errors, evaluation errors, and timeouts.
It is a compatibility probe, not a pass/fail test: it exits successfully even
when individual files fail. Set `BREPSCAD_BIN` to use another binary and
`CORPUS_TIMEOUT` to change the per-file timeout (default 8 seconds).

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
