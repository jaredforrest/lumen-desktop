# Desktop completion batches

Implementation and acceptance are tracked separately. Existing functionality is
verified rather than replaced merely to count it as new work. Personal assets
and machine policy remain in the consuming configuration.

## Demonstration baseline

The release target is a polished ordinary desktop, not a minimum feature list.
Build success and component assertions are prerequisites; neither is visual acceptance.
The same candidate must be reviewed as complete screens on a normal wallpaper,
in light and dark appearance. A diagnostic checkerboard supplements that review
for transparent padding and blur leaks. Isolated graphical sessions allow this
work without restarting a user's running desktop.

Release-blocking visible defects include rough or doubled corners, blur outside
the painted panel, inconsistent or doubled title bars, clipped ordinary labels,
unreadable text over wallpaper, leftover overlays, hover freezes and missing
normal ways to dismiss a surface or leave fullscreen. Everyday acceptance covers
launch/search, app switching, file navigation and preview, Save/Open, the Dock,
calendar and notifications, settings, password prompts and window controls.
Rare failure and unusual hardware cases follow this baseline.

Window Overview opens around search and the desktop/window list. The explicit
Manage Desktops disclosure contains creation, removal, ordering, persistent
assignment and display movement controls. Its disclosure must work with the
keyboard; opening it alone must not send a management action. Direct window drag
still exposes its destination controls during the drag workflow.

The current visual batch fixes full selected-date wrapping and gives the empty
notification status and widget-edit action their own readable surfaces. The
isolated smoke configuration now uses the shipped Dock alpha mask rather than
the obsolete full-buffer rounded blur rule. It captures all four Files views
and the calendar over the normal wallpaper, in addition to diagnostic captures.
These receipts do not certify an unobserved third-party application interior or
the renderer in an older, still-running graphical session.

| Group | Work included | Acceptance |
|---|---|---|
| Window consistency | Captured-window actions, integrated green hover menu, alternate fill action, shared layout material, negotiated client decorations | Helper identity tests and layout fixture passed. Installed disposable Files window passed physical green hover/fullscreen, alternate-click fill, minimize, restore and close |
| Desktop surfaces | Dock drag cancellation and blank-space release, consistent switcher/layout/pairing material, inactive closing panels cannot intercept input, calendar/notification/control centre review | Expanded physical Dock regression passed. Installed Files, layout menu, Dock, control centre, calendar/notifications and search screenshots reviewed together |
| Everyday workflows | Application switching and cancellation, workspace navigation, minimize/restore/fullscreen, file navigation and operations, password-prompt focus and appearance | Packaged application switching, cancellation, modifier release and same-app cycling passed. Shared authentication scenario passed; unchanged file/workspace implementations were retained |
| Application styling | Supported browser/office styling, application-owned dialog boundaries, authentication controls and accessible focus | Application preference preservation and inspection tests passed. Shared authentication views reviewed at constrained sizes. Third-party interior fidelity is not established by these tests |

## Acceptance boundaries

- A successful build does not establish appearance or input behaviour.
- Native window controls and integrated controls must target their captured window.
- A negotiated client title bar is retained for client-decorated Save/Open dialogs;
  adding a compositor title bar above it is a regression.
- Main window geometry belongs to the compositor; layer panels own their material
  mask and transparent padding. Do not round the complete transparent layer buffer.
- Fullscreen, tiled and floating surfaces are checked independently. A single
  rounded screenshot does not establish every case.
- Third-party interiors use their supported preferences. Unavailable customization
  remains explicit, rather than adding fragile browser or application patches.
- Screenshots containing personal content remain local.

## Delivery

Related fixes are assembled before the package build and focused regression checks.
One combined visual pass reviews the changed surfaces. Activation uses the clean,
committed consuming configuration, and unrelated pending work is restored exactly.

## Earlier activation receipt

Runtime changes are in `f95d0540`; persistent-keyboard fixture correction is in
`f3da557b`, and the application-preservation flake check is in `d7e74c19`.
The desktop package, source contracts, window-policy checks and personalized
system candidate built successfully. Publication changes passed a scoped secret
scan. Activation requires the consuming machine's temporary rebuild grant.

Activated on 2026-11-03 through consuming revision `e10b730`. The desktop service
is active with no failed user units. All 204 unrelated pending configuration paths
and their staged/unstaged patches were restored byte-for-byte. Disposable test
windows were closed and temporary input devices were released and removed.

That installed visual review showed a wrapped Appearance tile label at the
current control-centre width. Calendar data loading is not certified by a surface
review. These remain open; the installed receipt is not a complete parity claim.

The delayed menu now accepts a captured integrated-control identity. Paired-window
choices remain native-control-only until their picker carries that same captured
identity through dispatch. This is an explicit functional limit.
