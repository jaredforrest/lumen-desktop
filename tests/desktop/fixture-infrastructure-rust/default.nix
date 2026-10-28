{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-fixture-infrastructure";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeCheckInputs = [ pkgs.dbus ];
}
