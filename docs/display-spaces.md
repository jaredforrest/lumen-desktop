# Display spaces

The consumer-owned `spaces` settings select independent displays (the default)
or linked display contexts. `primaryDisplay` is either null for an automatic
stable choice, a connector record, or a make/model/serial record. Display metadata
is a placement identifier, never authenticated device identity. Automatic selection
uses initial display geometry and identity, never the currently focused display.
A disconnected preferred display retains its preference while a deterministic
connected fallback serves temporarily.

A linked context contains one existing logical desktop per connected output.
Enabling it starts with the currently visible composition. Existing desktops,
windows, assignments and logical identities are not renamed, merged or moved.
Missing members are new empty logical desktops. The existing 64-desktop budget
also bounds cohort completion; an over-budget plan fails explicitly and does not
remove an existing desktop. A failed user switch is never retried as a different
action. Ordinary native workspace navigation remains a recovery route when a
captured configuration becomes unavailable.

The daemon persists logical members before native registration. Registration
captures the compositor session nonce, generation, native output identities and
geometry, existing workspace identities and literal names. The native command
accepts bounded hex-encoded JSON, validates every member before visibility changes,
and applies peer visibility without delivering intermediate keyboard focus to
peers. All normal native focus sources use the same hook. Explicit context actions
add generation comparison and cannot fall through to independent navigation when
rejected. Empty workspace destruction retires its native identity while retaining
the logical member for later recreation. Output topology changes invalidate the
native plan; reconciliation repacks migrated members into reachable contexts
without moving user windows back on reconnection.

The Dock appears only on the main display in linked mode. A Dock keyboard request
from another output is routed to that main display. The read-only observer expires
unresponsive authority and does not start or configure services. Legacy compositors
use an explicit unapplied independent Dock fallback. Observer loss or malformed
frames retain the last validated Dock routing, with a deterministic connected main
fallback if that output disappeared. This recovery routing never advertises applied
coupling; Settings continues to report the unavailable policy. Incoming UI facts
validate their types, identities, session and exact decimal generation before they
can update applied state. Window Groups retain
membership but suspend visibility management while displays are linked. New paired
layouts on secondary outputs are refused; existing pairs and survivors remain
valid and can exit through their existing restore path. Ordinary fullscreen stays
within its existing workspace/output semantics. This policy does not implement
spanning desktops or blank secondary displays.

Settings edits use the existing explicit settings compiler. Runtime observations
never write the consumer configuration. Display-space behavior belongs to the
public desktop; monitor choices and whether to enable linking belong to its
consumer.
