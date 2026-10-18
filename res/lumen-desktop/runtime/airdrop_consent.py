"""Ask the desktop user before accepting an incoming AirDrop transfer.

The receiver passes only display metadata on stdin.  A missing desktop, a
closed dialog, or a timeout is a decline; this program never accepts silently.
"""

import json
import html
import os
import subprocess
import sys


def label(value, limit=80):
    text = value if isinstance(value, str) else ""
    text = " ".join(text.split())
    return "".join(ch for ch in text if ch.isprintable())[:limit]


def summary(request):
    sender = label(request.get("sender")) or "Nearby device"
    files = request.get("files")
    files = files if isinstance(files, list) else []
    names = [label(item.get("name")) for item in files[:3] if isinstance(item, dict)]
    names = [name for name in names if name]
    count = len(files)
    if count > 3:
        names.append(f"and {count - 3} more")
    if not names:
        items = request.get("items")
        items = items if isinstance(items, list) else []
        names = [label(item) for item in items[:3]]
        names = [name for name in names if name]
        if len(items) > 3:
            names.append(f"and {len(items) - 3} more")
    if not names:
        names = ["a shared item"]
    return f"{sender} wants to send {', '.join(names)}"


def decide(request, rofi="rofi"):
    if not isinstance(request, dict):
        return False
    try:
        result = subprocess.run(
            [rofi, "-no-config", "-dmenu", "-no-custom", "-format", "s",
             "-p", "AirDrop", "-mesg", html.escape(summary(request))],
            input="Decline\nAccept\n", text=True, capture_output=True,
            timeout=40, check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return False
    return result.returncode == 0 and result.stdout.strip() == "Accept"


def main():
    try:
        request = json.load(sys.stdin)
    except (ValueError, UnicodeError):
        return 1
    if decide(request, os.environ.get("LUMEN_AIRDROP_ROFI", "rofi")):
        print("accept")
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
