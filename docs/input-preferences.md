# Per-device input preferences

Settings → Input Devices lists the compositor's keyboard, mouse and trackpad
identities. It changes only explicitly selected device identifiers. It never sends
`*` or `type:` commands. Untouched devices retain their host configuration.

The adapter uses [Sway input IPC reporting](https://github.com/swaywm/sway/blob/1652c54b73f67df17b7b4ab0b0f7048204aa8104/sway/ipc-json.c) to discover reported values. Supported controls are:

- Keyboard repeat rate and repeat delay.
- Pointer/trackpad tracking speed, scroll speed, natural scrolling and button roles.
- Trackpad tap-to-click and disable-while-typing where reported.

Missing or unrecognised capability values produce disabled controls. Virtual
pointers commonly expose scroll speed without libinput tracking or button-role
capabilities. The inspector does not assume those capabilities exist.

## Identity and persistence

The `inputs` Nix section holds a device identity, explicit values and previous
values for reversal. The identity is the compositor identifier, reported name and
type. This is not a hardware serial identity. Two connected devices sharing the
same identifier are read-only because the compositor command would affect both.
Sequentially connected indistinguishable devices can share a profile; the provider
cannot establish a unique physical identity for them.

Edits recheck the connected identity, validate the capability and value, apply
only that option, verify the compositor response and persist through the existing
Nix settings adapter. The option is reapplied and verified after configuration
activation, which may reload input defaults. Failure rolls back the previous value
and, where necessary, the saved profile; any unsuccessful rollback is reported.
No secret device data is stored. A user-session watcher restores explicit saved
values on login and input-added events. It never writes defaults for devices with
no profile. It does not continuously fight external runtime policy changes.

Restore previous value returns the first recorded value for that option and
removes its override. This is the previous observed value, not a promise about
current host defaults. Forget profile removes future restoration and deliberately
leaves current live values unchanged. Disconnected profiles can be forgotten.

```nix
let identity = { identifier = "2:2:Example_Mouse"; name = "Example Mouse"; type = "pointer"; };
in {
inputs.${builtins.hashString "sha256" (builtins.toJSON identity)} = {
  inherit identity;
  values = { natural_scroll = true; };
  baseline = { natural_scroll = false; };
};
}
```

Prefer the UI, which derives stable profile keys and records real previous values.
Profile keys must match the canonical identity hash. Values and identities are typed; wildcard selectors, unknown options, invalid
ranges and boolean/numeric confusion are rejected.

## Boundary and limits

The public desktop owns selected user preferences and their reversible adapter.
The consumer owns drivers, seats, security policy, keyboard layouts, modifier maps
and default device policy. Gesture bindings currently have global compositor
scope; this pane does not invent per-device gestures or expose a device-disable
switch. Acceleration-profile choices are not inferred from a current profile.

This implements a bounded part of per-device input settings. It does not certify
all input hardware, remote modifier delivery, touchpad momentum or full gesture
parity. Unsupported identifiers remain inspectable rather than unsafe selectors.

## Acceptance

```sh
nix flake check --print-build-logs --option abort-on-warn true
nix develop --command desktop-test-python tests/desktop/test_input_settings.py
nix build .#shell --out-link /tmp/lumen-input-shell
LUMEN_TEST_SHELL_LAUNCHER=/tmp/lumen-input-shell/bin/lumen-desktop-shell nix develop --command desktop-test-python tests/desktop/test_input_settings_qml.py
nix build .#compositor-basic --out-link /tmp/lumen-input-compositor
nix build .#test-pointer --out-link /tmp/lumen-input-pointer
nix develop --command desktop-test-python tests/desktop/input_settings_native.py --launcher /tmp/lumen-input-shell/bin/lumen-desktop-shell --sway /tmp/lumen-input-compositor/bin/sway --pointer-tool /tmp/lumen-input-pointer/bin/wlrctl --output /tmp/lumen-input-evidence
```

Protocol tests use a real private Unix socket with the Sway IPC envelope and
synthetic device capabilities. They cover exact scope, mouse/trackpad separation,
reversal, typed requests, provider refusal, persistence rollback, activation
reset, reconnect, missing capabilities and duplicate identities. Actual QML uses
its real helper transport. Native testing changes and restores the repeat rate of
a disposable virtual keyboard in an isolated compositor, verifies other virtual
devices remain unchanged and asserts painted interiors. A separate Nix evaluator
test checks authoritative schema rejection. No host input settings are changed.
