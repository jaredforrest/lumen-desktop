# Saved portal decisions

Privacy & Security includes a provider-backed inspector for four known scopes:

| Scope | Provider resource | Actions |
| --- | --- | --- |
| Camera | `devices` / `camera` | Deny future portal requests; remove decision to ask again |
| Screenshots | `screenshot` / `screenshot` | Deny future portal requests; remove decision to ask again |
| Saved screen sharing | `screencast` / individual saved session | Forget that app's saved session |
| Saved remote desktop | `remote-desktop` / individual saved session | Forget that app's saved session |

The model uses the [PermissionStore interface](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.PermissionStore.html), not direct database editing. Enumeration uses Lookup/List; denial uses SetPermission with `create=false`; deletion uses the version-2 DeletePermission method. Mutations target one app/resource entry and preserve resource metadata and sibling decisions. Supported table/resource names come from [provider constants](https://github.com/flatpak/xdg-desktop-portal/blob/8eba842f30070e2d90e16832462ad8171bd5beed/shared/xdp-types.h).

Every action requires a separate confirmation showing the exact app ID, capability, resource fingerprint and current decision. The confirmation scrolls into view and focuses Cancel. Escape cancels. Friendly app names supplement provider IDs. An absent app ID or unknown decision format produces a read-only entry. Raw restore tokens and opaque resource metadata never enter the presentation JSON or command arguments; saved sessions use a stable hashed fingerprint.

The helper pins the provider's unique bus owner, re-reads decisions before mutation, rejects a changed confirmation revision, and verifies the result afterwards. A provider restart invalidates old confirmations. Errors clear stale rows and never produce a success notice. The interface has no compare-and-swap transaction: another writer can still race the final lookup and mutation. Polling refreshes an open pane every four seconds, and user actions refresh immediately. Partial scope failures remain explicit.

## Limits and consumer boundary

These are **saved portal decisions**, not an active-capture monitor or a global app sandbox. No microphone controls are invented. Native applications and other permission systems can bypass these scopes. The host owns portal installation, access policy, sandbox rules and camera enforcement. The desktop does not change them or require root.

Forgetting a saved sharing entry does not establish that an active stream stopped. The [provider persistence implementation](https://github.com/flatpak/xdg-desktop-portal/blob/8eba842f30070e2d90e16832462ad8171bd5beed/desktop-portal/xdp-session-persistence.c) consumes restore entries during restoration and may save permissions again on session close. Active sessions and transient grants may therefore be absent. The UI consistently tells users to stop existing capture in its app. Enforced termination and client-attributed active-use indicators require additional provider evidence and are not certified by this inspector.

No consumer configuration change is required for the pane. If a permission store is unavailable or policy denies inspection, it reports that state. This work does not install or enable a permission provider.

## Reproduction

```sh
nix flake check --print-build-logs --option abort-on-warn true
nix build .#shell --out-link /tmp/lumen-permission-shell
nix develop --command desktop-test-python tests/desktop/test_portal_permissions.py
LUMEN_TEST_SHELL_LAUNCHER=/tmp/lumen-permission-shell/bin/lumen-desktop-shell nix develop --command desktop-test-python tests/desktop/test_portal_permissions_qml.py
nix build .#compositor-basic --out-link /tmp/lumen-permission-compositor
nix build .#test-pointer --out-link /tmp/lumen-permission-pointer
nix develop --command desktop-test-python tests/desktop/portal_permissions_input.py --launcher /tmp/lumen-permission-shell/bin/lumen-desktop-shell --sway /tmp/lumen-permission-compositor/bin/sway --pointer-tool /tmp/lumen-permission-pointer/bin/wlrctl --output /tmp/lumen-permission-evidence
```

Tests use a private bus with activation disabled and a disposable implementation of the real D-Bus signatures. They cover denial/reset/forget, sibling isolation, explicit confirmation/cancel, changed decisions and service-owner replacement, version limits, refusal to apply a change, partial and missing providers, Changed signals, and secret redaction. Actual QML uses its real helper transport. The native fixture clicks confirmation against that provider and checks painted light/dark interiors. No live grants, credentials or account data are used.
