# Owned screen-reader acceptance

`tests/desktop/screen_reader_input.py` runs the installed Search, notification
stack and session confirmation components with real Orca and AT-SPI. Providers
contain disposable sentinel data; application activation and final session actions
are private observers. The controls, focus handlers and accessibility roles are
imported unchanged from the installed package. The manifest records their hashes.

The reader and all its peers run in private PID, user, network and mount/proc
namespaces, with private session/accessibility buses, home, settings and display.
This prevents the reader's ordinary same-user instance discovery from discovering
or signaling a reader in the desktop session. Namespace creation is a required
test capability; failure is an error. The live accessibility socket identity is
checked before and after. No host accessibility preferences or audio are used.

Speech Dispatcher uses its supported generic module, a fixed absolute `true`
executable, and one ignored quoted DATA argument. The exact pinned generic source
and its quote escaping are checked. Its module directory contains only this
observer, and its server has an empty executable search path. The separate
transport test sends adversarial quote/command-looking text and verifies actual
accepted utterances and BEGIN, END and CANCEL acknowledgements without executing
that text. This is a speech-generation/transport test, not audible voice quality.

The reader fixture verifies physical search typing/Tab, settled query/caret and
selection during provider mutation, rejected stale accessible actions, notification
stack expansion and ordered card/body speech, confirmation context and controls,
one actionable error announcement, and Escape returning actual focus and speech
to the opener. SSIP transcript records are accepted by the real dispatcher, with
completion events. AT-SPI receive timestamps are retained; these are client
observation times, not a measured universal latency or usability guarantee.

Test-only compatibility dependencies retain the pinned runtime's behavior while
using current supported GLibUnix, GDK default-seat/keymap and GIO closure APIs.
The AT-SPI typelib correction removes four incorrect Action alias shadow markers;
genuinely deprecated aliases remain deprecated. A real production Action peer
proves canonical calls and distinct deprecated-alias behavior. Original/corrected
GIR hashes are retained. No deprecation flags or Python warning filters are removed.

This does not establish braille, audible pronunciation, other toolkit usability,
or global reader modifier-key grabs. Orca's supported command bus can be used by a
separate optional typed shortcut adapter, but that is not implemented here. Such
an adapter needs a fixed command allowlist, exact reader owner/lifecycle checks,
and fail-closed lock/unknown-session handling before it can be enabled.

## Optional typed command adapter

The `reader-command` package is disabled by default. It never starts, replaces,
kills or configures a reader. Consumers can opt in and select their own chords:

```nix
services.lumen-desktop.integrations.readerCommands.enable = true;
services.lumen-desktop.shortcuts.readerBindings = {
  "Mod4+Control+F8" = "where-am-i";
  "Mod4+Control+F9" = "read-all";
  "--locked Mod4+Control+F10" = "stop";
};
```

No chords are consumed when disabled, and none are selected by default. Reader
bindings cannot replace the desktop's own bindings; the existing effective
shortcut registry also checks physical aliases and consumer reservations. A
compatible reader must already own `org.gnome.Orca.Service` on the existing Unix
session bus. The adapter does not activate that name or change accessibility
settings. The tested provider is Orca50.2.

`lumen-reader-command` accepts exactly one of `where-am-i`, `read-all`, or `stop`.
Fixed server-side module/action pairs are checked against the provider's supported
command list. The adapter captures its unique bus owner, same-user credentials,
PID/start identity, and checks them before and after a single dispatch. Every
D-Bus call has a remaining budget from a2.5-second deadline; a separate pinned
4-second process guard also bounds connection/authentication stalls.

Reading commands require the native compositor's known-unlocked authority and
unchanged socket/owner/revision at dispatch. Locked, unknown, disconnected or
changed authority refuses them. Safe stop retains the reader identity checks but
works even when lock authority is locked or unavailable. It cannot read context.

JSON reports `accepted`, a fixed reason on failure, and whether command dispatch
was attempted. A missing reply after dispatch is `dispatch_unknown`; there is no
automatic retry. If the outer process guard terminates without JSON, the outcome
is also uncertain. D-Bus and compositor lock changes are not one transaction:
these are predispatch gates and post-acknowledgement checks, not revocation of an
already issued command or a promise of silence after a later lock. A persistent
lock-triggered reader cancellation mechanism remains a separate capability.

Private acceptance covers actual supported reader commands and accepted speech,
a physical consumer key route, actual native lock/unlock, and safe stop with a
missing compositor. Synthetic private-bus tests separately cover owner retirement,
lock changes during discovery, withheld replies, ambiguous acknowledgements and
no replay. This does not claim the reader's own global modifier-grab protocol
works; the adapter uses explicitly configured compositor bindings.
