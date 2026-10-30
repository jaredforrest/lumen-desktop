{ pkgs, nixpkgs, system }:
let
  fixture = values: import ./shortcuts.nix { inherit nixpkgs system; bundle = true; editable = true; settingsValues.shortcuts.rebindings = values; reservedBindings = [{ id = "consumer.lock"; action = "session.lock"; chord = "Mod4+Control+F12"; }]; };
  defaults = fixture {};
  rebound = fixture { "window.fullscreen.toggle" = "Mod4+Control+x"; };
  disabled = fixture { "window.fullscreen.toggle" = null; };
  keymap = pkgs.runCommand "lumen-shortcut-fixture-keymap" {} ''
    ${pkgs.libxkbcommon}/bin/xkbcli compile-keymap --include ${pkgs.xkeyboard_config}/share/X11/xkb --layout us --format 1 > $out
  '';
  readOnly = import ./shortcuts.nix { inherit nixpkgs system; bundle = true; };
in pkgs.writeText "lumen-shortcut-editor-fixtures.json" (builtins.toJSON {
  version = 1;
  states = [
    { settings.shortcuts.rebindings = {}; bundle = defaults; }
    { settings.shortcuts.rebindings."window.fullscreen.toggle" = "Mod4+Control+x"; bundle = rebound; }
    { settings.shortcuts.rebindings."window.fullscreen.toggle" = null; bundle = disabled; }
  ];
  inherit readOnly keymap;
  layout = "${pkgs.callPackage ../../packages/shortcut-layout.nix {}}/bin/lumen-shortcut-layout";
  helper = "${pkgs.callPackage ../../packages/shortcut-settings.nix {}}/bin/lumen-shortcut-settings";
  witness = "${pkgs.callPackage ../../packages/shortcut-settings.nix {}}/bin/lumen-shortcut-runtime-witness";
  validator = "${pkgs.callPackage ../../packages/shortcuts.nix {}}/bin/lumen-shortcuts";
})
