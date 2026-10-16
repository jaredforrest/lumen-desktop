{ pkgs, nixSettings, stateDefaults ? import ../config/profile-state-fields.nix, cacheDefaults ? import ../config/profile-cache-fields.nix }:
let
  stateFields = pkgs.writeText "profile-state-fields.json" (builtins.toJSON stateDefaults);
  cacheFields = pkgs.writeText "profile-cache-fields.json" (builtins.toJSON cacheDefaults);
in pkgs.writeShellApplication {
  name = "lumen-native-profile";
  runtimeInputs = [ pkgs.python3 pkgs.zenity nixSettings ];
  text = ''
    export LUMEN_PROFILE_CACHE_FIELDS=${cacheFields}
    export LUMEN_PROFILE_ACTIVE=/etc/nix/desktop-settings.json
    export LUMEN_PROFILE_STATE_FIELDS=${stateFields}
    exec python3 ${../res/lumen-desktop/profiles/profile_bridge.py} "$@"
  '';
}
