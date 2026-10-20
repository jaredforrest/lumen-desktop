# Installed application and window switching

`tests/desktop/installed_switcher_input.py` runs a private full installed shell
with real independent Wayland terminal processes: multiple windows sharing an
application ID and windows belonging to other application IDs. Application
membership, history, creation order and activation all come from production
shell code and compositor/foreign-toplevel facts. Read-only fixture IPC exposes
state; it does not replace the application model or activation callback.

Physical virtual-keyboard input invokes the installed desktop-action dispatch
through the production service-PID router. The copied routing wrapper replaces
exactly its fixed systemctl executable with a private PID-file locator. A
manifest records both wrapper digests and the exact substitution; no host user
service is contacted. This is the production router with a private service
locator stub, not a claim to exercise a personal systemd user session.

Both compositor variants exercise forward/reverse application selection,
modifier-release commit, Escape cancellation, no preview activation, same-app
creation-order cycling and reverse cycling, minimized targets, selected-app
removal and same-app replacement. Native container ID and process ID are checked
against each created process. The native window event log records focus changes.
A replacement document of the same application may receive an application-level
commit; another application cannot inherit its snapshot slot.

The regression also exercises a delayed focus fallback. Once its target gains
native focus, its pending timer is stopped; a newer non-origin native focus
transition also cancels it. Expected restoration of the invocation's origin
while a layer-shell keyboard grab is released remains allowed. An actual focus
request is suppressed only in explicitly labelled fault-injection cases, so the
installed delayed fallback can be tested without replacing its implementation.
Destruction cannot redirect it to a new window instance.

A compositor active-window fact cannot distinguish restoration of the original
window from clicking that already-focused origin without additional input
provider metadata. No global input monitoring is added. Remote client delivery
of the modifiers remains a separate user-side acceptance check.
