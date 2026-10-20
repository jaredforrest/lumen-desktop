# Native edge layout acceptance

The edge-layout runner enters a complete disposable user, process, mount and
network namespace before starting any compositor or client. It captures immutable
tool identities and frozen source before entry and verifies them again after all
children are reaped. Its terminal uses a newly allocated private PTY instance;
no host terminal, display, session bus or input device is exposed. Disposable
terminal clients explicitly choose the available pinned DejaVu Sans Mono family,
avoiding reliance on host font-alias configuration or suppressing font warnings. Effects runs
explicitly request the existing software graphics capability and pinned driver.
The ordinary compositor exposes no graphics device.

A fixed-size client intentionally refuses `lumen_window left`: the only accepted
failure is exit status 2 with one IPC result containing `success:false`,
`parse_error:true` and the exact error `Window action is unavailable`. The runner
records the raw reply and complete before/after window snapshots, then checks
captured identity, geometry and placement state are unchanged. Other commands
still require success. This repairs an obsolete fixture expectation after native
disabled-action enforcement; it does not relax that enforcement.

The bounded client still requires successful no-op left placement when the first
output's tile exceeds its maximum size, an actual valid drag commit on the
narrower second output, and successful explicit left placement on that second
output with exact restoration. Physical fixed/bounded previews and pointer
lock/confinement retain their existing assertions. The complete suite also
retains target identity, preview/leave/cancel/commit, modifiers, top arbitration,
lock cancellation, output geometry and restore checks without wider deadlines or
retries. CI runs the complete corpus on both compositor variants and retains
command receipts, generated harnesses and source/tool provenance.
