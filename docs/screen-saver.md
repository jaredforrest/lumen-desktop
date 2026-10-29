# Decorative screen saver

Settings → Screen Saver configures an optional decorative idle display. It is
**off by default**. Preview works independently of automatic activation. Any
supported key, pointer movement, click or wheel activity dismisses the decoration.
The dismissing key or click is consumed by the overlay; subsequent input returns
to the underlying application. System shortcuts remain compositor-owned.

This is not a secure locker, authentication interface or privacy guarantee.
Existing host lock, suspend, display-off and password policy remain authoritative.
No saver preference changes those policies, starts a locker, inhibits locking or
unlocks a session. A real secure lock takes precedence in the compositor. The
saver is hidden when native lock authority reports locked or becomes unknown;
preview and automatic activation require known unlocked-session status. The
saver does not claim that an unlocked session requires no authentication in every
host environment; authentication remains the real locker’s responsibility.

The typed `screenSaver` section contains `enabled` (boolean, default false),
`idleSeconds` (integer 15–7200, default 300), and `motion` (boolean, default true).
Turning automatic activation off is reversible and does not prevent preview.
Animation also pauses with the shared Reduce Motion preference. Shapes are
rendered locally without artwork downloads or changes to private wallpapers.
The overlay follows connected outputs and does not change their modes/scales.

Automatic activation uses the compositor's `ext-idle-notify-v1` through
[Quickshell IdleMonitor](https://quickshell.org/docs/v0.3.0/types/Quickshell.Wayland/IdleMonitor/),
with `respectInhibitors: true`. No replacement idle manager is installed. Supported
media/presentation idle inhibitors postpone the saver; they do not change lock
policy. Native lock state comes from the existing authenticated compositor IPC
provider. Unsupported or unavailable native lock status prevents decoration
rather than guessing from window names, process names or a stored setting.

## Acceptance

```
nix flake check --print-build-logs --option abort-on-warn true
nix build .#test-screen-saver-fixture -o artifacts/screen-saver-fixture
nix develop --command artifacts/screen-saver-fixture/bin/lumen-screen-saver-fixture \
  --launcher "$(nix build .#shell --no-link --print-out-paths)/bin/lumen-desktop-shell" \
  --sway "$(nix build .#compositor-basic --no-link --print-out-paths)/bin/sway" \
  --pointer-tool "$(nix build .#test-pointer --no-link --print-out-paths)/bin/wlrctl" \
  --lock "$(nix build .#test-session-lock --no-link --print-out-paths)/bin/lumen-test-session-lock" \
  --output /tmp/screen-saver-acceptance
```

The fixture uses private HOME, state, bus and a genuine nested compositor. It
checks default-off preview, painted interior, consumed dismissal input and restored
application focus, real idle inhibition/activation, and real session-lock acquisition
and protocol-authorized test unlock. A saver action cannot release that lock.
The disposable lock driver does not handle passwords or test the user's PAM policy.
CI repeats native acceptance on both compositor variants. Mapping an overlay now
rebases the pointer after its scene maps, so a stationary first click is routed to
the decoration rather than a previously focused application.
Unit tests check closed reversible settings, default-off/unknown-lock behavior and
that no lock/power/inhibitor override exists. Real Nix evaluator acceptance checks
the same schema separately.
