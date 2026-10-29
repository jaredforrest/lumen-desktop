{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-power-provider-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
