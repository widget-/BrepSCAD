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

Requirements:

- `Xvfb`
- ffmpeg built with `x11grab`
- a working wgpu adapter (Vulkan or GLES) for the viewer
- a built viewer binary; set `VIEWER_BIN` to override the default
  `target/debug/brepscad-viewer` path

The script does not need a window manager or a visible desktop. It picks a
random free display number between `:100` and `:199`, so it can run alongside an
existing X session.

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
