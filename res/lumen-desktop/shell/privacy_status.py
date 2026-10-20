"""Read-only security facts. Unknown evidence must never become an 'enabled' badge."""
import json
from pathlib import Path
import subprocess
import sys

EFI_GUID = "8be4df61-93ca-11d2-aa0d-00e098032b8c"


def command_json(command):
    result = subprocess.run(command, capture_output=True, text=True, timeout=4, check=True)
    return json.loads(result.stdout)


def encryption_status(run=command_json):
    try:
        mounts = run(["findmnt", "--json", "--mountpoint", "/persist", "--output", "SOURCE,FSTYPE"])
        source = mounts["filesystems"][0]["source"].split("[", 1)[0]
        if not source.startswith("/dev/"):
            raise ValueError("Not a block device")
        devices = run(["lsblk", "--json", "--inverse", "--tree", "--output", "TYPE", source])
        def encrypted(items):
            # Every backing path must cross encryption. One encrypted RAID/LVM leg
            # does not establish protection for the other legs.
            return bool(items) and all(item.get("type") == "crypt" or encrypted(item.get("children", [])) for item in items)
        if not devices.get("blockdevices"):
            raise ValueError("No device evidence")
        if encrypted(devices["blockdevices"]):
            return {"state": "enabled", "value": "Encrypted", "detail": "Persistent storage is mounted through an encrypted device. Other disks and removable drives are separate."}
        return {"state": "unknown", "value": "Not confirmed", "detail": "No active device-encryption layer was found for persistent storage. Other forms of encryption are not assessed here."}
    except (OSError, ValueError, KeyError, IndexError, TypeError, subprocess.SubprocessError):
        return {"state": "unknown", "value": "Unavailable", "detail": "The encryption status of persistent storage could not be read."}


def secure_boot_status(directory=Path("/sys/firmware/efi/efivars")):
    def flag(name):
        data = (directory / f"{name}-{EFI_GUID}").read_bytes()
        if len(data) != 5 or data[4] not in (0, 1):
            raise ValueError("Invalid firmware flag")
        return data[4]
    try:
        enabled, setup = flag("SecureBoot"), flag("SetupMode")
        if enabled and not setup:
            return {"state": "enabled", "value": "On", "detail": "Firmware reports Secure Boot is enabled for this boot."}
        if not enabled:
            return {"state": "disabled", "value": "Off", "detail": "Firmware reports Secure Boot is disabled."}
        raise ValueError("Inconsistent firmware state")
    except (OSError, ValueError):
        return {"state": "unknown", "value": "Unavailable", "detail": "Firmware status could not be read. A configured bootloader alone does not confirm Secure Boot is enabled."}


def biometric_status(run=subprocess.run):
    try:
        result = run([sys.executable, str(Path(__file__).with_name('biometric_status.py'))],
                     capture_output=True, text=True, timeout=2.5, check=True)
        if len(result.stdout) > 4096:
            raise ValueError('Oversized result')
        value = json.loads(result.stdout)
        if (set(value) != {'state', 'value', 'detail'} or value['state'] not in ('unknown', 'reported', 'unavailable')
                or any(not isinstance(value[k], str) or len(value[k]) > 1024 for k in value)):
            raise ValueError('Invalid result')
        return value
    except (OSError, ValueError, TypeError, subprocess.SubprocessError):
        return {'state':'unknown', 'value':'Unavailable', 'detail':'Fingerprint reader status could not be established. Authentication remains managed by the host.'}


def snapshot():
    return {"encryption": encryption_status(), "secureBoot": secure_boot_status(), "biometrics": biometric_status()}


if __name__ == "__main__":
    print(json.dumps(snapshot()))
