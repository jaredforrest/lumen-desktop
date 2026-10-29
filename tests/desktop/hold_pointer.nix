{ pkgs }:
# Isolated compositor only: keep a pointer capability alive while testing hover.
pkgs.wlrctl.overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ [ ./hold-pointer.patch ];
})
