{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-airdrop-ui";
  version = "0.1.0";
  src = ../res/lumen-desktop/airdrop-ui;
  cargoLock.lockFile = ../res/lumen-desktop/airdrop-ui/Cargo.lock;
}
