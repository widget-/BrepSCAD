{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  packages = with pkgs; [
    # rustup owns the toolchain; do not also install nixpkgs cargo/rustc,
    # which would shadow the rustup proxies.
    rustup
    pkg-config
    openssl
    cmake
    gcc
    git
  ];

  buildInputs = with pkgs; [
    libxkbcommon
    wayland
    vulkan-loader
    mesa
    xorg.libX11
    xorg.libXcursor
    xorg.libXrandr
    xorg.libXi
  ];

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.libxkbcommon
    pkgs.wayland
    pkgs.vulkan-loader
    pkgs.mesa
    pkgs.xorg.libX11
    pkgs.xorg.libXcursor
    pkgs.xorg.libXrandr
    pkgs.xorg.libXi
  ];

  RUST_BACKTRACE = "1";
  PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" [ pkgs.openssl.dev ];

  shellHook = ''
    export PATH="$HOME/.cargo/bin:$PATH"

    if ! rustup toolchain list | grep -q '^1\.95\.0'; then
      rustup toolchain install 1.95.0 --profile minimal \
        --component clippy --component rustfmt --component rust-analyzer
    fi
    rustup component add --toolchain 1.95.0 clippy rustfmt rust-analyzer >/dev/null

    echo "brepscad dev shell"
    echo "  rustc: $(rustc --version)"
    echo "  cargo: $(cargo --version)"
  '';
}
