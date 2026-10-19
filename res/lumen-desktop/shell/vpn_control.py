"""Small, bounded bridge between the desktop VPN panel and installed clients.

The protocol is one JSON request on stdin and one JSON response on stdout.
Account numbers are never returned or written to disk by this bridge.
"""

import json
import re
import shutil
import subprocess
import sys


TIMEOUT = 8


def command(argv, *, input_text=None, timeout=TIMEOUT):
    if not shutil.which(argv[0]):
        raise RuntimeError(f"{argv[0]} is not installed")
    try:
        result = subprocess.run(argv, input=input_text, text=True, capture_output=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(f"{argv[0]} did not respond") from exc
    if result.returncode:
        message = (result.stderr or result.stdout).strip().splitlines()
        raise RuntimeError(message[-1][:240] if message else f"{argv[0]} failed")
    return result.stdout


def mullvad_status():
    status = {"installed": bool(shutil.which("mullvad")), "state": "unavailable", "account": False,
              "location": "", "relay": ""}
    if not status["installed"]:
        return status
    try:
        data = json.loads(command(["mullvad", "status", "--json"]))
        status["state"] = data.get("state", "unknown")
        location = (data.get("details") or {}).get("location") or {}
        status["location"] = ", ".join(part for part in (location.get("city"), location.get("country")) if part)
    except (RuntimeError, ValueError, TypeError) as exc:
        status["error"] = str(exc)
    try:
        output = command(["mullvad", "account", "get"])
        status["account"] = "Account number:" in output
    except RuntimeError as exc:
        if "Not logged in" not in str(exc):
            status["accountError"] = "Account status unavailable"
    try:
        output = command(["mullvad", "relay", "get"])
        match = re.search(r"^\s*Location:\s*(.+)$", output, re.MULTILINE)
        if match:
            status["relay"] = match.group(1).strip()
    except RuntimeError:
        pass
    return status


def tailscale_status():
    status = {"installed": bool(shutil.which("tailscale")), "state": "unavailable", "online": False,
              "peers": 0, "tailnet": "", "exitNode": "", "exitNodes": [], "health": []}
    if not status["installed"]:
        return status
    try:
        data = json.loads(command(["tailscale", "status", "--json"]))
        status["state"] = data.get("BackendState", "unknown")
        status["online"] = bool((data.get("Self") or {}).get("Online")) and status["state"] == "Running"
        status["tailnet"] = (data.get("CurrentTailnet") or {}).get("Name", "")
        peers = data.get("Peer") or {}
        status["peers"] = sum(bool(p.get("Online")) for p in peers.values())
        exit_status = data.get("ExitNodeStatus") or {}
        status["exitNode"] = exit_status.get("ID", "")
        status["exitNodes"] = sorted(
            ({"id": p.get("ID", ""), "name": (p.get("DNSName") or p.get("HostName") or "Exit node").rstrip(".")}
             for p in peers.values() if p.get("ExitNodeOption") and p.get("ID")),
            key=lambda node: node["name"].lower(),
        )
        status["health"] = [str(item)[:180] for item in (data.get("Health") or [])][:3]
    except (RuntimeError, ValueError, TypeError) as exc:
        status["error"] = str(exc)
    return status


def status():
    return {"mullvad": mullvad_status(), "tailscale": tailscale_status()}


def tailscale_action(args):
    try:
        command(["tailscale", *args], timeout=25)
    except RuntimeError as exc:
        # On Linux, tailscaled normally requires root or a configured operator.
        # Authenticate only for a user-requested action and only on permission errors.
        if not re.search(r"access denied|permission denied|not authorized|requires root", str(exc), re.I):
            raise
        binary = shutil.which("tailscale")
        if not binary or not shutil.which("pkexec"):
            raise
        command(["pkexec", binary, *args], timeout=90)


def perform(request):
    action = request.get("action", "status")
    if action == "status":
        return status()
    if action == "mullvad-connect":
        command(["mullvad", "connect"], timeout=30)
    elif action == "mullvad-disconnect":
        command(["mullvad", "disconnect"], timeout=30)
    elif action == "mullvad-login":
        account = request.get("account", "")
        if not isinstance(account, str) or not re.fullmatch(r"\d{16}", account):
            raise RuntimeError("Enter a 16-digit Mullvad account number")
        try:
            command(["mullvad", "account", "login"], input_text=account + "\n", timeout=30)
        except RuntimeError as exc:
            raise RuntimeError(str(exc).replace(account, "[account number]")) from exc
    elif action == "mullvad-location":
        country = request.get("country", "")
        if not isinstance(country, str) or not re.fullmatch(r"[a-z]{2}|any", country):
            raise RuntimeError("Choose a valid country")
        command(["mullvad", "relay", "set", "location", country])
    elif action == "tailscale-connect":
        tailscale_action(["up"])
    elif action == "tailscale-disconnect":
        tailscale_action(["down"])
    elif action == "tailscale-exit":
        node_id = request.get("id", "")
        if not isinstance(node_id, str):
            raise RuntimeError("Invalid exit node")
        if node_id and node_id not in {node["id"] for node in tailscale_status()["exitNodes"]}:
            raise RuntimeError("That exit node is no longer available")
        tailscale_action(["set", "--exit-node=" + node_id])
    else:
        raise RuntimeError("Unknown VPN action")
    return status()


def main():
    try:
        request = json.loads(sys.stdin.readline())
        if not isinstance(request, dict):
            raise RuntimeError("Invalid request")
        result = perform(request)
    except (RuntimeError, ValueError, TypeError, json.JSONDecodeError) as exc:
        result = {"error": str(exc)}
    print(json.dumps(result))


if __name__ == "__main__":
    main()
