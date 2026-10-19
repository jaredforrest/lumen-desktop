# Media control transport

The owned media panel consumes Quickshell's MPRIS service model. It selects an explicitly chosen player, then a playing player, then the first available player; the relay service is excluded. Expanded controls expose source selection, playback position and player volume only according to the provider's advertised capabilities. Player volume is independent of system output volume, mute and output-device policy.

A physical playback press captures its player object and intended operation. Release checks that the exact object is still the selected live player and still supports that operation. Play and Pause remain distinct intents if another client changes playback during the press. A source popup keeps its labels and object identities until it closes, so a disappearing row cannot select a replacement. Position drags retain the player's track identity; volume drags retain the player. Fresh keyboard and assistive actions use the current validated target.

The native fixture exports real private MPRIS services on independent D-Bus connections. It exercises two players, property updates, missing optional volume, capability loss, source replacement while a chooser is open, held actions across source/owner changes, keyboard actions, typed volume/position messages and a private AT-SPI action. It records provider calls, accessibility hierarchy and captures without reading live audio or accounts.

```sh
nix develop --command desktop-test-python tests/desktop/media_control_native.py --launcher /absolute/path/to/lumen-desktop-shell --sway /absolute/path/to/sway --pointer-tool /absolute/path/to/wlrctl --output /absolute/new/evidence-directory
```

Use `--effects` with the effects compositor. This acceptance covers the represented provider lifecycle and capabilities; it does not establish compliance for every external player or claim system output-volume testing.

Provider contracts: [MPRIS Player interface](https://specifications.freedesktop.org/mpris/latest/Player_Interface.html) and [Quickshell MprisPlayer](https://quickshell.org/docs/v0.2.1/types/Quickshell.Services.Mpris/MprisPlayer/). Providers vary in optional capabilities; the panel checks the associated supported/control properties before writes.
