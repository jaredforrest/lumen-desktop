{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-speech-transport-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  LUMEN_TAR = "${pkgs.gnutar}/bin/tar";
  LUMEN_TRUE = "${pkgs.coreutils}/bin/true";
}
