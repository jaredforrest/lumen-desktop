{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-office-workflows";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [ ./. ../application-isolation ../fixtures/x11-us.xkb ];
  };
  sourceRoot = "source/office-workflows-rust";
  cargoLock.lockFile = ./Cargo.lock;
  RUSTFLAGS = "-D warnings";
  postPatch = ''
    mkdir -p ../fixtures
    cp ${../fixtures/x11-us.xkb} ../fixtures/x11-us.xkb
  '';
}
