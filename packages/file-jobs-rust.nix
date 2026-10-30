{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-file-jobs";
  version = "0.1.0";
  src = ../res/lumen-desktop/file-jobs;
  cargoLock.lockFile = ../res/lumen-desktop/file-jobs/Cargo.lock;
  cargoTestFlags = [ "--" "--test-threads=1" ];
}
