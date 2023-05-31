{ rustPlatform
, pkg-config
, gcc
, nix
, boost
, libclang
, writeText
, writeShellScriptBin
, symlinkJoin
}:

let
demo-config = writeText "config.toml" ''
listen = "127.0.0.1:8080"

signing_key = "demo.nixcache-0:vjg4zb3o8U3SapIoeG5dWZ9+G4OyqA96J2+nxuoMPCT3a7/zXWgXpuKr+rJWChlyTGeCV2aARebK+ffmh+u2fw=="

[storage]
type = "local"
path = "/tmp/_demo_nixcache"
'';

in rec {
  nixcache = rustPlatform.buildRustPackage {
    name = "nixcached";
    src = ./.;
    cargoLock = {
      lockFile = ./Cargo.lock;
    };

    nativeBuildInputs = [
      pkg-config
      rustPlatform.bindgenHook
      gcc
    ];
    buildInputs = [
      nix
      boost
      libclang.lib
    ];

    doCheck = false;
  };

  nixcached-demo = writeShellScriptBin "nixcached-demo" ''
    exec ${nixcache}/bin/nixcached --config ${demo-config}
  '';

  default = symlinkJoin {
    name = "nixcache-all";
    paths = [
      nixcache
      nixcached-demo
    ];
  };
}
