{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-office-capability-records";
  version = "0.1.0";
  src = ../tests/desktop/office-records-rust;
  cargoLock.lockFile = ../tests/desktop/office-records-rust/Cargo.lock;
}
