"""Exercise the folder desktop-entry adapter without opening user windows."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

HELPER = Path(__file__).resolve().parents[2] / "res/lumen-desktop/open-folder.py"
spec = importlib.util.spec_from_file_location("folder_handoff", HELPER)
handoff = importlib.util.module_from_spec(spec)
spec.loader.exec_module(handoff)


class FolderHandoffTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="folder-handoff-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.folder = self.root / "literal spaces % # ? ü $(not-a-command)"
        self.folder.mkdir()
        self.other = self.root / "second"
        self.other.mkdir()
        self.log = self.root / "calls.jsonl"
        self.stub = self.root / "quickshell"
        self.stub.write_text(
            "#!" + sys.executable + "\nimport json,os,sys\n"
            "with open(os.environ['FOLDER_HANDOFF_LOG'],'a') as log: "
            "log.write(json.dumps(sys.argv[1:])+'\\n')\n"
            "sys.exit(int(os.environ.get('FOLDER_HANDOFF_EXIT','0')))\n"
        )
        self.stub.chmod(0o700)

    def run_helper(self, *folders, code=0):
        env = dict(os.environ, FOLDER_HANDOFF_LOG=str(self.log), FOLDER_HANDOFF_EXIT=str(code))
        return subprocess.run(
            [sys.executable, str(HELPER), "--quickshell", str(self.stub),
             "--shell", "/test/shell", "--", *folders],
            env=env, text=True, capture_output=True,
        )

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def test_local_url_decoded_once_and_multiple_folders_open_separately(self):
        result = self.run_helper(self.folder.as_uri(), str(self.other))
        self.assertEqual(result.returncode, 0, result.stderr)
        prefix = ["-p", "/test/shell", "ipc", "call", "desktop"]
        self.assertEqual(self.calls(), [prefix + ["files", str(self.folder)],
                                      prefix + ["filesNew", str(self.other)]])

    def test_literal_path_preserved_and_no_argument_reopens(self):
        self.assertEqual(self.run_helper(str(self.folder)).returncode, 0)
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertEqual(self.calls()[0][-1], str(self.folder))
        self.assertEqual(self.calls()[1][-2:], ["files", ""])

    def test_localhost_url_and_relative_directory(self):
        self.assertEqual(handoff.directory_path(self.other.as_uri().replace("file:///", "file://localhost/")), str(self.other))
        self.assertEqual(handoff.directory_path(os.path.relpath(self.other)), str(self.other))

    def test_bad_arguments_do_not_partially_dispatch(self):
        regular_file = self.root / "document.txt"
        regular_file.write_text("fixture")
        for bad in ["smb://server/share", "https://example.com", "file://server/share",
                    "file:///tmp/%ZZ", "file:relative", "file:///tmp/%00",
                    self.other.as_uri() + "#fragment", str(regular_file), str(self.root / "missing")]:
            with self.subTest(bad=bad):
                result = self.run_helper(str(self.folder), bad)
                self.assertEqual(result.returncode, 2)
                self.assertIn("Files could not open folder", result.stderr)
                self.assertFalse(self.log.exists())

    def test_shell_failure_stops_without_fallback(self):
        self.assertEqual(self.run_helper(str(self.folder), str(self.other), code=17).returncode, 17)
        self.assertEqual(len(self.calls()), 1)

    def test_gio_desktop_entry_expands_urls_into_separate_literal_arguments(self):
        try:
            import gi
            gi.require_version("GioUnix", "2.0")
            from gi.repository import Gio, GioUnix
        except ImportError:
            self.skipTest("PyGObject required for desktop entry launch integration")
        desktop = self.root / "test-folder.desktop"
        desktop.write_text(
            "[Desktop Entry]\nType=Application\nName=Folder handoff fixture\n"
            f"Exec={sys.executable} {HELPER} --quickshell {self.stub} --shell /test/shell -- %U\n"
            "MimeType=inode/directory;\nTerminal=false\n"
        )
        app = GioUnix.DesktopAppInfo.new_from_filename(str(desktop))
        self.assertIsNotNone(app)
        context = Gio.AppLaunchContext()
        context.setenv("FOLDER_HANDOFF_LOG", str(self.log))
        self.assertTrue(app.launch_uris([self.folder.as_uri(), self.other.as_uri()], context))
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if self.log.exists():
                try:
                    if len(self.calls()) == 2:
                        break
                except json.JSONDecodeError:
                    pass
            time.sleep(.02)
        self.assertTrue(self.log.exists())
        self.assertEqual([call[-2:] for call in self.calls()],
                         [["files", str(self.folder)], ["filesNew", str(self.other)]])


if __name__ == "__main__":
    unittest.main()
