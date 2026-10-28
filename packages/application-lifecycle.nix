{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-application-lifecycle";
  version = "0.1.0";
  src = ../res/lumen-desktop/application-lifecycle;
  cargoLock.lockFile = ../res/lumen-desktop/application-lifecycle/Cargo.lock;
}
