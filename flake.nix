{
  inputs = {
    nixpkgs.url = github:NixOS/nixpkgs;
    flake-utils.url = github:numtide/flake-utils;
    devshell = {
      url = "github:numtide/devshell";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    rust-overlay = {
      url = github:oxalica/rust-overlay;
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = { self, nixpkgs, flake-utils, devshell, rust-overlay }:
    with flake-utils.lib;
    eachSystem [ system.x86_64-linux ] (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          devshell.overlays.default
          rust-overlay.overlays.default
        ];
      };
      custom-rust = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "rust-src" ];
        targets = [ "x86_64-unknown-linux-gnu" ];
      };

    in {
      devShell = pkgs.mkShell {
        buildInputs = with pkgs; [
          git
          rust-analyzer
          cargo-edit

          custom-rust
          gcc
          pkg-config
          nixVersions.nix_2_10
          boost
          libclang.lib
          rustPlatform.bindgenHook
        ];
        NIX_PATH = "nixpkgs=${pkgs.path}";
        LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
        RUST_SRC_PATH = "${pkgs.rustPlatform.rustcSrc}/library";
      };
    });
}
