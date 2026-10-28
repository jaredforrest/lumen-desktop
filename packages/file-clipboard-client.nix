{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-file-clipboard-client";
  version = "0.1.0";
  src = ../res/lumen-desktop/file-clipboard-client;
  cargoLock.lockFile = ../res/lumen-desktop/file-clipboard-client/Cargo.lock;
}
