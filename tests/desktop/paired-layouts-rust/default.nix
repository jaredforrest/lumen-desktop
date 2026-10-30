{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-paired-layouts-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  LUMEN_PAIR_RELATIONSHIP_CLIENT = "${import ../window_relationship_client.nix { inherit pkgs; }}/bin/lumen-test-window-relationships";
}
