# Text composition ownership

The focused editor owns an active input-method composition. Owned launcher and
window navigation handlers yield Enter, Escape and arrows while that editor's
`inputMethodComposing` is true. Group-switch input also yields to the editor;
an AltGr character is not an application shortcut. This uses the actual focused
editor and its ancestors, not a timer or a separately maintained composition flag.

The shared policy covers the launcher, Settings search, Files filtering and
ancestor shortcuts, Overview, Calendar editing and desktop naming. Escape/help
shortcuts in tooltips and window shortcuts in Help, remote connections and Text
Tools are disabled only while the focused editor composes. Desktop reordering
shortcuts also yield during composition. Other ordinary text controls retain Qt's
native input-method handling. Owned single-line controls share `DesktopTextField`:
while composing, Return requests the input method to commit without also emitting
the form's `accepted` action, and Escape requests a composition reset. Otherwise
the original Qt editing and acceptance behavior is inherited. No accepted signal
is simulated. The native fixture also exercises the real network-address form
against a disposable in-process counter, with no network connection attempted.

The scoped Qt Wayland client runtime omits cursor/format attributes for an empty
text-input-v3 preedit. The pinned provider otherwise leaves Qt Quick's composition
state true after the committed text arrives. Nonempty preedit formatting remains
unchanged. This changes the desktop runtime, not the consumer's system Qt.

`lumen-input-composition-fixture` exercises the actual production launcher with
a disposable Wayland input-method-v2 provider and text-input-v3 client path.
The provider sends real Japanese preedit and Unicode commits, owns actual candidate
Enter/Escape/arrows through a keyboard grab, and releases the grab before ordinary
navigation. An additional ungrabbed preedit case exercises the owned handlers
directly, including a visible owned tooltip. A forwarding observer records real
editor key delivery without accepting the event or replacing its handler.

The physical keyboard uses a German XKB map for dead-acute composition and
RightAlt/Mod5+Q. The expected `@` must reach the editor without Ctrl/Alt command
modifiers or a configured Ctrl+Alt action. Qt may consume GroupSwitch during
level-three translation; the fixture retains both actual editor modifiers and
the acknowledged physical keyboard command transcript. The same Ctrl+Alt+Q action must run in a
separate positive control. Fixtures use private state, a private bus and headless
compositor; no host seat, clipboard, account or audio service is used.

These checks address T26-INPUT-009 and B-INP-08/09/10. They do not certify every
input-method implementation, candidate window, language or remotely translated
keymap. Real user-selected input methods remain an integration boundary. The
fixture records exact tools, copied assets, observed text/state and owned child
identities, then checks that owned children have been reaped.

The native CI backend loop runs all nine cases on both compositor providers and
retains `artifacts/input-composition-*`, including the keyboard command/ack
transcript, actual client memory maps and final receipt. The modal form case also
checks that composing Escape does not dismiss its real `CloseOnEscape` dialog.
Ordinary tooltip Escape keeps its existing dismissal behavior.

Two separate causal negatives accompany the initial acceptance: prior launcher
handlers move selection on the first delivered Down while preedit remains active;
the prior scoped Qt library commits the exact Unicode string but retains a true
composition flag with an empty preedit. The latter is distinguished using the
actual loaded library mapping. CI exercises the corrected current runtime; it does
not rebuild historical variants on every run. Actual selected input-method and
remote-client integration limits remain as described above.
