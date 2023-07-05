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
demo-url = "127.0.0.1:8080";
demo-config-server = writeText "config.toml" ''
version = "v1"

listen = "${demo-url}"

signing_key = "demo.nixcache-0:vjg4zb3o8U3SapIoeG5dWZ9+G4OyqA96J2+nxuoMPCT3a7/zXWgXpuKr+rJWChlyTGeCV2aARebK+ffmh+u2fw=="

[storage]
type = "local"
path = "/tmp/_demo_nixcache"
'';
demo-config-client = writeText "config.toml" ''
version = "v1"

[server]
endpoint = "http://${demo-url}"
'';

in rec {
  nixcache = rustPlatform.buildRustPackage {
    name = "nixcache";
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

  demo-nixcached = writeShellScriptBin "demo-nixcached" ''
    exec ${nixcache}/bin/nixcached --config ${demo-config-server}
  '';
  demo-nixcache = writeShellScriptBin "demo-nixcache" ''
    exec ${nixcache}/bin/nixcache --config ${demo-config-client}
  '';
  demo = symlinkJoin {
    name = "demo-nixcache";
    paths = [
      demo-nixcached
      demo-nixcache
    ];
  };

  default = nixcache;
}
