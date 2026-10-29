{ pkgs, nixSettings, stateDefaults ? import ../config/profile-state-fields.nix, cacheDefaults ? import ../config/profile-cache-fields.nix }:
let
  stateFields = pkgs.writeText "profile-state-fields.json" (builtins.toJSON stateDefaults);
  cacheFields = pkgs.writeText "profile-cache-fields.json" (builtins.toJSON cacheDefaults);
  runtime = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-profile-tools";
    version = "0.1.0";
    src = ../res/lumen-desktop/profiles;
    cargoLock.lockFile = ../res/lumen-desktop/profiles/Cargo.lock;
  };
in pkgs.writeTextFile {
  name = "lumen-native-profile";
  destination = "/bin/lumen-native-profile";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.zenity nixSettings ])} | split row ':') ++ $env.PATH)
      $env.LUMEN_PROFILE_CACHE_FIELDS = ($env.LUMEN_PROFILE_CACHE_FIELDS? | default "${cacheFields}")
      $env.LUMEN_PROFILE_ACTIVE = ($env.LUMEN_PROFILE_ACTIVE? | default "/etc/nix/desktop-settings.json")
      $env.LUMEN_PROFILE_STATE_FIELDS = ($env.LUMEN_PROFILE_STATE_FIELDS? | default "${stateFields}")
      exec ${runtime}/bin/lumen-native-profile ...$args
    }
  '';
}
