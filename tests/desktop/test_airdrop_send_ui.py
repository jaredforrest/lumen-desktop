"""The Files sender must target an explicitly selected nearby device."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import time
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[2] / "res/lumen-desktop/runtime/airdrop_send_ui.py"
SPEC = importlib.util.spec_from_file_location("airdrop_send_ui", SOURCE)
ui = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ui)


class SendUiTests(unittest.TestCase):
    def test_streamed_first_peer_opens_without_waiting_for_lookup_deadline(self):
        line = "c6:6b:14:cd:8a:63  -55 dBm  [fe80::1%awdl0]:8770  iPhone"
        started = time.monotonic()
        found = ui.lookup_peers(
            [sys.executable, "-u", "-c", f"import time; print({line!r}); time.sleep(20)"],
            ["sleep", "20"],
        )
        self.assertEqual(found, [("c6:6b:14:cd:8a:63", "iPhone")])
        self.assertLess(time.monotonic() - started, 6)

    def test_peer_rows_and_explicit_choice(self):
        listing = "AirDrop is available\nc6:6b:14:cd:8a:63  -55 dBm  [fe80::1%awdl0]:8770  iPhone\n"
        self.assertEqual(ui.peers(listing), [("c6:6b:14:cd:8a:63", "iPhone")])
        with patch.object(ui, "lookup_peers", return_value=ui.peers(listing)), patch.object(
            ui.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "iPhone  ·  c6:6b:14:cd:8a:63\n", "")
        ):
            self.assertEqual(ui.select_peer(), "c6:6b:14:cd:8a:63")

    def test_cancelled_or_spoofed_choice_never_targets_a_peer(self):
        listing = "c6:6b:14:cd:8a:63  -55 dBm  [fe80::1%awdl0]:8770  iPhone\n"
        with patch.object(ui, "lookup_peers", return_value=ui.peers(listing)), patch.object(
            ui.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "unlisted device\n", "")
        ):
            self.assertIsNone(ui.select_peer())

    def test_batch_uses_one_transfer_and_preserves_each_path(self):
        paths = [Path("/tmp/first file.txt"), Path("/tmp/second.pdf")]
        self.assertEqual(
            ui.send_command(paths, "c6:6b:14:cd:8a:63"),
            ["iphone-airdrop", "send-files", "c6:6b:14:cd:8a:63", *map(str, paths)],
        )
        self.assertEqual(
            ui.send_command(paths[:1], "c6:6b:14:cd:8a:63"),
            ["iphone-airdrop", "send", str(paths[0]), "c6:6b:14:cd:8a:63"],
        )


if __name__ == "__main__":
    unittest.main()
