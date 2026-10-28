# Desktop file captions and keyboard layout

The optional desktop surface retains consumer-selected paths and normalized
placement metadata. Caption text follows the shared metadata typography role;
actual shaped caption capacity and two-line height determine each tile's client
and input-mask bounds. Optical file symbols remain 46 pixels. Default text size
retains the 108-by-100 tile. Font changes do not rewrite normalized placements.

Arrow keys choose a visible neighbour in the requested geometric direction,
prefer aligned entries, and use stable layout keys to resolve ties. Collapsed
stack children and overflow entries cannot receive directional selection.
Physical stack activation toggles that captured stack without opening a file.
Quick Look steps through visible file entries only.

Placement pruning waits for a successful current Files listing committed after
its rows and history presentation. A stale reply, error, disconnected transport,
changed path, limited listing or unreadable-item count never authorizes pruning.
Partial rows remain available for display. Placement edits wait for a complete
listing; failed refreshes preserve the last durable placement snapshot.

`tests/desktop/desktop_typography_native.py` exercises the production surface
against disposable real files in the full application namespace. It accepts the
same pinned launcher, compositor, physical keyboard, pointer, fontconfig and
software-driver arguments as `overview_typography_native.py`; add `--effects`
for the effects compositor. CI runs both through `tests/ci/run-native.sh` and
retains desktop-typography-native artifacts. The corpus covers four text scales,
light/dark presentation, normal/constrained outputs, actual caption ink and input
masks, directional navigation, stack/Quick Look dispatch, listing failure and a
fresh shell/provider restart preserving four placements. The Quick Look receiver
records an intent; this fixture does not claim external preview functionality or
reference-platform measurement parity.
