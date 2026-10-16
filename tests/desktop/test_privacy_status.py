import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("privacy_status", Path(__file__).resolve().parents[2] / "res/lumen-desktop/shell/privacy_status.py")
privacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(privacy)


class PrivacyStatusTests(unittest.TestCase):
    def test_firmware_flags_require_active_enforcement(self):
        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder)
            def write(name, value):
                (directory / f"{name}-{privacy.EFI_GUID}").write_bytes(b"\x07\0\0\0" + bytes([value]))
            self.assertEqual(privacy.secure_boot_status(directory)["state"], "unknown")
            write("SecureBoot", 1); write("SetupMode", 0)
            self.assertEqual(privacy.secure_boot_status(directory)["state"], "enabled")
            write("SetupMode", 1)
            self.assertEqual(privacy.secure_boot_status(directory)["state"], "unknown")
            write("SecureBoot", 0)
            self.assertEqual(privacy.secure_boot_status(directory)["state"], "disabled")
            write("SecureBoot", 2)
            self.assertEqual(privacy.secure_boot_status(directory)["state"], "unknown")

    def test_device_ancestry_and_btrfs_source_suffix(self):
        calls = []
        def run(command):
            calls.append(command)
            if command[0] == "findmnt":
                return {"filesystems": [{"source": "/dev/mapper/main[/persist]"}]}
            return {"blockdevices": [{"type": "lvm", "children": [{"type": "crypt", "children": [{"type": "part"}]}]}]}
        self.assertEqual(privacy.encryption_status(run)["state"], "enabled")
        self.assertEqual(calls[-1][-1], "/dev/mapper/main")

    def test_plain_disk_or_failed_probe_never_reports_encrypted(self):
        for response in ({"blockdevices": [{"type": "part"}]}, {"blockdevices": []}, {},
                         {"blockdevices": [{"type": "lvm", "children": [{"type": "crypt"}, {"type": "part"}]}]}):
            def run(command):
                if command[0] == "findmnt": return {"filesystems": [{"source": "/dev/sda1"}]}
                return response
            self.assertEqual(privacy.encryption_status(run)["state"], "unknown")
        def timeout(command): raise subprocess.TimeoutExpired(command, 4)
        self.assertEqual(privacy.encryption_status(timeout)["state"], "unknown")


if __name__ == "__main__":
    unittest.main()
