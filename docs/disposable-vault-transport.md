# Disposable vault HTTPS transport

The compatibility harness uses a private verified TLS front end and a loopback
HTTP backend inside the same disposable network/PID/filesystem namespace. The CA
and private key remain in its ephemeral home. It changes no host trust store.

TLS handshakes are deferred from the listener's accept path to request workers.
Accepted socket reads have a two-second inactivity timeout. This is not a total
deadline against a peer continually supplying request bytes. Existing backend
requests retain their independent ten-second timeout and body/response bounds.

Closing stops the listener, interrupts tracked incoming sockets, and joins the
owned request workers with a three-second shared budget. A worker that cannot
stop causes an explicit error; it is not silently treated as clean shutdown.
The surrounding disposable PID namespace still owns final failure cleanup.
Only expected peer reset, broken pipe and TLS EOF disconnects are handled as
normal disconnects. Unexpected handler exceptions retain server diagnostics.

`tests/desktop/browser-workflows-rust/src/bin/vault-transport-native.rs` is an explicit CI namespace test, not a
Nix-build unit test. It generates a private certificate with the immutable TLS
tool, starts only private loopback listeners, and proves:

- A TCP peer that never sends TLS cannot block a separately verified HTTPS
  request or listener/worker cleanup.
- An incomplete HTTPS request body expires through the socket read timeout.
- Explicit shutdown interrupts an incomplete body and joins all owned workers.

The private backend and transport workers are reaped on failure.
Retained receipts contain timings and lifecycle facts, not certificates or keys.
This transport regression does not claim browser/vault feature acceptance; the
separate compatibility harness tests that boundary.
