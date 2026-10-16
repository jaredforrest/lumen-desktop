# Runtime observation and transport primitives

`lumen-desktop-state` is a read-only compositor observer. It validates the owned
local compositor socket and emits window IDs scoped to an endpoint epoch, output
bounds, workspace membership, key-window state and observed frame ownership.
It omits window titles, PIDs, environment and document contents. It opens no
listener, grants no privilege and performs no window actions.

The snapshot explicitly identifies its limitations: separate compositor IPC
queries are not an atomic revisioned snapshot; app IDs are client-advertised
hints; app residency, main-document ownership, hidden state and menu export
require additional app/backend capabilities. Scratchpad membership alone is not
proof of minimisation. Missing information is null or an unavailable capability,
never fabricated from a title or list order. Namespaced placement fields are
used only when actually supplied by a compatible compositor.

Run `lumen-desktop-state --fixture /absolute/synthetic-state.json` for an offline
fixture containing `tree` and `outputs`. Production diagnostics should retain only
synthetic or appropriately private artifacts. The command's JSON has schema
version 1 and must not be confused with a complete application registry.

`res/lumen-desktop/runtime/protocol.py` supplies dependency-free primitives for
future adapters: bounded four-byte-length UTF-8 JSON frames, duplicate-key and
nonfinite-number rejection, fragmented frame decoding, peer/epoch-scoped replay
fingerprints and revision-gap reconciliation. These primitives are not a running
command broker and do not claim authentication or capabilities an adapter has
not implemented. Replay retention fails closed at capacity; uncertain destructive
requests require reconciliation after a process restart.

Regression tests use synthetic streams, fake monotonic time, isolated socket
paths and compositor trees. They cover changed-payload retries, bounded allocation,
sequence/epoch gaps, duplicate identities, unknown minimisation and title privacy.
No test reaches a real account, modifies a document or injects live input.

## Opt-in clipboard history

`lumen-clipboard-history` provides status, list/search, explicit enable/disable,
clear and copy-by-ID. It never pastes into another application. Collection begins
only after the user accepts the warning and enables the installed integration.
The module integration is off by default and requires an explicit consumer
retention and item bound. Starting an overlay only reads status; it cannot start
collection. A failed collector start rolls consent back instead of showing success.

Only consent persists. Entries live under the private user runtime directory,
expire by monotonic age, are bounded, and disappear at logout. Disabled/expired/
cleared entries cannot be copied by stale ID. Marked sensitive clipboard offers
are excluded. Arbitrary unmarked secrets cannot reliably be recognised, so the
consent interface explains this limit and provides Clear and Disable. Tests use
synthetic text and injected writers; no regression captures the real clipboard.
