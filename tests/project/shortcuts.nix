# Exercise the actual merged-binding export without starting a desktop session.
{ nixpkgs, system, extraBindings ? {}, applicationBindings ? [], reservedBindings ? [] }:
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
          shortcuts = { inherit applicationBindings reservedBindings; };
        };
      };
    }) ];
  };
  pkgs = evaluated.pkgs;
  unwrap = value: if builtins.isAttrs value && value ? _type && value._type == "override" then unwrap value.content else value;
  home = evaluated.config.home-manager.users.test-user {
    inherit lib;
    config = {
      home.username = "test-user";
      home.homeDirectory = "/home/test-user";
      xdg.stateHome = "/home/test-user/.local/state";
      programs.rofi.package = pkgs.rofi;
      wayland.windowManager.sway.config.keybindings = (lib.mapAttrs (_: unwrap) (unwrap home.wayland.windowManager.sway.config.keybindings)) // extraBindings;
    };
  };
in home.xdg.configFile."lumen-desktop/shortcuts.json".source
