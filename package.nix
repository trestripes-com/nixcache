{ rustPlatform
, pkg-config
, gcc
, nix
, boost
, libclang
, symlinkJoin
}:

let
builder = rustPlatform.buildRustPackage {
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

in rec {
  nixcached = builder.overrideAttrs (drv: {
    name = "nixcached";
    cargoBuildFlags = "-p server";
  });
  nixcache = builder.overrideAttrs (drv: {
    name = "nixcache";
    cargoBuildFlags = "-p client";
  });
  default = symlinkJoin {
    name = "nixcache-all";
    paths = [
      nixcached
      nixcache
    ];
  };
}
