"""Consent is explicit and fails closed when the desktop cannot answer."""

import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch


SOURCE = Path(__file__).resolve().parents[2] / "res/lumen-desktop/runtime/airdrop_consent.py"
SPEC = importlib.util.spec_from_file_location("airdrop_consent", SOURCE)
consent = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(consent)


class ConsentTests(unittest.TestCase):
    def test_accept_only_exact_button(self):
        request = {"sender": "iPhone", "files": [{"name": "photo.heic"}]}
        for output, code, expected in [("Accept\n", 0, True), ("Decline\n", 0, False),
                                       ("Accept\n", 1, False), ("", 0, False)]:
            with patch.object(consent.subprocess, "run", return_value=subprocess.CompletedProcess([], code, output, "")):
                self.assertEqual(consent.decide(request), expected)

    def test_timeout_and_missing_ui_decline(self):
        for failure in [OSError("missing"), subprocess.TimeoutExpired("rofi", 40)]:
            with patch.object(consent.subprocess, "run", side_effect=failure):
                self.assertFalse(consent.decide({"sender": "iPhone"}))

    def test_untrusted_metadata_is_escaped_and_bounded(self):
        request = {"sender": "<b>iPhone</b>" + "x" * 200,
                   "files": [{"name": "<bad>.heic"}]}
        with patch.object(consent.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "")) as run:
            consent.decide(request)
        argv = run.call_args.args[0]
        self.assertIn("&lt;b&gt;iPhone&lt;/b&gt;", argv[argv.index("-mesg") + 1])
        self.assertLess(len(argv[argv.index("-mesg") + 1]), 200)
        self.assertEqual(run.call_args.kwargs["input"], "Decline\nAccept\n")


if __name__ == "__main__":
    unittest.main()
