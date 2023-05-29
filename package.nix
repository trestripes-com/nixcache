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
    outputHashes = {
      "attic-0.1.0" = "sha256-9L+OZeU3bcNZ55mhMINBxnqskbaEU0mhiZIMhkEtNl0=";
      "nix-base32-0.1.2-alpha.0" = "sha256-wtPWGOamy3+ViEzCxMSwBcoR4HMMD0t8eyLwXfCDFdo=";
    };
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
