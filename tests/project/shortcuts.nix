# Exercise the actual merged-binding export without starting a desktop session.
{ nixpkgs, system, extraBindings ? {}, applicationBindings ? [], reservedBindings ? [], rebind ? null, bundle ? false, settingsValues ? {}, editable ? false }:
let
  lib = nixpkgs.lib;
  evaluated = lib.nixosSystem {
    inherit system;
    modules = [ ../../modules/desktop.nix ({ lib, pkgs, ... }: {
      options.home-manager.users = lib.mkOption { type = lib.types.attrsOf lib.types.raw; default = {}; };
      config = {
        system.stateVersion = "26.05";
        time.timeZone = "UTC";
        services.lumen-desktop = {
          enable = true; user = "test-user"; settingsPackage = pkgs.hello;
          shortcuts = { inherit applicationBindings reservedBindings editable; rebindings = (import ../../packages/desktop-settings-schema.nix { inherit lib; values = settingsValues; }).shortcuts.rebindings; };
        };
      };
    }) ];
  };
  pkgs = evaluated.pkgs;
  unwrap = value: if builtins.isAttrs value && value ? _type && value._type == "override" then unwrap value.content else value;
  defaults = lib.mapAttrs (_: unwrap) (unwrap home.wayland.windowManager.sway.config.keybindings);
  effective = defaults // extraBindings // lib.optionalAttrs (rebind != null) { ${rebind.from} = null; ${rebind.to} = defaults.${rebind.from}; };
  home = evaluated.config.home-manager.users.test-user {
    inherit lib;
    config = {
      home.username = "test-user";
      home.homeDirectory = "/home/test-user";
      xdg.stateHome = "/home/test-user/.local/state";
      programs.rofi.package = pkgs.rofi;
      wayland.windowManager.sway.config.keybindings = effective;
    };
  };
  registry = home.xdg.configFile."lumen-desktop/shortcuts.json".source;
in if !bundle then registry else pkgs.writeText "lumen-shortcut-fixture.json" (builtins.toJSON {
  inherit registry;
  editor = home.xdg.configFile."lumen-desktop/shortcut-editor.json".source;
  runtimeCommand = (lib.findFirst (item: lib.hasInfix "/bin/lumen-shortcut-runtime-witness " item.command) { command = ""; } home.wayland.windowManager.sway.config.startup).command;
  dispatch = lib.filterAttrs (_: command: command != null) effective;
})
