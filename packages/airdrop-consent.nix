{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-airdrop-consent";
  version = "0.1.0";
  src = ../res/lumen-desktop/airdrop-consent;
  cargoLock.lockFile = ../res/lumen-desktop/airdrop-consent/Cargo.lock;
}
