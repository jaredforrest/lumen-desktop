# NixOS integration

Add this repository as a locked flake input. For a private or local development checkout use a local input override; commit the lock only after validation. Normal deployments consume an immutable public revision.

Import `inputs.lumen-desktop.nixosModules.default` and configure:

```nix
services.lumen-desktop = {
  enable = true;
  user = "desktop-user";
  settingsPackage = hostSettingsAdapter;
};
```

This module expects existing Home Manager and Sway configuration. It does not partition disks, declare persistence or manage secrets. Keep those policies in the host repository. The host must preserve the runtime state directories it wants retained across reboots.

The default shell package is suitable for package/build development. A host-specific session can be constructed through `lib.mkSession` with explicit `pkgs`, `settingsPackage`, `user`, `homeDirectory`, `timeZone`, `swayPackage` and `rofiPackage`. Its returned attributes include the shell launcher and service commands.

A host should test a new pinned revision before activation, retain rollback generations and avoid restarting its active compositor or greeter during remote work.

## Personal appearance

Set `services.lumen-desktop.appearance.icons` to a directory of icon overrides,
`brandMark` to an SVG, `wallpaper` to an image and `emojiFamily` to the installed
font's real family name. The icon directory overlays `shell/reference-icons` by
filename; unmentioned icons retain the public defaults. Assets are copied into an
immutable derivation shared by the shell, launcher, greeter and lock screen.
The consumer installs/licences fonts and controls fontconfig. The character-viewer
package separately accepts `fontPackage`, `fontFile` and `fontFamily`.

Account services, Bluetooth UI, accessibility bus, and greeter integration
are disabled by default under `services.lumen-desktop.integrations`. Set the
corresponding `enable` flag explicitly. `replaceExistingShell` explicitly opts in
to disabling competing bar/notification services.

The supported session uses Sway/SwayFX as compositor and Lumen as shell and notification provider. The basic compositor package is retained for isolated headless tests; it is not a second installed desktop session.
