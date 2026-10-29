# Native frame hit-target acceptance

`tests/desktop/desktop-policy-rust/src/resize_hit_targets.rs` exercises all four frame edges and
four corners with a Rust 2024 GTK pointer protocol client. It reconstructs
the outer rectangle from actual native content/decorations and samples the
centre of the advertised logical border. It does not invent an outside-frame
target width or claim measured reference-system parity.

At actual output scales 1, 1.5 and 2, the fixture verifies directional resizing,
stationary opposite edges, client configure acknowledgement, exact captured
container/PID identity, unchanged adjacent windows and no leaked client button
events. Per-scale controls prove positive client input, topmost neighboring
content ownership over a covered target edge, and minimum-size corner clamping.
All clients, input, output configuration and screenshots stay in a private
filesystem/PID/network/session-bus namespace. Source and immutable tools are
checked before and after execution; logs are scanned after owned children exit.

The initial 33 checks per backend use one physical motion followed by release.
An independent two-motion rapid-release baseline exposed a stale floating-client
configure overwriting the latest width while retaining the newer origin.

The appended interactive-resize patch reuses the existing compositor-owned
placement transaction guard for native XDG views. Each pointer motion invalidates
the earlier expected acknowledgement; the transaction records the latest pending
size and configure serial. Older commits cannot overwrite that size, and the
guard ends on the matching or later serial, not merely on button release. The
existing serial-wrap and pending-dimension checks remain in one implementation.

`resize-hit-targets-rapid` sends two half-motions then release without waiting for the client
between motions. Its 34 checks retain the original hard geometry/ownership
assertions and add a later client-authored resize to prove the guard releases.
Both variants run in CI; existing rapid Fill/Zoom restoration checks remain
separate regression coverage. These cases establish the reproduced XDG race fix,
not every client protocol, arbitrary acknowledgement schedule or whole geometry
family completion. No extra outside-frame target width is introduced.
