{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-nearby-provider-fixture";
  version = "0.1.0";
  src = ../res/lumen-desktop/nearby-provider-fixture;
  cargoLock.lockFile = ../res/lumen-desktop/nearby-provider-fixture/Cargo.lock;
}
