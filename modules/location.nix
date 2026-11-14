# Opt-in host policy: approximate network location, without retaining IP addresses or history.
{ config, lib, pkgs, ... }:
let
  cfg = config.services.lumen-location;
  service = pkgs.callPackage ../packages/location-service.nix {};
in {
  options.services.lumen-location.enable = lib.mkEnableOption "automatic local timezone and solar appearance location";
  config = lib.mkIf cfg.enable {
    time.timeZone = lib.mkForce null;
    services.timesyncd.enable = lib.mkDefault true;
    systemd.packages = [ service ];
    systemd.timers.lumen-location.wantedBy = [ "timers.target" ];
  };
}
