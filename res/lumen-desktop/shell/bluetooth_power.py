"""Per-adapter radio blocks, without changing Wi-Fi or other Bluetooth adapters."""
from pathlib import Path
import re
import subprocess


def status(root=Path("/sys/class/rfkill")):
    radios = {}
    for item in root.glob("rfkill*"):
        try:
            if item.joinpath("type").read_text().strip() != "bluetooth":
                continue
            name = item.joinpath("name").read_text().strip()
            if not re.fullmatch(r"hci[0-9]+", name) or not re.fullmatch(r"rfkill[0-9]+", item.name):
                continue
            soft, hard = (item.joinpath(key).read_text().strip() for key in ("soft", "hard"))
            if soft not in ("0", "1") or hard not in ("0", "1"):
                continue
            radios["/org/bluez/" + name] = {"id": item.name[6:], "soft": soft == "1", "hard": hard == "1"}
        except OSError:
            continue
    return radios


def prepare_enable(path, radios=None, run=subprocess.run):
    if not re.fullmatch(r"/org/bluez/hci[0-9]+", path):
        raise RuntimeError("Invalid Bluetooth adapter")
    radio = (status() if radios is None else radios).get(path)
    if radio and radio["hard"]:
        raise RuntimeError("Bluetooth is blocked by a hardware switch. Turn the switch on first.")
    if radio and radio["soft"]:
        try:
            run(["rfkill", "unblock", radio["id"]], check=True, capture_output=True, text=True, timeout=4)
        except (OSError, subprocess.SubprocessError) as error:
            raise RuntimeError("Bluetooth could not be turned on. Check your radio controls or try Bluetooth Settings.") from error
