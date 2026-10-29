# Window action acceptance

Window behavior is covered by several independent fixture scopes, rather than one universal application adapter:

- `application_lifecycle_input.py` retains a validated running application after its last window closes, restarts the observer and shell, and exercises advertised cooperative dirty-document Quit.
- `owned_window_context_input.py` keeps an owned Files document association independent of Quick Look/inspector key focus. Unknown third-party document associations remain unknown.
- `window_relationships_input.py` and `xwayland_relationships_input.py` exercise protocol parent/modal identity and actual keyboard recipients. Neither protocol proves an application's main document role.
- `window_frames_render.py` exercises native minimize/restore geometry and distinct hide/minimize state, including both states on one window. `desktop_lifecycle_input.py` additionally covers saved placements after desktop/output changes.
- The Rust 2024 `window-controls` scenario in `test-desktop-policy-fixture` uses an actual constrained protocol client, a native pointer, production Dock window rows and a private keyboard. It checks unavailable controls and explicit command refusal, supported fixed-size minimize, utility Close, and two independently minimized documents restored through their exact application-menu rows.

Unavailable entry commands fail explicitly. Fullscreen exit and placement restore remain available to release an existing state even when entry capability has changed. Close remains cooperative with the application; constraints do not imply that every action is unavailable.

The separate minimized-window destination is covered by the Rust 2024 `minimized-dock` scenario in `test-desktop-policy-fixture`. It checks same-title window identity, distinct short-lived handles, exact keyboard and pointer restore, held-pointer retirement, and destination switching under both compositor renderers. Inactive-control reference comparison and third-party main-document adapters are separate scopes.
