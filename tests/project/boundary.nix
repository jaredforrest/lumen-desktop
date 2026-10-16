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
in
assert c.boot.kernel.sysctl."vm.swappiness" == 73;
assert !c.hardware.i2c.enable;
assert !c.programs.hyprland.enable;
assert !c.services.greetd.enable;
assert !c.services.blueman.enable;
assert !c.services.gnome.gnome-online-accounts.enable;
assert !c.services.gnome.evolution-data-server.enable;
assert !(c.environment.etc ? "polkit-1/actions/com.bitwarden.Bitwarden.policy");
true
