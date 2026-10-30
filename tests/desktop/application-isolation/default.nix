{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-application-isolation";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.git ];
  LUMEN_PRIVATE_SSH_SHELL = "${pkgs.bash}/bin/bash";
  LUMEN_BWRAP = "${pkgs.bubblewrap}/bin/bwrap";
}
