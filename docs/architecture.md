# Architecture

One repository currently contains several components with a shared release boundary:

| Area | Ownership |
| --- | --- |
| `res/lumen-desktop/shell` | QML interface, models and service clients |
| `res/lumen-desktop/notifications` | Notification daemon and history |
| `res/lumen-desktop/auth` | Greeter and lock interface |
| `res/lumen-desktop/global-menu` | Menu bridge and protocol adapters |
| `res/character-viewer` | Character selection and input service |
| `packages` | Reproducible runtime factories, compositor/toolkit patches, app adapters |
| `modules` | Optional NixOS/Home Manager integration |
| `config` | Generic preference and working-state schemas |
| `tests/desktop` | Backend, protocol and isolated rendering tests |
| `tests/project` | Publication and dependency-boundary checks |

The shell is not the compositor. Sway/SwayFX remains responsible for windows, input routing and output management; the shell is a Wayland client. Authentication remains with greetd/PAM and the compositor's session-lock protocol. Desktop modules must not acquire an unauthenticated remote control interface.

Host settings enter through `LUMEN_NIX_SETTINGS_*` and the `lumen-nix-settings` command. The JSON schema is exported as `lib.settingsSchema`. Host adapters own privilege, Git provenance and durable deployment. Runtime factories accept the adapter explicitly, so they do not import a consumer checkout.

Keep components together while they share interfaces and integration tests. Split a component into another repository only when it has a stable API, independent consumers and its own release/test lifecycle.
