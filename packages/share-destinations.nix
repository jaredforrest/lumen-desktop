{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-share-destinations";
  version = "0.1.0";
  src = ../res/lumen-desktop/share-destinations;
  cargoLock.lockFile = ../res/lumen-desktop/share-destinations/Cargo.lock;
  postInstall = "rm $out/bin/lumen-share-provider-fixture";
  checkFlags = [ "--test-threads=1" ];
}
