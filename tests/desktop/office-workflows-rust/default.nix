{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-office-workflows";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postPatch = ''
    mkdir -p ../fixtures
    cp ${../fixtures/x11-us.xkb} ../fixtures/x11-us.xkb
  '';
}
