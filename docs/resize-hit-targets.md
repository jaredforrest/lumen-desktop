# Native frame hit-target acceptance

`tests/desktop/resize_hit_targets_native.py` exercises all four frame edges and
four corners with an unmodified physical pointer protocol client. It reconstructs
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

The 33 checks per backend use one physical motion followed by release. An
independent two-motion rapid-release baseline exposed a stale floating-client
configure overwriting the latest width while retaining the newer origin. That
race remains open after this hit-target-only batch; these passes do not establish
general rapid-resize stability or whole geometry-family completion.
