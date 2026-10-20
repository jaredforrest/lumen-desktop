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
