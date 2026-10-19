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
  defaultHome = c.home-manager.users.test-user { config = {}; lib = nixpkgs.lib; };
  optedInHome = optedIn.home-manager.users.test-user { config = {}; lib = nixpkgs.lib; };
in
assert c.boot.kernel.sysctl."vm.swappiness" == 73;
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
