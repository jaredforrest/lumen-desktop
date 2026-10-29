# Wallpaper preferences

Settings → Wallpaper supports one default image and optional exact-display
image overrides. Choose a local still PNG or JPEG up to 64 million pixels;
files stay in place and are never fetched, copied into the repository or moved.
The renderer creates disposable private runtime symlinks with safe filenames;
selected filename text never enters the compositor’s path expansion.
The owned chooser browses local folders, supports keyboard selection and accepts
an absolute local image path. Cancel makes no settings change.

The typed `wallpaper` section contains `defaultImage` (absolute path or null)
and `displays` (map from SHA-256 display identity to `{identity,image}`).
Overrides match the authoritative display identity contract: known make/model/
serial, otherwise connector name. Connector fallback cannot distinguish replacement
hardware on the same connector; identical reported identities cannot be edited
independently. Images belong to displays, not logical desktops. Desktop-specific
wallpapers are currently unavailable and the UI states that limit.

The installed consumer asset remains the fallback. `appearance.wallpaper` still
accepts a consumer-owned path; private assets never enter the public repository.
Null default uses that installed asset; removing a display override uses the
current default. Disconnected overrides stay saved. Unreadable, deleted, invalid
or unsupported selected images fall back without erasing their saved reference.
The session watcher restores selections on login and polls output/image state
every two seconds, including reconnect and scale changes. Fill crops to the
output's aspect ratio. Modes, scales, placement, timezone, lock and host policies
are unchanged. The Dock samples the corresponding selected image where its
wallpaper material is enabled.

The provider uses Sway's documented `output <name> bg <file> fill` command:
[upstream output manual](https://github.com/swaywm/sway/blob/master/sway/sway-output.5.scd).
File readability is checked at runtime, never required during Nix evaluation.
Decoded-image metadata is cached privately against inode, size and precise change
and modification times; repeated status reads do not decode unchanged images.
A file may become unavailable after validation; the watcher then restores fallback.
Successful persistence followed by a provider error is reported rather than
silently claiming the background changed. Reopen/refresh to inspect actual status.

## Reproduction

```
nix flake check --print-build-logs --option abort-on-warn true
nix build .#shell .#compositor-basic .#test-pointer .#test-wallpaper-settings-fixture
nix develop --command "$(nix build .#test-wallpaper-settings-fixture --no-link --print-out-paths)/bin/lumen-wallpaper-settings-fixture" \
  --launcher "$(nix build .#shell --no-link --print-out-paths)/bin/lumen-desktop-shell" \
  --sway "$(nix build .#compositor-basic --no-link --print-out-paths)/bin/sway" \
  --pointer-tool "$(nix build .#test-pointer --no-link --print-out-paths)/bin/wlrctl" \
  --output /tmp/wallpaper-acceptance
```

The native fixture isolates HOME, state, the session bus and compositor. It opens
the actual owned chooser, paints synthetic images on two headless outputs with
fractional scale, starts a fresh provider after removing an image, and disconnects/
reconnects the second output. Pixel assertions prove the backgrounds painted.
Unit acceptance covers renamed stable displays, persistence failure, unsupported
image contents, and no changes to personal image bytes. The real Nix evaluator
checks closed typed values and display identity hashes separately.
