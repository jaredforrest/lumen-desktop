{ pkgs, nixSettings, contextTools }:
let
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-text-tools";
    version = "0.1.0";
    src = ../res/lumen-desktop/text-tools;
    cargoLock.lockFile = ../res/lumen-desktop/text-tools/Cargo.lock;
    cargoTestFlags = [ "--" "--test-threads=1" ];
  };
  nuWrapped = import ./nu-wrapped-rust.nix { inherit pkgs; };
  wrap = name: nuWrapped {
    inherit name binary;
    binaryName = "lumen-text-tools";
    runtimeInputs = [ pkgs.systemd pkgs.sway pkgs.wtype pkgs.wl-clipboard pkgs.wordnet pkgs.espeak-ng pkgs.whisper-cpp pkgs.pipewire pkgs.dbus ];
    environment = {
      LUMEN_CONTEXT_TOOLS = "${contextTools}/bin/lumen-context-tools";
      LUMEN_NIX_SETTINGS_BIN = "${nixSettings}/bin/lumen-nix-settings";
    };
  };
in pkgs.symlinkJoin {
  name = "lumen-text-tools";
  paths = [ (wrap "lumen-text-tools") (wrap "lumen-menu-command") ];
}
