{ pkgs }:
# Persistent virtual pointer for isolated input tests. Commands arrive on stdin.
pkgs.wlrctl.overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ [ ./pointer-driver.patch ];
})
