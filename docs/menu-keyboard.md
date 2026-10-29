# Menu keyboard ownership

Owned menus traverse the complete enabled heading ring with Left and Right, including headings represented in overflow. Hidden headings use the real overflow button as their anchor. The invoking window, application context and menu snapshot remain captured across heading changes. Window disappearance or an observed exporter generation change cancels the captured traversal.

Exported menu cards share a focus scope. Up and Down skip disabled rows and separators, Right enters a submenu, Left returns to its selected parent, and Escape closes one level before dismissing the root. The top-level heading ring crosses between owned and exported menus. Holding the read-only export snapshot across an owned menu does not relax the bridge's focused-client activation check.

Keyboard, pointer and assistive row intents retain the actual client token, exported layout revision, node and enclosing path. Replacement or changed layout invalidates previous intents, including identical numeric IDs and labels. The bridge retains the provider's uint layout revision and checks exact equality again after refreshing before dispatch. Revision wrap has no ordering assumption. Unrevisioned existing bridge clients retain their existing API; the owned menu sends the captured revision. A provider that changes an action after the final refresh remains subject to the external protocol's non-transactional action interface.

Rows expose semantic menu-item roles, enabled and checked states, shortcut descriptions and focus. Assistive activation retains its own delegate-generation intent; a stale reference cannot inherit a replacement row. Pointer hover updates highlighting without manufacturing a new keyboard activation intent after a model refresh.

The native fixture uses an actual private same-process XWayland window and DBusMenu exporter, matched by PID and native window ID through the installed registrar. It tests dynamic checked/disabled/shortcut rows, same-ID unchanged-label revisions, nested and overflow travel, actual client input, stale pointer and assistive references, exporter replacement and exact owner dispatch. This protocol coverage does not imply that applications without an export provider expose their menus. Escape depth and other keyboard policy are project interaction choices; external-reference exact timing and geometry are not claimed.

```sh
nix develop --command lumen-menu-native-fixture keyboard --launcher /path/to/lumen-desktop-shell --sway /path/to/sway --pointer-tool /path/to/wlrctl --at-spi-prefix /path/to/at-spi-prefix --exporter /path/to/lumen-menu-exporter-fixture --output /tmp/menu-keyboard-evidence
```

Add `--effects` for the effects compositor. Both variants run in CI and retain painted nested-card and overflow captures, producer events, native identities, accessibility trees and logs. Bridge unit tests also verify revision freshness after refresh and equality across uint wrap.
