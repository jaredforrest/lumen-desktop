"""Send one Files item by AirDrop with peer choice, progress and cancellation."""

import os
from pathlib import Path
import queue
import re
import signal
import subprocess
import sys
import threading
import time


PEER = re.compile(r"^([0-9a-f]{2}(?::[0-9a-f]{2}){5})\s+.*?\]:8770(?:\s+(.*))?$", re.I)


def peers(output):
    found = []
    for line in output.splitlines():
        match = PEER.match(line)
        if match:
            mac, name = match.groups()
            if name in (None, "(no response)", "(anonymous)"):
                name = "Nearby device"
            found.append((mac.lower(), " ".join(name.split())[:80]))
    return found


def message(kind, text):
    subprocess.run(["zenity", f"--{kind}", "--title=AirDrop", f"--text={text}"],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30,
                   check=False)


def select_peer():
    last = ""
    for _ in range(3):
        result = subprocess.run(["iphone-airdrop", "peers"], text=True,
                                capture_output=True, timeout=45, check=False)
        last = result.stderr or result.stdout
        found = peers(result.stdout)
        if found:
            break
        time.sleep(2)
    else:
        message("warning", "No AirDrop device is nearby. Check the iPhone's AirDrop setting and try again.")
        return None
    rows = [f"{name}  ·  {mac}" for mac, name in found]
    choice = subprocess.run(["rofi", "-no-config", "-dmenu", "-no-custom", "-p", "AirDrop to"],
                            input="\n".join(rows) + "\n", text=True, capture_output=True,
                            timeout=60, check=False)
    if choice.returncode != 0 or choice.stdout.strip() not in rows:
        return None
    return found[rows.index(choice.stdout.strip())][0]


def send(path, mac):
    transfer = subprocess.Popen(["iphone-airdrop", "send", str(path), mac],
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, start_new_session=True)
    progress = subprocess.Popen(["zenity", "--progress", "--pulsate", "--auto-close",
                                 "--title=AirDrop", "--text=Sending…", "--percentage=5"],
                                stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL, text=True)
    lines = queue.Queue()

    def read_output():
        for line in transfer.stdout:
            lines.put(line.rstrip())

    threading.Thread(target=read_output, daemon=True).start()
    tail = []
    cancelled = False
    try:
        while transfer.poll() is None or not lines.empty():
            if progress.poll() is not None and transfer.poll() is None:
                cancelled = True
                os.killpg(transfer.pid, signal.SIGTERM)
                break
            try:
                line = lines.get(timeout=0.2)
            except queue.Empty:
                continue
            tail = (tail + [line])[-12:]
            stage = ("30\n# Finding device\n" if "DISCOVER" in line else
                     "60\n# Waiting for acceptance\n" if "ASK" in line else
                     "90\n# Uploading\n" if "UPLOAD" in line else None)
            if stage and progress.poll() is None:
                try:
                    progress.stdin.write(stage)
                    progress.stdin.flush()
                except BrokenPipeError:
                    cancelled = True
                    os.killpg(transfer.pid, signal.SIGTERM)
                    break
        result = transfer.wait(timeout=5)
    finally:
        if progress.poll() is None:
            try:
                progress.stdin.write("100\n" if not cancelled and transfer.returncode == 0 else "# Stopped\n")
                progress.stdin.flush()
            except (BrokenPipeError, OSError):
                pass
            progress.terminate()
    if cancelled:
        return 130
    if result:
        detail = next((line for line in reversed(tail) if "declined" in line.lower()), "Transfer failed. Check AirDrop and try again.")
        message("error", detail)
    return result


def main(argv):
    if len(argv) != 2 or not Path(argv[1]).is_file():
        message("error", "Select one file in Files before sending by AirDrop.")
        return 2
    try:
        mac = select_peer()
        return send(Path(argv[1]), mac) if mac else 1
    except (OSError, subprocess.TimeoutExpired) as exc:
        message("error", f"AirDrop could not start: {exc}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
