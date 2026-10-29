{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-reader-commands-provider-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
