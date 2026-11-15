{ pkgs }:
# Upstream new-device email verification support, pinned for rbw 1.15.0.
# https://github.com/doy/rbw/pull/319 (2485ba6)
# Keeps verification enabled and prompts for the code sent by Bitwarden.
pkgs.rbw.overrideAttrs (old: {
  patches = (old.patches or []) ++ [ ../res/patches/rbw-new-device-verification.patch ];
})
