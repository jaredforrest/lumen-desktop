# Native focus guard version 2

This adapter closes the interval between a sampled desktop policy decision and
compositor dispatch. The observer still validates its owned socket epoch and
live PID/start/executable witnesses. The compositor validates its actual seat
source, target and complete policy topology inside one IPC callback, before
performing any operation. Kernel process lifetime is not atomic with compositor
state; a process can exit immediately after its last witness check. This does
not authorize a replacement surface or process to inherit a captured decision.

`GET_VERSION.native_focus_guard` must be integer `2`. Missing or unknown versions
disable exact backing-window captures and return explicit unsupported results;
the original window-based path remains available. The owned shell exemption
uses the observer's exact live launching-parent process witness, never an
executable basename. It is not saved in the recovery journal.

The digest keeps valid requests below the existing compositor IPC whole-payload
buffering limit. The actual bounded request remains at most 64 KiB of hex plus
its command prefix. Oversize tokens just above that limit are tested at the
parser; an earlier 2 MiB transport diagnostic timed out before the parser and
is not counted as parser-rejection proof. Kernel socket buffering still belongs
to the transport: a transport failure is uncertain, bounded, and never replayed.
SHA-256 collision resistance underlies topology digest comparison; surface and
source witnesses remain explicit exact fields. Both implementations check the
empty-object canonical golden vector (`0600000000`) against digest
`b45482224b439a3d548c65378929b7dcc16a42288530b7b20d5c8103cc879d10`.
The compositor advertises version 2 only after that C/OpenSSL EVP self-test.

The runtime command is `lumen_focus_guard HEX`. HEX contains lowercase hexadecimal
of a fixed-layout binary packet, bounded to 32 KiB decoded / 64 KiB encoded. No
user-provided command string is present. All integers use big-endian byte order.
The packet has exactly these fields, in order:

1. Four bytes `LFG2`.
2. One source-kind byte: `0` means affirmatively no focused client, `1` means a
   following native source witness. Other kinds are rejected.
3. A target witness.
4. A fixed 32-byte SHA-256 digest of the canonical policy topology.
5. A four-byte action count followed by that many typed action records.

A witness is unsigned 64-bit conID (`1..i64::MAX`), unsigned 32-bit PID
(`2..i32::MAX`), unsigned 32-bit foreign-identifier length (`1..256`), and that
many identifier bytes. NUL/control characters are rejected. An action record is
one byte (`1` unhide, `2` show scratchpad, `3` focus), followed by a witness.
All unhide records must precede show/focus and have distinct conIDs. Show is
optional, must target the final target and must be followed by focus. Focus is
optional, last and may occur only once. Empty/no-op and unhide-only plans are
intentional: shared policy can find the target already focused, or unhide a
group without switching spaces. No focus is fabricated in those cases.

The typed layout admits no duplicate or unknown fields; extra bytes, unknown
operations/version, invalid lengths/ranges, duplicate unhide actions, dangling
show, missing references or unavailable placement reject the whole packet
before mutation. All affected surfaces must retain exact conID/PID/foreign ID.
Native source must be the actual keyboard-focused view and seat node. A focused
layer is unsupported, never equivalent to no focused client.

The canonical topology is the compositor's complete recursive tree with window
titles (`name` on non-root/output/workspace objects), `title`, `representation`
and window geometry omitted. Root/output/workspace names and geometry remain.
All other fields remain, including focus, transient parent/modal relationships,
hidden/minimized/restore state, workspace/output layout, sticky/fullscreen state,
display spaces and window groups. This deliberately conservative snapshot can
reject unrelated policy/state changes (including urgency or layout percentages).
Geometry changes to an ordinary window alone do not reject a request. Neither
titles nor app-name heuristics identify the captured target.

Canonical tags are: null `0`; bool `1` plus one `0/1` byte; signed i64 `2` plus
eight two's-complement bytes; finite IEEE-754 f64 `3` plus eight bit-pattern
bytes; UTF-8 string `4` plus u32 length and bytes; array `5` plus u32 length and
values; object `6` plus u32 count and key/value pairs sorted by UTF-8 key bytes.
Object keys use the same tagged string encoding. The writer validates raw tree
size before cloning/traversing recursively; depth is at most 64, each collection
at most 8,192 entries, at most 8,192 tree nodes / 2,048 window nodes, and at most
2,048 actions. Canonical allocations are bounded to 1 MiB; decoded wire allocation is bounded to 32 KiB. The joint wire/action bound can reject a very large group before dispatch.

The compositor regenerates the bounded canonical bytes and compares their SHA-256 digest before dispatch.
New modal windows, closed/replaced targets, changed source focus, moves,
group/visibility changes and changed workspace/output state reject the packet.
Every action is preflighted before the first mutation, then fixed internal
unhide/show/focus operations run synchronously. Validation errors report
`LFG_REJECTED` with no action. Any unexpected operation failure reports
`LFG_UNCERTAIN`; the observer reports uncertainty and does not retry or roll back.
This is local desktop IPC, not a new authentication or remote control listener.
