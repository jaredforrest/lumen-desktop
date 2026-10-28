{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-portal-permission-provider-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
