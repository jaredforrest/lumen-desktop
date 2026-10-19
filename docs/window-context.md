# Window context and document ownership

Keyboard focus, native parenthood, application presentation grouping and the
main document association are different observations.

The compositor advertises `native_window_relationships: true` and includes
`lumen_relationships` version 1 in each window. `parent_supported` reports whether
the native parent protocol can be observed. `parent` is the current mapped direct
parent container ID, or null. `modal` is the actual xdg-dialog state, or null when
that window has no dialog object. An absent extension is unknown, including for
older backends. A null parent does not assert a document role.

The standard xdg-dialog manager is instantiated on both supported backends.
Per-seat compositor focus redirects a parent target to its live modal descendant;
it does not block sibling documents or other applications. Plain utility parents
do not redirect focus. Ambiguous sibling modal requests refuse a parent focus
change rather than selecting an arbitrary child. Dynamic parent and modal signals
refresh workspace arrangement and apply the focus rule to current seats. Dialog
objects and mapped views have separately cleaned-up listener lifetimes.

The title-free runtime snapshot exposes parent/modal facts and a separate
`context_window`/`application_hint` derived from live native ancestry. Advertised
application IDs are not authenticated package identities. It never invents a main
document from parenthood, a title or most recent focus. `contexts` can accept
independent adapter declarations with live window IDs; these are not an external
command channel and grant no destructive document authority.

Owned Files windows explicitly register QObject references as main windows.
Quick Look and File Info register their actual owning Files instance as their
main association while independently becoming the key utility window. Returning
to another application clears the owned key recipient but keeps the document
association. Returning to a different Files window changes the main association;
returning to its utility restores that utility's explicit owner. Closing the owner
invalidates its main association. Files menu commands use this owning instance.
These annotations affect presentation and document routing within the owned app;
they do not authorize Force Quit, close-all or external process control.

The shell correlates the active native toplevel handle with the owned window's
actual Qt active state before retaining a presentation grouping association.
This lets an observed owned utility remain in its owning Files Dock/switcher group
when inactive. Title equality is only an additional race guard, not the source of
its role. Destroyed handles and invisible owners are excluded. The menu bridge
exposes parent application context separately from its exact key-window exporter;
its existing exporter PID and focused token guards remain unchanged.

## Verification and limits

`test_window_context.py` covers independent main/key association, graph cycles,
ambiguous modal siblings, stale adapter declarations and strictly typed facts.
`window_relationships_input.py` uses a disposable real Wayland client and virtual
keyboard in an isolated compositor: utility/document/other-app recipients, modal
scope, dynamic set/unset/reparent/unparent, dialog-object destruction and live
orphan utility after parent exit. `owned_window_context_input.py --full-shell`
exercises real Files, Quick Look and inspector windows, routing a New Tab command
from a key utility to the associated document and retaining its application group.
CI runs native integration on both compositor backends.

Unreported external application main documents remain unknown. Native parenthood
does not supply document-command semantics, dirty state or an application menu.
XWayland modal/parent adaptation and native popup surface introspection are not
implemented by this extension. Toolkit popup surfaces are not separate managed
toplevels and do not create a new application context here. The runtime remains a
sequential read-only observation rather than an atomic focus broker. Owned app
associations are in-memory and are rebuilt by actual owned window focus after a
shell restart; no stale QObject or container identity is persisted.
