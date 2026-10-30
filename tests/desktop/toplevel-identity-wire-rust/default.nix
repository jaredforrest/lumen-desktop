{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-toplevel-identity-wire";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
