{ nixpkgs, system }:
let
  evaluated = nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [ ../../modules/desktop.nix ({ lib, pkgs, ... }: {
      options.home-manager.users = lib.mkOption { type = lib.types.attrsOf lib.types.raw; default = {}; };
      config = {
        system.stateVersion = "26.05";
        services.lumen-desktop = { enable = true; user = "test-user"; settingsPackage = pkgs.hello; };
        boot.kernel.sysctl."vm.swappiness" = 73;
      };
    }) ];
  };
  c = evaluated.config;
  optedIn = (evaluated.extendModules {
    modules = [ { services.lumen-desktop.integrations.authorization.enable = true; } ];
  }).config;
  sharingConfigured = (evaluated.extendModules {
    modules = [ ({ pkgs, ... }: {
      services.lumen-desktop.integrations.nearbySharing = {
        providerPackage = pkgs.hello;
        label = "Fixture Sharing";
      };
    }) ];
  }).config;
  recoveryDisabled = (evaluated.extendModules { modules = [ {
    services.lumen-desktop.integrations.shellRecovery.enableFallback = false;
  } ]; }).config;
  recoveryDisabledHome = recoveryDisabled.home-manager.users.test-user { config = {}; lib = nixpkgs.lib; };
  destinationConfigured = (evaluated.extendModules {
    modules = [ ({ pkgs, ... }: {
      services.lumen-desktop.integrations.sharing.providers = [ {
        id = "fixture"; label = "Fixture destination"; providerPackage = pkgs.hello;
      } ];
    }) ];
  }).config;
  readerBinding = { "Mod4+Control+F8" = "where-am-i"; };
  readerDisabled = (evaluated.extendModules { modules = [ {
    services.lumen-desktop.shortcuts.readerBindings = readerBinding;
  } ]; }).config;
  readerEnabled = (evaluated.extendModules { modules = [ {
    services.lumen-desktop.integrations.readerCommands.enable = true;
    services.lumen-desktop.shortcuts.readerBindings = readerBinding;
  } ]; }).config;
  readerCollision = (evaluated.extendModules { modules = [ {
    services.lumen-desktop.integrations.readerCommands.enable = true;
    services.lumen-desktop.shortcuts.readerBindings."Mod4+Tab" = "where-am-i";
  } ]; }).config;
  readerHome = settings: settings.home-manager.users.test-user {
    config = { home.username = "test-user"; home.homeDirectory = "/home/test-user"; };
    lib = nixpkgs.lib;
  };
  defaultHome = c.home-manager.users.test-user { config = {}; lib = nixpkgs.lib; };
  optedInHome = optedIn.home-manager.users.test-user { config = {}; lib = nixpkgs.lib; };
in
assert (defaultHome.wayland.windowManager.sway.config.input or {}) == {};
assert defaultHome.systemd.user.services.lumen-desktop-shell.Unit.StartLimitIntervalSec == 60;
assert defaultHome.systemd.user.services.lumen-desktop-shell.Unit.StartLimitBurst == 3;
assert defaultHome.systemd.user.services.lumen-desktop-shell.Unit.OnFailure == [ "lumen-desktop-recovery.service" ];
assert defaultHome.systemd.user.services.lumen-desktop-shell.Service.Restart == "on-failure";
assert c.services.lumen-desktop.integrations.shellRecovery.enableFallback;
assert recoveryDisabledHome.systemd.user.services.lumen-desktop-shell.Unit.OnFailure == [];
assert recoveryDisabledHome.systemd.user.services.lumen-desktop-shell.Unit.StartLimitBurst == 3;
assert c.services.lumen-desktop.integrations.shellRecovery.fallbackPackage == null;
assert c.boot.kernel.sysctl."vm.swappiness" == 73;
assert !c.services.lumen-desktop.integrations.readerCommands.enable;
assert !((readerHome readerDisabled).wayland.windowManager.sway.config.keybindings.content ? "Mod4+Control+F8");
assert nixpkgs.lib.hasSuffix " where-am-i" (readerHome readerEnabled).wayland.windowManager.sway.config.keybindings.content."Mod4+Control+F8";
assert builtins.all (entry: entry.assertion) (readerHome readerEnabled).assertions;
assert !(builtins.head (readerHome readerCollision).assertions).assertion;
assert c.services.lumen-desktop.integrations.nearbySharing.providerPackage == null;
assert c.services.lumen-desktop.integrations.nearbySharing.label == "Nearby Sharing";
assert sharingConfigured.services.lumen-desktop.integrations.nearbySharing.label == "Fixture Sharing";
assert nixpkgs.lib.getName sharingConfigured.services.lumen-desktop.integrations.nearbySharing.providerPackage == "hello";
assert sharingConfigured.services.gvfs.enable == c.services.gvfs.enable;
assert sharingConfigured.services.greetd.enable == c.services.greetd.enable;
assert sharingConfigured.services.blueman.enable == c.services.blueman.enable;
assert c.services.lumen-desktop.integrations.sharing.providers == [];
assert (builtins.head destinationConfigured.services.lumen-desktop.integrations.sharing.providers).id == "fixture";
assert destinationConfigured.services.gvfs.enable == c.services.gvfs.enable;
assert destinationConfigured.services.greetd.enable == c.services.greetd.enable;
assert destinationConfigured.services.blueman.enable == c.services.blueman.enable;
assert destinationConfigured.networking.firewall.allowedTCPPorts == c.networking.firewall.allowedTCPPorts;
assert !c.hardware.i2c.enable;
assert !c.programs.hyprland.enable;
assert !c.services.greetd.enable;
assert !c.services.lumen-desktop.integrations.authorization.enable;
assert defaultHome.wayland.windowManager.sway.config.startup == [];
assert builtins.length optedInHome.wayland.windowManager.sway.config.startup == 1;
assert optedIn.security.polkit.enable == c.security.polkit.enable;
assert optedIn.security.polkit.extraConfig == c.security.polkit.extraConfig;
assert !c.services.gvfs.enable;
assert !c.services.lumen-desktop.integrations.networkLocations.enable;
assert !c.services.blueman.enable;
assert !c.services.gnome.gnome-online-accounts.enable;
assert !c.services.gnome.evolution-data-server.enable;
assert !(c.environment.etc ? "polkit-1/actions/com.bitwarden.Bitwarden.policy");
true
