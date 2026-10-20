# Retained-browser workflow acceptance

This optional corpus runs an explicitly supplied immutable Brave executable. The browser remains the retained application; its interiors and supported commands are tested directly. No replacement browser, injected theme, fake file chooser or reference-exact claim is involved.

## Isolation and dependency contract

The application runner requires Linux user, PID, mount, network, IPC and UTS namespaces. A private filesystem exposes only the immutable store, read-only public source, a fresh writable artifact directory, synthetic configuration and universal null/random devices. Hardware devices, host home/state, host buses and network routes are absent. Only the loopback interface is brought up. The mount namespace pivots to the disposable filesystem and detaches the original root; it does not leave the process in a chroot that would prevent an application from creating its own sandbox namespaces. Capabilities are dropped before applications execute, with non-root identity and no-new-privileges retained. Unsupported isolation fails closed; there is no ambient execution fallback.

The browser argument must identify an immutable executable. Every launch supplies a private `--user-data-dir`; the existing production appearance adapter respects that explicit profile. A pipe-only diagnostic transport may observe browser/page/download state, but it cannot grant permissions, choose files or turn a partial download into a completion. Native input exercises those decisions. Browser sandbox, actual prompts, download/trust policy and overwrite errors remain enabled. Onboarding and default-browser-check exclusions are recorded independently from application workflow acceptance. No real accounts, external websites/stores, audio/camera devices, printers or host profile are used.

## First native corpus

1. Main window and Save As have separate captured surface identities. The real browser command opens its real chooser. Verify exact native parent, frame ownership, destination/name, Cancel and page-focus return.
2. Existing synthetic file: cancel overwrite preserves its hash; explicit overwrite commits the new file before a success claim. Read-only destination keeps an actionable error. A disconnected network-mount case requires an actual disposable mount/provider; absence is reported rather than simulated as an ordinary directory.
3. Independently stream a fixed local download. Observe running UI/provider state; release the server gate, then verify completed UI and exact committed hash. Cancel a second stream through its real UI; no completed record or completed-file claim, with partial cleanup disclosed. Downloads popover and full manager are distinct from Save As.
4. Exercise an actual origin-visible permission decision using a local secure-context page and native input. No DevTools grant/deny overrides or automatic-consent flags are permitted. Each capability is reported separately; no synthetic camera flag is treated as real hardware acceptance.
5. Browser fullscreen and app mode have distinct native identities and dismissal routes, with one frame owner and preserved ordinary-page interaction.

The initial corpus does not close a capability containing several actions from one scenario. Follow-up local cases cover tab lifecycle/groups where supported; address/navigation/draft/error behavior; disposable bookmarks/history and private-profile isolation; self-authored disposable extension lifecycle/popups; print-to-file/PDF/find/zoom and picture-in-picture where supported; external-handler consent using inert local handlers. Each named action receives an observed result or precise unsupported/dependency status. No external extension store, account, remote service or physical device is required or probed.

## Source and unit phase

`nix develop --command desktop-test-python tests/desktop/test_browser_isolation.py` executes the actual namespace boundary without launching a compositor or browser. It proves non-root identity, no effective/bounding capabilities, read-only source, inaccessible outside sentinel, absent hardware, no default route and working private loopback. A real nested user-namespace syscall also succeeds after capability removal, preserving the application sandbox route. It also verifies unrelated environment secrets are omitted. Browser/native acceptance remains pending until its runner executes with explicitly pinned dependencies; a successful isolation unit is not application acceptance.
