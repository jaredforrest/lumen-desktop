{ pkgs }:
# Scoped runtime fixes; the system Qt and compiler packages remain unchanged.
pkgs.quickshell.overrideAttrs (old: {
  # These protocol modules are unused; the session talks to its compositor broker.
  cmakeFlags = (old.cmakeFlags or [ ]) ++ [
    "-DHYPRLAND=OFF"
    "-DI3=OFF"
    "-DSCREENCOPY_HYPRLAND_TOPLEVEL=OFF"
  ];
  patches = (old.patches or [ ]) ++ [ ./quickshell-accessibility-lifecycle.patch ./quickshell-compiler-corrections.patch ];
})
