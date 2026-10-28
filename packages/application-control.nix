{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-application-control";
  version = "0.1.0";
  src = ../res/lumen-desktop/application-control;
  cargoLock.lockFile = ../res/lumen-desktop/application-control/Cargo.lock;
  nativeCheckInputs = [ pkgs.sway pkgs.coreutils ];
}
