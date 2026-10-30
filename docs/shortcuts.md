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
lumen-shortcuts declarations.json --output effective.json
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

The authoritative action registry and editor remain declarative and default-mode.
A separate read-only native observer imports the currently loaded compositor's
bounded symbol/code binding metadata, including modes, devices, groups and release
flags. It exports no command text and cannot invoke bindings. Opaque native
commands are labelled **Custom shortcut / command identity unverified**; matching
a chord never certifies an overridden command's identity. Application shortcuts
remain unknown unless an adapter declares them. Remote key-delivery diagnostics
remain a separate boundary.

### Current keyboard layouts and native collisions

`lumen-shortcut-layout` reads each seat's actual current keyboard keymap and active
layout through the optional versioned `GET_SEATS` extension. The shared model
updates owned menu keycaps and Settings → Accessibility when that layout changes.
It evaluates both physical keycode and symbol bindings using the pinned
compositor's raw and translated symbol paths. This deliberately preserves its
direct XKB consumed-mask intersection with the eight-bit WLR mask, including
nonidentity modifier-index behavior; it is not a claim about every compositor's
binding algorithm. No depressed keys, text entry, or global input events are read.

A nondefault mode suspends default-mode menu hints. Missing/invalid provider data
also suppresses native hints and displays an unavailable status. Settings displays
current declared keycaps and bounded actual native collision diagnostics,
including code and other-mode entries, without importing arbitrary commands into
the authoritative action registry. Device-specific keymaps and multi-key
sequences are explicitly incomplete rather than certified safe.

Before an owned edit mutates settings or starts its deployment provider, its
validated proposed symbol chord is checked against actual native physical
activations. Overlap, unreachable symbols, unavailable metadata or incomplete
sequence/device analysis refuses the edit and retains the previous map. No old
native chord is excluded as allegedly owned: metadata alone cannot verify an
opaque replacement. This is a sample of current seat layouts, not a promise about
layouts selected later or concurrently with a deployment. Subsequent observations
recalculate diagnostics and hide conflicted hints. New native generations,
including observer loss/hydration, cancel an already pending application-switcher
preview; they never activate its target.

The transport uses a one-second absolute socket deadline, a 2 MiB frame cap,
regular-file bounded registry reads, and owned socket/process/session facts.
These identify an observation session, not authentication against another
same-user process. Provider limits include 16 seats, 512 KiB aggregate keymap
text, 128 KiB per map, eight layouts, keycodes through 1023, 2048 binding records,
eight keys per record and 256 KiB metadata. The client bounds activation memberships,
collision comparisons, cached maps and retained output rows, refusing incomplete
results. It publishes changes only and polls at 500 ms; observed generations do
not imply atomic monitoring of every transient input-map change.

## Owned shortcut editor

Settings → Accessibility contains an owned shortcut editor and the complete
effective inventory. Editing defaults to unavailable. Consumers enable it only
after connecting the authoritative settings section to the module, for example:

```nix
let
  desktopSettings = import "${lumenSource}/packages/desktop-settings-schema.nix" {
    inherit lib;
    values = import ./settings/desktop.nix;
  };
in {
  services.lumen-desktop.shortcuts = {
    editable = true;
    rebindings = desktopSettings.shortcuts.rebindings;
  };
}
```

The existing consumer `settingsPackage` must update that exact source and rebuild
and activate the configuration containing this module. The standalone settings
bundle alone cannot change compositor bindings. An unwired editor cannot report
success: the helper requires a coherent immutable catalog/registry pair and a
matching load witness from the connected compositor. Exporting files alone does
not confirm activation. The witness checks the live configuration's validity and
generation marker, socket ownership and inode, peer process start time and boot
identity. Its private session record is informational, not an authentication
boundary against another process running as the same user. Older compositors
without the read-validity capability remain read-only until a new session starts
with the updated package. An invalid reload cannot confirm a new generation;
the helper does not claim that the compositor atomically rolls back invalid
configuration files.

The settings shape is `shortcuts.rebindings`, indexed by stable identities from
the owned catalog. A chord changes an ordinary owned press binding; `null`
disables it; removing the entry restores its configured default. Original command
and action identity remain module-owned. Neither the UI nor this file contains
arbitrary dispatch commands. Paired switcher/release bindings, security policy and
unrelated consumer bindings are not editable here. Requested and fully merged
maps both pass collision validation, including consumer reservations and selected
application declarations, before publication.

Apply, Disable and Restore use an exact captured map generation. The settings
transaction compares the previous section under its existing lock, so a stale
editor cannot replace newer settings. Failure to confirm an effective update triggers compare-and-set
rollback only while the helper still owns that section and map; concurrent
changes or failed rollback are reported as uncertain rather than overwritten.
The previous map remains active while the configuration builds. Reopening the
session rebuilds the same bindings from the durable Nix source.

Provider reads have a 20-second budget; deployment commands have a 4000-second
budget to allow builds. Standard output is limited to 512 KiB and diagnostic
output to 64 KiB. Timeout, interruption or excess output terminates and reaps
the owned process group and stores private diagnostics. A failed or malformed
mutation response can mean partial activation: it is reported as unconfirmed,
without blindly launching rollback while the deployment outcome is unknown.
Declaration reads follow declarative store symlinks but require regular opened
files and consume at most their size limit plus one byte. Session records and
transaction locks reject links and nonregular files. Transaction directories and
locks require private ownership/modes. A second request receives a busy response
instead of waiting behind an ongoing build; normal validation errors remain
retryable.

The isolated acceptance uses actual immutable module outputs, the real validator
and edit helper, synthetic Qt typing/pointer input, and disposable deployment
success/refusal/no-confirmation/rollback-failure transports. Actual native
shortcut acceptance installs the module-generated rebinding, verifies dispatch
and menu hints, and keeps application text input separate. This does not certify
layout switching, arbitrary compositor modes, raw keycode configuration or keys
that a remote client never transmitted.

The separate Settings presentation corpus opens the actual Accessibility →
Configured Shortcuts page at 480×360 and 790×640, with 100% and 150% text in
both light and dark appearances. It follows real Qt Tab and reverse Tab focus
through the selector, chord field and all four actions, types a disposable draft
and discards it by keyboard. Each of 48 frames checks the actual parent
Flickable's focus reveal, laid out text bounds and painted control foreground.
The pane may exceed the viewport; its scrolling parent must reveal each control.
An injected presentation service rejects all mutations, so this corpus does not
deploy settings or certify native input masks. Qt offscreen's exact unsupported
window-mask diagnostic is retained and counted once per window show; all other
warnings fail the fixture. The functional editor and native runtime acceptance
above cover application and dispatch separately.

## Layout observation acceptance

`nix build .#shortcut-layout .#shortcut-settings` runs the warning-fatal Rust
corpus, including actual US/German XKB symbol/code conflicts, a nonidentity
modifier-index case matching the pinned provider, unavailable/bounded inputs,
partial/oversized IPC frames, and owned candidate cleanup on early refusal.
The actual Qt model test covers native custom labels, keycaps, collision display
and malformed-observation retirement. The edit transport fixture includes a
physical overlap refusal before any settings mutation, then real set/disable/
restore and rollback journeys; its compact and regular panes use the same
scroll/focus-reveal contract as Settings rather than assuming all diagnostics
fit into one window.

CI's `shortcut_layout_*` cases run the production observer and full owned shell
against both compositor variants with immutable real US/German virtual keyboard
maps. They require visible keycap labels inside the actual Settings viewport,
recalculated collision state without command activation, exact native switcher
capture, cancellation while the old modifier is still acknowledged held, and a
single successful commit using the newly rebound key. Screenshots, provider and
helper observations, controlled input acknowledgements and result JSON are
retained under `artifacts/shortcut-layout-*`. The effects case additionally
requires the actual logged software renderer identity, rather than inferring it
from environment variables.
