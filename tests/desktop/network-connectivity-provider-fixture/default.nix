{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-network-connectivity-provider-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
