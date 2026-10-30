{ pkgs }:
(import ./pointer_driver.nix { inherit pkgs; }).overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ [ ./projection-pointer-ack.patch ];
})
