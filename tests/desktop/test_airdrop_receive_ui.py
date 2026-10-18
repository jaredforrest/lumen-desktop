"""The receive control must reflect the actual bounded radio state."""

import importlib.util
from pathlib import Path
import unittest


SOURCE = Path(__file__).resolve().parents[2] / "res/lumen-desktop/runtime/airdrop_receive_ui.py"
SPEC = importlib.util.spec_from_file_location("airdrop_receive_ui", SOURCE)
ui = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ui)


class ReceiveUiTests(unittest.TestCase):
    def test_off_offers_only_on(self):
        options, description = ui.choices({"ready": False})
        self.assertEqual(options, [ui.ON])
        self.assertIn("off", description)

    def test_visible_offers_off_and_refresh_with_remaining_time(self):
        options, description = ui.choices({"ready": True, "remaining": 421})
        self.assertEqual(options, [ui.OFF, ui.REFRESH])
        self.assertIn("8 more minute", description)


if __name__ == "__main__":
    unittest.main()
