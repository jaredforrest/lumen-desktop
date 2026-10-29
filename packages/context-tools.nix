{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-context-tools";
  version = "0.1.0";
  src = ../res/lumen-desktop/context-tools;
  cargoLock.lockFile = ../res/lumen-desktop/context-tools/Cargo.lock;
  cargoTestFlags = [ "--" "--test-threads=1" ];
}
