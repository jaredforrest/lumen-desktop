# Desktop widgets

The supported catalogue contains Calendar and Local Time. Edit Widgets in the notification panel adds instances, configures size and display scope, changes calendar presentation or clock hour format, and removes individual instances. No downloaded code or third-party provider is loaded.

Each instance has a stable identifier. Sizes are small, medium and large; geometry is bounded to the available display. Positions are independent per output connector. Drag the move handle and release to commit, or press Escape to cancel. With the handle focused, Space begins keyboard placement; arrows move, Return commits and Escape cancels. A disconnected display assignment remains explicit.

Layouts are personal session state in the existing widget preferences file. They are not machine policy. Existing visible calendar layouts migrate without losing positions, while an explicitly empty catalogue stays empty across restart. The visible Add controls stop at 16 instances; normalization retains at most 32. Unsupported descriptors are rejected rather than executed.

## Verification and boundaries

`test_widget_model.cjs` checks migration, validation, immutable updates, identities and bounded geometry. `test_widget_catalogue_qml.py` instantiates the actual editor, item and state, verifies cancellation and restores saved instances into a second state object. `widget_catalogue_input.py` uses two disposable native outputs and real pointer/keyboard input to test Add, single activation, background input masks, cancelled and committed movement, independent positions, component identity, size, restart and removal. CI runs it on both compositor variants and retains logs and captures.

Only these two local providers are available. This does not claim a general widget ecosystem, reference-exact packing, collision avoidance, shared account data, or third-party application widget support. Display identity currently follows connector names.
