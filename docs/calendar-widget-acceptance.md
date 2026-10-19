# Calendar widget data and activation

The existing CalendarCard presents the calendar application's read-only month
snapshot. Widget interactions issue only list/load requests. Day selection and
calendar-grid arrow navigation change the selected day; an event opens that exact
provider/calendar/event identity in the application. The explicit related-app
link opens the selected date. Non-control card regions do not open an app.
Keyboard users can Tab through controls and activate buttons with Space; arrow
keys navigate the focused date grid. The desktop widget reveals focused controls
inside its constrained scrolling area.

While following Today, a clock day change immediately selects the new day's cached
events. A month change clears the old snapshot until new data arrives; previous-day
appointments are never relabelled as the new date. Explicitly choosing a different
date preserves that selection across clock/month changes. Today resumes following.
Provider failure remains explicit and does not select an unrelated date or invoke
an application. Day, event and related-link presses capture their target identity;
a clock change cannot silently reinterpret a held press as a different date/event.
A model change that destroys or disables a pressed control can cancel activation.

Calendar-zone wall-time strings are formatted independently of the event's date
in the host timezone, avoiding a second conversion through host DST gaps. Backend
expansion still owns timezone conversion and all-day exclusive-end semantics.
Rows retain provider/calendar names in visible metadata and expose full provider,
calendar and calendar-timezone identity in their accessible names.

## Reproduction

```
nix flake check --print-build-logs --option abort-on-warn true
nix develop --command desktop-test-python tests/desktop/calendar_widget_native.py \
  --launcher "$(nix build .#shell --no-link --print-out-paths)/bin/lumen-desktop-shell" \
  --sway "$(nix build .#compositor-basic --no-link --print-out-paths)/bin/sway" \
  --pointer-tool "$(nix build .#test-pointer --no-link --print-out-paths)/bin/wlrctl" \
  --output /tmp/calendar-widget-acceptance
```

CI repeats the native fixture using the effects compositor with `--effects`.
The fixture uses a private HOME, bus, state and compositor and the real desktop
widget. Synthetic local ICS files include a two-day all-day event, overlapping
Los Angeles/New York events, a UTC event, an empty day and a month boundary.
The actual backend expands those files without account discovery or sync. A
delayed read-only adapter exposes stale-data races; a failure adapter checks
unavailable status. Real pointer and Tab/Space/arrow input verifies single-action
identity and related-link reachability. A held day press crosses a synthetic
month change with background activity paused to exercise stable targets.
No personal accounts, credentials, networks or host clock changes are used.
