# Notification preview authority

Notification previews support `always`, `when_unlocked` (the default), and
`never`. The notification backend redacts subject, body, image, icon and action
labels before sending its UI snapshot or signal. It keeps the original history
in its existing private directory and atomically written, owner-only files.
Redacted actions cannot be invoked by a stale UI command.

The native compositor publishes `get_version.lumen_session_lock` with schema
version, manager availability, actual locked state and monotonic transition
revision. An abandoned lock remains locked. Both supported lock clients use this
same session-lock protocol; no client-written marker can authorize disclosure.
Native transition tick events only wake the reader. Every publication rechecks
the native facts over IPC, verifies the compositor peer and socket identity, and
rejects disconnected, unsupported, malformed or regressed authority state.

The service consumes `SWAYSOCK` from its session environment. A missing address
is unknown authority, never an unlocked session. Unknown authority always hides
content. A known unlocked session permits the
selected preview policy. A locked session permits content only for `always`
when the consumer supplies `LUMEN_LOCK_PREVIEWS=1` to the notification service.
The default is deny. This permission is host policy and is not a user preference
or an authentication bypass. Consumers own any corresponding deployment policy.

The notification service instance and transport revision identify UI snapshots.
The watcher pins the unique D-Bus owner and drops reordered replies. A per-watch
generation rejects late callbacks from replaced helpers. The UI rejects stale
revisions, clears
content on transport failure, and reconnects with a new instance handshake.
Changing disclosure policy never rewrites the retained original history.
