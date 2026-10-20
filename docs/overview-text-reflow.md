# Overview text and reachability

Overview title, desktop metadata, instructions, group helper and drag-caption text use shared roles. Grid columns account for text scale; card height reserves measured title, metadata and group-handle areas. The search baseline is12px, matching the original public font measurement. At100% existing geometry remains unless a real control already exceeds its former fixed band, such as a wrapped display name.

The body scrolls below the fixed header. Command bands wrap using actual control metrics; footer focus reveals its destination through the shared viewport. Modal dialogs remain attached to the actual window rather than to scrolled content. DesktopStrip text already scaled; its caption capacity and row now honor those metrics. Desktop ordering, group captures and action dispatch are unchanged.

The isolated software corpus uses the production Overview and a synthetic read-only status provider. It paints sixteen mode/size/text-scale combinations in three phases: window card, focused footer and modal group body. The modal capture is its real QML body, since the window's C++ root cannot be grabbed with a QML engine. Actual font, capacity, overlap, focused bounds and foreground ink are checked; model calls are not physical input proof.

The native fixture uses two synthetic window identities, a strict inert focus recorder and both compositor variants. It physically selects the second filtered window, traverses the footer and group dialog, dismisses the dialog before the window and scrolls with a virtual pointer. Font resolution and actual focused-label pixels are required after authoritative focus/geometry readiness. No host application or desktop command is executed; no reference measurements or external application parity are claimed.
