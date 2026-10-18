"""Send one Files item by AirDrop with peer choice, progress and cancellation."""

import fcntl
import os
from pathlib import Path
import queue
import re
import selectors
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
            address, name = match.groups()
            if name in (None, "(no response)", "(anonymous)"):
                name = "Nearby device"
            found.append((address.lower(), " ".join(name.split())[:80]))
    return found


def message(kind, text):
    subprocess.run(["zenity", f"--{kind}", "--title=AirDrop", f"--text={text}"],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30,
                   check=False)


def lookup_peers(lookup_command=None, progress_command=None):
    # A named peer can answer well before the 30-second BLE wake window ends.
    # Stream rows so the picker does not wait for every silent nearby device.
    progress = subprocess.Popen(progress_command or
                                ["zenity", "--progress", "--pulsate", "--auto-close",
                                 "--title=AirDrop", "--text=Finding nearby devices…",
                                 "--percentage=5"], stdin=subprocess.PIPE,
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                text=True)
    lookup = subprocess.Popen(lookup_command or ["iphone-airdrop", "peers", "--stream"],
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              text=True, bufsize=1, start_new_session=True)
    found = []
    first_seen = None
    deadline = time.monotonic() + 34
    selector = selectors.DefaultSelector()
    fd = lookup.stdout.fileno()
    os.set_blocking(fd, False)
    selector.register(fd, selectors.EVENT_READ)
    buffered = b""
    try:
        while time.monotonic() < deadline:
            if progress.poll() is not None:
                return None
            events = selector.select(timeout=0.2)
            eof = False
            for _, _ in events:
                chunk = os.read(fd, 4096)
                if not chunk:
                    eof = True
                    break
                buffered += chunk
                while b"\n" in buffered:
                    line, buffered = buffered.split(b"\n", 1)
                    for item in peers(line.decode("utf-8", "replace")):
                        if item not in found:
                            found.append(item)
                            first_seen = first_seen or time.monotonic()
            if eof and buffered:
                for item in peers(buffered.decode("utf-8", "replace")):
                    if item not in found:
                        found.append(item)
                        first_seen = first_seen or time.monotonic()
            if first_seen and time.monotonic() - first_seen >= 1.5:
                break
            if eof or (lookup.poll() is not None and not events):
                break
    finally:
        selector.close()
        if lookup.poll() is None:
            os.killpg(lookup.pid, signal.SIGTERM)
            try:
                lookup.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(lookup.pid, signal.SIGKILL)
                lookup.wait()
        lookup.stdout.close()
        if progress.poll() is None:
            progress.terminate()
        progress.wait()
        progress.stdin.close()
    return found


def select_peer():
    found = lookup_peers()
    if not found:
        message("warning", "No AirDrop device is nearby. Check the iPhone's AirDrop setting and try again.")
        return None
    rows = [f"{name}  ·  {address}" for address, name in found]
    choice = subprocess.run(["rofi", "-no-config", "-dmenu", "-no-custom", "-p", "AirDrop to"],
                            input="\n".join(rows) + "\n", text=True, capture_output=True,
                            timeout=60, check=False)
    if choice.returncode != 0 or choice.stdout.strip() not in rows:
        return None
    return found[rows.index(choice.stdout.strip())][0]


def send_command(paths, address):
    if len(paths) == 1:
        return ["iphone-airdrop", "send", str(paths[0]), address]
    return ["iphone-airdrop", "send-files", address, *map(str, paths)]


def send(paths, address):
    transfer = subprocess.Popen(send_command(paths, address),
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
    paths = [Path(item) for item in argv[1:]]
    if not paths or not all(path.is_file() for path in paths):
        message("error", "Select one or more files in Files before sending by AirDrop.")
        return 2
    runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    lock_fd = os.open(runtime / "lumen-airdrop-send.lock", os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        os.close(lock_fd)
        return 0
    try:
        address = select_peer()
        return send(paths, address) if address else 1
    except (OSError, subprocess.TimeoutExpired) as exc:
        message("error", f"AirDrop could not start: {exc}")
        return 1
    finally:
        os.close(lock_fd)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
