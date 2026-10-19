# Window placement

The compositor owns placement and the shell presents the layout menu. The menu
opens after hovering the green control, or with Super+Control+L for the focused
application window. It can be navigated with arrows, Tab, Return and Escape.
Pointer travel from the trigger into the menu has a dismissal grace period.
Actions address the captured numeric container identity, never a window title.

The native command is `lumen_window ACTION`, optionally preceded by Sway criteria.
Supported actions are `close`, `minimize`, `hide`, `unhide`, `fullscreen`, `fill`,
`restore`, `left`, `right`, `top`, `bottom`, `top-left`, `top-right`, `bottom-left`
and `bottom-right`. Full screen and Fill remain separate operations.

Half and quarter actions set an explicit layout. Repeating one is a no-op on
placement rather than a toggle. Changing between layouts retains the original
floating or tiled placement; Restore returns to it. Restore without saved
placement is a no-op. Fill retains its existing reversible toggle behavior.
A client reporting resize increments does not replace a compositor-owned layout.
Floating restoration is constrained to a surviving work area after display
changes, and an emptied original workspace can be recreated.

Runtime configuration accepts `lumen_window margins enable|disable` and
`lumen_window titlebar-action fill|minimize|none`. Settings owns these preferences
and applies them only when the compositor advertises
`window_placement_preferences`. Margins use an explicitly provisional eight
logical pixel gap. Double-clicking unused titlebar space follows the configured
action; the three window controls keep their separate click behavior.

The version response advertises `native_window_layouts`. A shell running with an
older compositor explains that the updated session is needed and disables layout
choices instead of sending unsupported commands. Installing a compositor binary
does not replace the running compositor; it takes effect at the next login.

Full-screen paired placement has a separate native lifecycle and partner chooser;
see [paired-layouts.md](paired-layouts.md). Application-declared preferred-content
Zoom remains an unfinished adapter requirement.

Show Desktop uses `lumen_window show-desktop` (toggle),
`show-desktop-on` and `show-desktop-off` (idempotent). Super+Control+D and the
menu item are enabled only when `native_show_desktop` is advertised. It saves
the visible windows on each output independently of application hiding and
minimisation. Reversing the action restores placement and full-screen state;
activating a revealed window or opening a new application window restores the
saved windows. Previously minimised windows remain minimised.

The nested native tests cover repeated on/off, independent minimisation,
full-screen restoration, activation, and a newly mapped application window on
both compositor variants. Gesture activation and reference timing remain
unverified.
