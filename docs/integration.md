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

This module expects existing Home Manager, Sway and Waybar configuration. It does not partition disks, declare persistence or manage secrets. Keep those policies in the host repository. The host must preserve the runtime state directories it wants retained across reboots.

The default shell package is suitable for package/build development. A host-specific session can be constructed through `lib.mkSession` with explicit `pkgs`, `settingsPackage`, `user`, `homeDirectory`, `timeZone`, `swayPackage` and `rofiPackage`. Its returned attributes include the shell launcher and service commands.

A host should test a new pinned revision before activation, retain rollback generations and avoid restarting its active compositor or greeter during remote work.
