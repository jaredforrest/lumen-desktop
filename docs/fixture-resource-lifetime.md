# Native fixture resource lifetime

A fixture's `Popen` call borrows file descriptors from an explicit log-file
context. The parent closes that file as soon as child creation returns; the child
retains its duplicated descriptor. No temporary anonymous `Path.open()` result
may be passed directly to `Popen`. A unittest cleanup registration is also an
explicit owner for longer-lived test logs.

Parent-side stdin/stdout/stderr PIPE endpoints remain open while the fixture
uses them. Existing teardown first terminates and waits for only its captured
children. After reaping, `close_reaped_pipes` closes all parent endpoints
idempotently and rejects any still-running child. A final bounded reap covers
children already killed by the existing timeout cleanup. Input commands, child
PIDs, functional deadlines, geometry and test assertions are unchanged.

`test_fixture_resources.py` checks all fixture sources for anonymous or unowned
named subprocess logs and requires parent-pipe cleanup in native process-list
fixtures. Its real child test proves streams are usable before reaping, a live
child is refused, output can be consumed after exit, and explicit cleanup closes
all three streams without double-close errors.

CI uses `tests/ci/run-native.sh` so a zero child exit does not hide an unraisable
finalizer warning. Replay uses `PYTHONWARNINGS=error` and retains the command log.

Legacy render helpers that are not in the CI native matrix are covered by the
source ownership checks. Their standalone execution is not evidence of safe
isolation: use a private HOME/XDG environment, activation-free private bus and
inert system-bus address, and never inherit a live compositor or account session.
