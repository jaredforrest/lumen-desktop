{ pkgs }:
(import ../../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-menu-exporter-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.gtk3 ];
}
