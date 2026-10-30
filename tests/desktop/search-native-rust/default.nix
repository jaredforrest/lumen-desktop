{ pkgs }:
let
  quickshell = import ../../../packages/quickshell-runtime.nix { inherit pkgs; };
  inputMethod = pkgs.runCommand "lumen-test-input-method" {
    nativeBuildInputs = [ pkgs.stdenv.cc pkgs.pkg-config pkgs.wayland-scanner ];
    buildInputs = [ pkgs.wayland ];
  } ''
    wayland-scanner client-header ${../../../res/character-viewer/protocols/input-method-unstable-v2.xml} input-method-client.h
    wayland-scanner private-code ${../../../res/character-viewer/protocols/input-method-unstable-v2.xml} protocol.c
    mkdir -p "$out/bin"
    cc -Wall -Wextra -Werror -I. ${./input-method.c} protocol.c $(pkg-config --cflags --libs wayland-client) -o "$out/bin/lumen-test-input-method"
  '';
in
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-search-native-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  env.LUMEN_TEST_INPUT_METHOD = "${inputMethod}/bin/lumen-test-input-method";
  env.LUMEN_TEST_COMPOSITION_TOOLS = pkgs.lib.makeBinPath [ quickshell pkgs.dbus pkgs.coreutils ];
}
