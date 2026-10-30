{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-files-share-menu-fixture";
  version = "0.1.0";
  src = ./.;
  buildInputs = [ pkgs.libxkbcommon ];
  cargoLock.lockFile = ./Cargo.lock;
}
