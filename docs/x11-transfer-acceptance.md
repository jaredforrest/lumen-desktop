# Managed X11 transfer acceptance

The `--transfer` branch of the existing isolated X11 fixture uses the production
`lumen_spaces::transfer` snapshot, source/family, destination and packet API. It
uses the same disposable Xwayland server, XRes-owned protocol clients, native IPC,
namespace boundary and cleanup as the ancestry/presentation corpus. It does not
recreate the native codec or claim physical Overview card coverage.

The finite corpus exercises floating and tiled roots, same-process nonmodal
floating descendants, exact native PID/foreign-surface/XID/parent preservation,
child frame offsets and sizes, and no activation of the destination desktop.
Captured root/child geometry changes and root retirement/replacement must refuse
with the full native tree unchanged. An unsupported raw cross-process transient
edge must disable the typed source and refuse a forged fresh native packet.
Direct unowned native projection on either root or member must likewise prevent
both typed capture and a forged fresh native dispatch until explicitly cleared.
The XRes-disabled variant checks that unknown ownership cannot produce a typed
source. Both backends run through CI and retain raw outcomes and exact tool hashes.

This is an actual native protocol/outcome test, separate from the existing
physical card/handle journey and its keyboard/drag cancellation coverage. It does
not authorize client PID hints, titles or application names as document identity.
