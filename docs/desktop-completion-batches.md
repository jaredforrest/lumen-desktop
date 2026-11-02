# Desktop completion batches

Implementation and acceptance are tracked separately. Existing functionality is
verified rather than replaced merely to count it as new work. Personal assets
and machine policy remain in the consuming configuration.

| Group | Work included | Acceptance |
|---|---|---|
| Window consistency | Captured-window actions, integrated green hover menu, alternate fill action, shared layout material, negotiated client decorations | Helper identity tests and layout fixture passed; real disposable native fill/minimize/close and retired-target rejection passed. Integrated-control pointer acceptance awaits activation |
| Desktop surfaces | Dock drag cancellation and blank-space release, consistent switcher/layout/pairing material, inactive closing panels cannot intercept input, calendar/notification/control centre review | Expanded physical Dock regression passed; synthetic Dock screenshot reviewed. Final installed multi-panel visual pass awaits activation |
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

## Candidate receipt

Runtime changes are in `f95d0540`; persistent-keyboard fixture correction is in
`f3da557b`, and the application-preservation flake check is in `d7e74c19`.
The desktop package, source contracts, window-policy checks and personalized
system candidate built successfully. Publication changes passed a scoped secret
scan. Activation requires the consuming machine's temporary rebuild grant.

The delayed menu now accepts a captured integrated-control identity. Paired-window
choices remain native-control-only until their picker carries that same captured
identity through dispatch. This is an explicit functional limit.
