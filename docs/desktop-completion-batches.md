# Desktop completion batches

Implementation and acceptance are tracked separately. Existing functionality is
verified rather than replaced merely to count it as new work. Personal assets
and machine policy remain in the consuming configuration.

| Group | Work included | Acceptance |
|---|---|---|
| Window consistency | Captured-window actions, integrated green hover menu, alternate fill action, shared layout material, negotiated client decorations | Pending combined build and interaction pass |
| Desktop surfaces | Dock drag cancellation and blank-space release, consistent switcher/layout/pairing material, inactive closing panels cannot intercept input, calendar/notification/control centre review | Pending combined build and visual pass |
| Everyday workflows | Application switching and cancellation, workspace navigation, minimize/restore/fullscreen, file navigation and operations, password-prompt focus and appearance | Existing implementations plus this batch; focused checks pending |
| Application styling | Supported browser/office styling, application-owned dialog boundaries, authentication controls and accessible focus | Supported styling review and fixes in progress |

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
