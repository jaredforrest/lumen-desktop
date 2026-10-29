{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-owned-window-context-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-owned-window-context-fixture"
    cp standalone.qml full-shell.fragment.qml "$out/share/lumen-owned-window-context-fixture/"
  '';
}
