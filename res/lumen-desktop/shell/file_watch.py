"""GIO directory events shared by Files and the local search index."""
import threading
from gi.repository import Gio, GLib


class Watcher:
    def __init__(self, changed):
        self.changed, self.monitors = changed, {}
        self.lock = threading.Lock()
        self.generation = 0
        self.closed = False
        self.loop = GLib.MainLoop()
        self.thread = threading.Thread(target=self.loop.run, daemon=True)
        self.thread.start()

    def set_paths(self, paths, ready=None):
        requested = set(map(str, paths))
        with self.lock:
            if self.closed:return
            self.generation += 1
            generation = self.generation
        def update():
            failed = set()
            with self.lock:
                if self.closed or generation != self.generation:return False
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
                        failed.add(path)
            # A consumer may reconcile the enumeration/registration interval.
            # Call outside the watcher lock; consumer epochs guard late replies.
            if ready is not None:ready(set(self.monitors), failed)
            return False
        GLib.idle_add(update)

    def close(self):
        with self.lock:
            if self.closed:return
            self.closed = True
            self.generation += 1
        def cleanup():
            for monitor in self.monitors.values():monitor.cancel()
            self.monitors.clear()
            self.loop.quit()
            return False
        GLib.idle_add(cleanup)
