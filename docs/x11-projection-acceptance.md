# Managed X11 presentation acceptance

The existing Xwayland relationship fixture has a separate `--projection`
scenario. Its original relationship and no-XRes scenarios remain separate.
All variants reuse the real disposable Xwayland server, XRes owner queries,
acknowledged Rust/X11 client and physical keyboard protocol helper. The client
uses pinned `x11rb` and the shared warning-fatal owned Rust package; its original
map/unmap, new/destroy, parent/modal/PID/override, property, WM_DELETE, keyboard
journal and clean server-disconnect behavior is retained. The superseded C
source is removed. The optional
`--without-xres` starts an actual server with XRes disabled; it does not falsify
the compositor PID or capability response.

The projection scenario records actual GET_VERSION and exact versioned native
metadata. For supported providers it checks ordinary managed X11 origin versus
effective placement, retained XID/PID/foreign identity and frame, current-desktop
focus and exact key recipient, actual EWMH fullscreen requests and property
responses, suspension/recovery, explicit per-member intent for a bounded known
nonmodal family, modal ON refusal with usable global activation, and independent
intent after unparenting and owner clear. No parent/application flag is inherited.
It also checks stale clear refusal and fresh checked clear, plus
tiled X11 structure and an actual shared group's one-at-a-time and
all-together masks with distinct real keyboard recipients. Rejected commands
must return the exact native refusal and leave the entire tree unchanged. A
background fullscreen negative waits for a fresh real `_NET_WM_STATE` property
notification and reads the returned fullscreen atom before comparing topology.
This is managed X11 protocol coverage, not toolkit/document adapter coverage.

Genuinely capability-absent providers take a distinct native floating sticky
compatibility path, preserving identity and real keyboard delivery. Unknown
capability versions fail. The fixture does not fabricate absence on a modern
provider. CI runs modern, real no-XRes and genuine legacy scenarios on both
renderers, retaining the existing `xwayland-relationships-*` artifact namespace.
The test-only `--projection-groups` selector calls the same group helper before
the other lifecycle cases, allowing an actual older-provider eligibility failure
to be recorded without changing the full scenario's assertions.
