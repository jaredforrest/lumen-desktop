# Owned-control accessibility verification

The private native AT-SPI fixture covers these bounded parts of semantic
controls, keyboard input, focus restoration, reading order and assistive input:

- Widget catalogue rows expose a named grouping and type-specific choice/action
  names. Child order follows the visual form. Keyboard changes commit to the
  same descriptor, and removing a row returns focus to Add Calendar.
- Overview group actions accept Return, Space and the AT-SPI Action interface.
  Pointer/Space press captures are consumed once and cancelled captures cleared;
  assistive activation without a press captures the current target. Requests
  retain the selected group, exact window/PID and revision. The native backend
  still validates those facts; this protocol fixture records intent rather than
  claiming native grouping execution. Closing the group dialog returns focus to
  its opener, and hiding Overview closes its nested group dialog.
- Session confirmation exposes a dialog role, action-specific name and a readable
  waiting/final-confirmation description. Waiting is genuinely disabled through
  AT-SPI enabled/sensitive state. Keyboard Cancel clears the pending session
  request without a host action. The separate native session-end fixture verifies
  actual application-owned dirty-document Save, Discard and Cancel.

Notification cards also expose their existing primary/stack activation through
AT-SPI Press rather than advertising an inert button. Launcher preview/grid
controls are keyboard-focusable checkboxes with real checked/enabled state,
visible focus indication and assistive actions. Tab order includes both controls;
Escape closes the launcher while either is focused. These checks use synthetic
notification actions and a no-launch search service, not personal notifications,
clipboard history or host commands.

Launcher list and grid results expose named, selected, focusable actions with
result-key dispatch. Hidden or removed result references cannot retarget another
app. Dock context actions expose menu-item roles, enabled/selected state and
keyboard/assistive activation. Menu invocations capture the application, process
observation identity, native window references and menu generation. A changed
window set invalidates the menu instead of allowing an index to select another
window. Window focus and Close Windows are explicit actions; a destroyed window
never falls through to closing other windows. Queued cooperative Quit also
compares the expected process observation identity at helper dispatch.

The protocol regression deliberately calls retired accessibility references.
Qt may reject these with UnknownObject and a matching missing-path warning;
only paths from those explicit probes are allowed. Other provider warnings
still fail. Actual disposable native windows prove stale selection leaves the
bystander focused and valid actions focus the exact chosen window once. App
launch requests are recorded by a no-launch fixture, not executed on the host.

`lumen-accessibility-protocol-fixture` from
`tests/desktop/accessibility-protocol-rust` runs a private activation-free session
bus, explicitly launched private AT-SPI bus/registry, private runtime and native
headless compositor. It records actual protocol trees, checks keyboard and
assistive actions, and verifies the live accessibility socket is unchanged.
The same fixture retains the existing complete-shell Settings and Files checks.

This is not a claim about third-party application providers, a particular screen
reader's spoken announcements, all assistive devices, or complete accessibility
parity. Contrast, motion preferences, text enlargement and other owned surfaces
have independent requirements and evidence.
