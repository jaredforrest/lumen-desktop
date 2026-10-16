"""GIO directory events shared by Files and the local search index."""
import threading
from gi.repository import Gio, GLib


class Watcher:
    def __init__(self, changed):
        self.changed, self.monitors = changed, {}
        self.loop = GLib.MainLoop()
        threading.Thread(target=self.loop.run, daemon=True).start()

    def set_paths(self, paths):
        requested = set(map(str, paths))
        def update():
            for path in list(self.monitors):
                if path not in requested:
                    self.monitors.pop(path).cancel()
            for path in requested - self.monitors.keys():
                try:
                    monitor = Gio.File.new_for_path(path).monitor_directory(Gio.FileMonitorFlags.WATCH_MOVES, None)
                    monitor.set_rate_limit(150)
                    monitor.connect('changed', lambda m, f, other, event, directory=path: self.changed(directory))
                    self.monitors[path] = monitor
                except GLib.Error:
                    pass
            return False
        GLib.idle_add(update)

    def close(self):
        self.set_paths([])
        self.loop.quit()
