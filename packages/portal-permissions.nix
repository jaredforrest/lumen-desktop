{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-portal-permissions";
  version = "0.1.0";
  src = ../res/lumen-desktop/portal-permissions;
  cargoLock.lockFile = ../res/lumen-desktop/portal-permissions/Cargo.lock;
}
