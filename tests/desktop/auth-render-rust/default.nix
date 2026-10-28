{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-auth-render-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
