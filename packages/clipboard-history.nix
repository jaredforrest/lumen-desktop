{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-clipboard-history";
  version = "0.1.0";
  src = ../res/lumen-desktop/clipboard-history;
  cargoLock.lockFile = ../res/lumen-desktop/clipboard-history/Cargo.lock;
}
