{ pkgs, qt6 ? true }:
# Refresh the palette of already-open Qt applications when GTK's theme changes.
# Kept separate so the adapter can be validated before session-wide activation.
(if qt6 then pkgs.qgnomeplatform-qt6 else pkgs.qgnomeplatform).overrideAttrs (old: {
  patches = (old.patches or []) ++ [ ./patches/qgnomeplatform-live-palette.patch ];
})
