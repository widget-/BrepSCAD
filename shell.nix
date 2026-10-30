{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  packages = with pkgs; [
    cargo
    rustc
    rustfmt
    clippy
    rust-analyzer

    # Native build essentials. Heavy CAD deps (OCCT, TBB, X11, fontconfig)
    # will be added with the first B-rep backend.
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
    echo "brepscad dev shell"
    echo "  rustc: $(rustc --version)"
    echo "  cargo: $(cargo --version)"
  '';
}
