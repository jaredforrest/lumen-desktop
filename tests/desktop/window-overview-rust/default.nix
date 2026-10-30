{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-window-overview-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-window-overview-fixture"
    cp fixture.qml "$out/share/lumen-window-overview-fixture/fixture.qml"
  '';
}
