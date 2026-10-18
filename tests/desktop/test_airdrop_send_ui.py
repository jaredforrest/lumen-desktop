"""The Files sender must target an explicitly selected nearby device."""

import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[2] / "res/lumen-desktop/runtime/airdrop_send_ui.py"
SPEC = importlib.util.spec_from_file_location("airdrop_send_ui", SOURCE)
ui = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ui)


class SendUiTests(unittest.TestCase):
    def test_peer_rows_and_explicit_choice(self):
        listing = "AirDrop is available\nc6:6b:14:cd:8a:63  -55 dBm  [fe80::1%awdl0]:8770  iPhone\n"
        self.assertEqual(ui.peers(listing), [("c6:6b:14:cd:8a:63", "iPhone")])
        with patch.object(ui.subprocess, "run", side_effect=[
            subprocess.CompletedProcess([], 0, listing, ""),
            subprocess.CompletedProcess([], 0, "iPhone  ·  c6:6b:14:cd:8a:63\n", ""),
        ]):
            self.assertEqual(ui.select_peer(), "c6:6b:14:cd:8a:63")

    def test_cancelled_or_spoofed_choice_never_targets_a_peer(self):
        listing = "c6:6b:14:cd:8a:63  -55 dBm  [fe80::1%awdl0]:8770  iPhone\n"
        with patch.object(ui.subprocess, "run", side_effect=[
            subprocess.CompletedProcess([], 0, listing, ""),
            subprocess.CompletedProcess([], 0, "unlisted device\n", ""),
        ]):
            self.assertIsNone(ui.select_peer())


if __name__ == "__main__":
    unittest.main()
