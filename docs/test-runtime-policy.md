# Disposable test socket paths

Retained evidence belongs in the requested artifact directory. Runtime sockets
belong in a separate, short, private directory owned by the fixture. Keep that
directory alive until its children exit and are reaped, then remove it.

The pinned Qt 6.11.2 local server **and client** reject encoded socket paths
longer than 106 bytes on Linux. They reserve two bytes in the 108-byte
`sockaddr_un.sun_path`, including for ordinary filesystem sockets. Count UTF-8
bytes, not displayed characters, and reserve framework-generated suffixes.
The native-input runtime test reserves
`quickshell/by-id/xxxxxxxxxxxxxxxx/ipc.sock` and binds a real Unix listener.

Hosted clipboard evidence contained a 107-byte IPC path below an inherited
`/home/runner/work/_temp/nix-shell.XXXXXX` temporary parent. The shell loaded
its configuration but could not start its IPC server. With the unchanged
fixture, a controlled 37-byte temporary parent produced a 105-byte IPC path
and passed; a 40-byte parent produced a 108-byte path, the same IPC failure,
and the existing status timeout. This is a path-budget failure, rather than
evidence of a slow startup or a permission failure. No timeout was increased.

`tests/ci/run-native.nu` supplies `TMPDIR=/tmp` to the test command through
the standard environment executable. It preserves streamed diagnostic logs,
failure exit status, and warning rejection. It changes neither the calling
environment nor host configuration. Fixtures still create their own private
random directories; `/tmp` itself is only their parent.

The shared native-input Session additionally uses a short owned runtime root
for direct calls, independently of inherited `TMPDIR`. Its existing artifact
locations, input assertions, and child-before-directory cleanup remain intact.
Tests require mode 0700, a bounded real socket path, and removal after use.

The command policy does not fix fixtures that explicitly put runtime sockets
under their artifact output, or whose own long prefixes and suffixes consume
the path budget even under `/tmp`. Those constructors require their own short
runtime roots. The source survey and separate acceptance receipts track those
conversions; passing one clipboard case does not establish every fixture's
socket safety.
