# Shortcut declarations and collision checks

The desktop exports its **effective merged default-mode Sway keybindings** to
`$XDG_CONFIG_HOME/lumen-desktop/shortcuts.json`. Building that file runs the typed
registry validator. A collision fails the build before activation; there is no
last-definition-wins policy between equivalent physical chords.

The export includes consumer keybindings and preserves consumer overrides. It
contains action identities, physical chords, owners and diagnostics, **not shell
commands**. Action identity is resolved from exact configured desktop dispatch commands, so
moving one to a new chord preserves its identity. An unrelated consumer command
is labelled `consumer.binding` and cannot inherit the old chord's action. The public defaults metadata
is in `res/lumen-desktop/runtime/shortcut-defaults.json`.

Modifier order and aliases such as `Super`/`Mod4`, `Ctrl`/`Control` and `Alt`/`Mod1`
are normalised. Press and release bindings are distinct. Different explicit
compositor modes or different exact devices can be disjoint; a wildcard device
still overlaps either device. `--locked` and `--inhibited` extend reach and do not
make a competing unlocked binding safe. Duplicate active chords are rejected even
when both declarations name the same action.

Selected application adapters can declare native shortcuts without rewriting
application input:

```nix
services.lumen-desktop.shortcuts.applicationBindings = [{
  id = "editor.save";
  action = "document.save";
  chord = "Control+s";
  appId = "example.editor";
}];
```

Two exact application identities can use the same chord. A global binding
conflicts with either application declaration. Application wildcards are rejected
because they cannot establish disjoint scopes. Disabled declarations are retained
for diagnostics but do not capture input.

Consumers may also declare `shortcuts.reservedBindings` with `id`, `action` and
`chord`. This reserves an otherwise unused global chord, for example against a
selected adapter. A reservation that overlaps an effective binding also fails;
reservations do not silently override a working security binding. Security policy
and the choice of lock shortcut remain consumer-owned.

For standalone validation:

```sh
python3 res/lumen-desktop/runtime/shortcuts.py declarations.json --output effective.json
```

The input is `{ "version": 1, "bindings": [...] }`. Bindings require `id`,
`action` and `chord`; optional fields are `scope`, `appId`, `mode`, `device`,
`trigger`, `enabled`, `owner` and `fallback`. Unknown fields, invalid booleans,
unsupported options and ambiguous declarations fail validation. The command exits
with 1 for collisions and 2 for an invalid registry.

## Effective hints

The same validated export contains action-scoped `hints`, generated only for
enabled default-mode, wildcard-device press bindings. Release edges, reservations,
device-specific declarations and unrelated consumer commands are not advertised
as general shortcuts. Multiple aliases are retained in the model and settings
listing; the shortest physical chord is selected for a compact menu hint. The shell loads the effective file through `ShortcutState`, clears
hints on missing/malformed input and watches atomic replacements. It does not
silently fall back to bundled defaults when a consumer disables a binding.

Owned Window menu fullscreen/fill and desktop-reveal commands display their
configured chord alongside the label and in the accessible description. Hints
are presentation, never permission to dispatch a command. Exported application
menus retain their provider-owned shortcut data; a declared application adapter
can query the model with an exact application identity without intercepting input.

The module-derived fixture checks moving the fullscreen command, disabling it and
replacing it with an unrelated command. Real native acceptance then installs that
exact generated configuration in a private compositor, checks the old key is
inert, the new key toggles the captured native window, and the owned menu renders
the new hint. Actual Qt file reload tests cover malformed and deleted exports.

## Current boundary

This is a build-time default-mode collision registry and diagnostic export, not a
shortcut editor or an input broker. Raw `extraConfig` bindings, separately declared
Sway modes, physical keycodes and runtime changes are not yet imported. Application
conflicts are known only when an adapter declares them. Layout-dependent symbol discovery, remote key-delivery diagnostics and user rebind
UI are not implemented by this registry. Those acceptance clauses remain open.
