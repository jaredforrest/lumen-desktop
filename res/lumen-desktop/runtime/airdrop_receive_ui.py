"""Offer a desktop control for the bounded AirDrop receiving window."""

import json
import subprocess
import sys


ON = "Turn On for 10 Minutes"
REFRESH = "Refresh 10 Minutes"
OFF = "Turn Off"


def choices(status):
    if status.get("ready"):
        remaining = max(0, int(status.get("remaining") or 0))
        return [OFF, REFRESH], f"Visible for about {remaining // 60 + 1} more minute(s)"
    return [ON], "AirDrop receiving is off"


def message(kind, body):
    subprocess.run(["zenity", f"--{kind}", "--title=AirDrop Receiving", f"--text={body}"],
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                   timeout=30, check=False)


def main():
    try:
        current = subprocess.run(["iphone-airdrop", "status"], capture_output=True,
                                 text=True, timeout=15, check=True)
        status = json.loads(current.stdout)
        options, description = choices(status)
        selection = subprocess.run(["rofi", "-no-config", "-dmenu", "-no-custom",
                                    "-p", "AirDrop Receiving", "-mesg", description],
                                   input="\n".join(options) + "\n", text=True,
                                   capture_output=True, timeout=60, check=False)
        selected = selection.stdout.strip()
        if selection.returncode or selected not in options:
            return 0
        action = "off" if selected == OFF else "on"
        progress = subprocess.Popen(["zenity", "--progress", "--pulsate", "--no-cancel",
                                     "--title=AirDrop Receiving", "--text=Changing AirDrop receiving…",
                                     "--percentage=5"], stdin=subprocess.PIPE,
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                    text=True)
        try:
            result = subprocess.run(["iphone-airdrop", action], capture_output=True,
                                    text=True, timeout=45, check=False)
        finally:
            if progress.poll() is None:
                progress.terminate()
            progress.wait()
            progress.stdin.close()
        if result.returncode:
            message("error", result.stderr.strip() or "AirDrop receiving could not change state.")
            return result.returncode
        message("info", "AirDrop receiving is off." if action == "off" else
                "AirDrop receiving is on for up to 10 minutes. Normal Wi‑Fi resumes when it ends.")
        return 0
    except (OSError, ValueError, subprocess.TimeoutExpired, subprocess.CalledProcessError) as exc:
        message("error", f"AirDrop receiving could not start: {exc}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
