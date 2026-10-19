# Owned backend recovery acceptance

The notification service, its shell transport, notification producers and the owned shell have independent lifetimes. Service restart restores retained history with a new service instance; restored entries have no live action authority. Producer disappearance removes its actions. Transport reconnect receives a fresh snapshot and does not replay queued commands from the dead transport. Notification controls fail closed while unavailable; unrelated owned controls and widget preferences remain usable.

`notification_recovery_input.py` runs a private D-Bus daemon and native headless compositor with actual production NotificationState, NotificationCard and WidgetState components. Synthetic producer processes publish real notifications and record action signals for their exact protocol ID. The fixture:

- kills a producer and verifies retained history loses action authority;
- stops a transport watcher, queues an action, kills it and verifies a fresh watcher drops the stale command before one new activation;
- kills and restarts the notification service, checks new instance identity and inactive retained entries, and activates an unrelated owned button with a physical key while notifications are unavailable;
- kills only the shell process, checks its orphaned transport exits, then verifies persisted widget layout and notification history after restart and one fresh producer action.

`widget_catalogue_input.py` adds a separate native two-output acceptance case. During an actual held pointer move it kills only the shell process, releases the pointer and restarts the owned components. The last committed widget identity, size and per-output positions remain unchanged; preview geometry is discarded. An independent synthetic document client keeps the same native container/PID and accepts fresh keyboard text. A background click passes through and the restored widget activates once. Existing graceful restart, keyboard placement, resize and removal checks remain in the same fixture.

CI runs both fixtures against both compositor variants. Every process, D-Bus name, state directory, synthetic document and input device belongs to a disposable fixture. No live service, account, notification history or preference is used.

This establishes recovery for the current local Calendar and Local Time widget state plus the notification paths above. These widgets do not implement a separate third-party provider process, so this does not establish arbitrary external widget-provider recovery. Full production shell reconstruction, every open application popup, hung-process recovery, power-loss durability and every search/Dock service failure are separate acceptance work. Host restart supervision is configuration policy; these tests explicitly restart their failed processes.
