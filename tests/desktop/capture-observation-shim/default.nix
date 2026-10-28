{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-capture-observation-shim";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
