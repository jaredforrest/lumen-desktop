# Exact foreign window identity

The optional project-owned `lumen_toplevel_identity_manager_v1` protocol links
one live `zwlr_foreign_toplevel_handle_v1` resource to the compositor's existing
`ext-foreign-toplevel-list` identifier. This is an identity lookup, not an
activation permission. The existing broker's process, session and captured
target guards remain authoritative for native activation.

The typed handle argument belongs to the requesting Wayland connection. The
server checks the client and exact resource pointer membership in live view
resource lists, without interpreting the argument's user data. It returns the
existing identifier only for one unambiguous live match. An inert resource
retained after ordinary closure returns unavailable with an empty identifier.
Malformed object IDs, null or wrong-interface wire arguments may instead cause
Wayland core protocol errors before the handler runs. The bridge global follows
the same security-context visibility classification as the foreign managers.

Each request creates one reply resource. The server snapshots membership,
queues one terminal result and destroys that resource synchronously, without
retaining a view pointer, worker, listener or timer. There is no wire cancellation
request. The client may abandon its local proxy. Generated QtWayland code does
not retire incoming destructor-event proxies, so the production reply helper
explicitly destroys its raw proxy exactly once before invoking completion.
Pending generation, owner and self guards cover cancellation and reentrant
QObject destruction. Capability loss clears identifiers and pending replies;
the old manager proxy is retired before a replacement global can be bound.

Quickshell exposes the optional `foreignToplevelIdentifier` on the same existing
window QObject. Empty means unavailable. App IDs, captions, proxy object numbers
and observer PIDs never substitute for this identity. Different observer
connections may see different local handles for the same stable identifier.

The application switcher uses exact one-to-one pairs between these QObjects and
validated broker captures. App keys only partition presentation groups. A group
must have complete unique pairs, and the existing global coverage gate must
also be true. Its broker MRU choice retains the matching QObject and complete
capture, including session/process witnesses. Missing capability, duplicate or
missing identities, unsupported members and broker-only records preserve the
original QObject fallback. A broker-only app-key match cannot synthesize a
native target. The global gate remains conservative: enabling independent
supported groups while other groups are unsupported needs separate mixed-client
acceptance before that restriction can be relaxed.

The upstream QtTest uses the actual production reply helper and generated wire
protocol over a disposable Wayland socketpair. Address, undefined-behavior and
leak sanitizers cover delayed, unavailable, malformed and duplicate results,
cancelled generations and owner destruction before and during completion.
Native acceptance separately checks the actual compositor membership and
Quickshell property: owned PID tree identifier sets, duplicate-title windows,
independent observer connections, hidden-view observer restart, ordinary closure
and retained inert resources on four independent wire connections. Those tests
do not certify an upstream standard identity bridge or reference-exact behavior.
