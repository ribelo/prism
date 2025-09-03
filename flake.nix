{
  description = "A flake for Setu project with nightly Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            (rust-bin.nightly.latest.default.override {
              extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
            })
          ];

          buildInputs = with pkgs; [
            openssl
          ];
        in
        with pkgs;
        {
          devShells.default = mkShell {
            nativeBuildInputs = nativeBuildInputs;
            buildInputs = buildInputs;

            shellHook = ''
              echo "Setu development environment loaded!"
              echo "Using nightly Rust toolchain"
            '';
          };
        });
}