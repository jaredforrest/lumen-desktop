import sys
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "res/lumen-desktop/shell"))
import bluetooth_power as power


class RadioPowerTests(unittest.TestCase):
    def test_only_requested_software_block_is_removed(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for index, (kind, name, soft, hard) in enumerate((("bluetooth", "hci0", "1", "0"), ("wlan", "phy0", "1", "0"), ("bluetooth", "hci1", "1", "1"))):
                device = root / ("rfkill" + str(index)); device.mkdir()
                for key, value in dict(type=kind, name=name, soft=soft, hard=hard).items():
                    (device / key).write_text(value)
            radios = power.status(root)
            self.assertEqual(set(radios), {"/org/bluez/hci0", "/org/bluez/hci1"})
            run = Mock()
            power.prepare_enable("/org/bluez/hci0", radios, run)
            self.assertEqual(run.call_args.args[0], ["rfkill", "unblock", "0"])
            run.reset_mock()
            with self.assertRaisesRegex(RuntimeError, "hardware switch"):
                power.prepare_enable("/org/bluez/hci1", radios, run)
            run.assert_not_called()

    def test_unknown_and_enabled_radios_do_not_unblock_everything(self):
        run = Mock()
        power.prepare_enable("/org/bluez/hci0", {}, run)
        power.prepare_enable("/org/bluez/hci0", {"/org/bluez/hci0":dict(id="0",soft=False,hard=False)}, run)
        run.assert_not_called()
        with self.assertRaises(RuntimeError):
            power.prepare_enable("/org/bluez/hci0/dev_00_11_22_33_44_55", {}, run)


if __name__ == "__main__": unittest.main()
