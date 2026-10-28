{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-effects-software-render";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
