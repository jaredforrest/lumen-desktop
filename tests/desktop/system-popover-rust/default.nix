{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-system-popover-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-system-popover-fixture"
    cp audit.fragment.qml "$out/share/lumen-system-popover-fixture/audit.fragment.qml"
  '';
}
