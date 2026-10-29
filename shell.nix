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

  RUST_BACKTRACE = "1";
  PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" [ pkgs.openssl.dev ];

  shellHook = ''
    echo "visual-openscad dev shell"
    echo "  rustc: $(rustc --version)"
    echo "  cargo: $(cargo --version)"
  '';
}
