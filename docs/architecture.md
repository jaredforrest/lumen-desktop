# Architecture

One repository currently contains several components with a shared release boundary:

| Area | Ownership |
| --- | --- |
| `res/lumen-desktop/shell` | QML interface, models and service clients |
| `res/lumen-desktop/notifications` | Notification daemon and history |
| `res/lumen-desktop/auth` | Greeter and lock interface |
| `res/lumen-desktop/global-menu` | Menu bridge and protocol adapters |
| `res/utilities` | Screen capture, recording and shortcut dispatch |
| `res/character-viewer` | Character selection and input service |
| `packages` | Reproducible runtime factories, compositor/toolkit patches, app adapters |
| `modules` | Optional NixOS/Home Manager integration |
| `config` | Generic preference and working-state schemas |
| `tests/desktop` | Backend, protocol and isolated rendering tests |
| `tests/project` | Publication and dependency-boundary checks |

The shell is not the compositor. Sway/SwayFX remains responsible for windows, input routing and output management; the shell is a Wayland client. Authentication remains with greetd/PAM and the compositor's session-lock protocol. Desktop modules must not acquire an unauthenticated remote control interface.

Host settings enter through `LUMEN_NIX_SETTINGS_*` and the `lumen-nix-settings` command. The JSON schema is exported as `lib.settingsSchema`. Host adapters own privilege, Git provenance and durable deployment. Runtime factories accept the adapter explicitly, so they do not import a consumer checkout.

Keep components together while they share interfaces and integration tests. Split a component into another repository only when it has a stable API, independent consumers and its own release/test lifecycle.

## Consumer boundary

Lumen owns desktop mechanisms: shell widgets, window actions, protocol clients,
rendering, application launch contracts and session integration. The NixOS module
provides overridable desktop defaults. Enabling it must not tune the host.

The consumer owns kernel and swap policy, storage and persistence, hardware and
GPU workarounds, accounts and secrets, app privilege policies, compositor selection,
keyboard/monitor preferences, idle timeouts, clipboard retention and user folders.
Optional account, Bluetooth, accessibility, greeter and alternate-session
integrations require explicit opt-in. Enabling an integration does not grant it
ownership of host policy.

Personal or licensed artwork and fonts belong in the consumer repository. Public
assets remain redistributable defaults. Tests evaluate the module with a deliberately
different host policy and verify it is preserved and optional services stay off.

## Decoration and icon contracts

Every window has one decoration owner. Sway draws the title bar for clients using
server decorations. Clients with their own header (including portal file dialogs)
retain it; do not force `border normal` onto every client. Desktop-owned QML
windows integrate their controls into their toolbar and opt out of an outer title
bar. Toolkit button placement, typography and colour share desktop defaults.
Content/header geometry must be inspected as well as counting window controls.

Calendar icons are live templates: `reference-icons/calendar.svg` and its dark
variant contain only a blank background. AppIcon draws the weekday and day once.
Consumer overrides must follow that contract rather than embed another calendar
or a pre-rendered date.
