{ pkgs }:
# Only for an isolated compositor. Allows Qt to bind the new pointer before clicks.
pkgs.wlrctl.overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ [ ./click-pointer.patch ];
})
