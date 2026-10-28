{ pkgs }:
# Test-only virtual pointer: press, move over a folder, wait, release without
# another motion event. Run only against the isolated installed-shell smoke compositor.
pkgs.wlrctl.overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ [ ./drag-pointer.patch ];
})
