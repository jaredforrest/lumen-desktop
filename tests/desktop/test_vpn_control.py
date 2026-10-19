"""VPN bridge parses client state and rejects untrusted control requests."""

import importlib.util
import json
from pathlib import Path
from unittest import mock


source = Path(__file__).resolve().parents[2] / "res/lumen-desktop/shell/vpn_control.py"
spec = importlib.util.spec_from_file_location("vpn_control", source)
vpn = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vpn)


def fake_command(argv, **kwargs):
    if argv[:3] == ["mullvad", "status", "--json"]:
        return json.dumps({"state": "connected", "details": {"location": {"city": "London", "country": "United Kingdom", "ipv4": "private"}}})
    if argv[:3] == ["mullvad", "account", "get"]:
        return "Mullvad account:    1234567890123456\nExpires at:        2026-11-18 19:35:16 +00:00\n"
    if argv[:3] == ["mullvad", "relay", "get"]:
        return "Location: country gb\n"
    if argv[:3] == ["tailscale", "status", "--json"]:
        return json.dumps({"BackendState": "Running", "Self": {"Online": True}, "CurrentTailnet": {"Name": "private.test"},
                           "Peer": {"one": {"Online": True, "ExitNodeOption": True, "ID": "node-1", "DNSName": "london.private.test."},
                                    "two": {"Online": False}}, "ExitNodeStatus": {"ID": "node-1"}, "Health": []})
    raise AssertionError(argv)


with mock.patch.object(vpn.shutil, "which", return_value="/bin/fake"), mock.patch.object(vpn, "command", side_effect=fake_command):
    state = vpn.status()
    assert state["mullvad"]["state"] == "connected"
    assert state["mullvad"]["location"] == "London, United Kingdom"
    assert state["mullvad"]["account"] is True
    assert state["mullvad"]["expires"] == "2026-11-18"
    assert "1234567890123456" not in json.dumps(state)
    assert "private" not in state["mullvad"]["location"]
    assert state["tailscale"]["online"] and state["tailscale"]["peers"] == 1
    assert state["tailscale"]["exitNodes"] == [{"id": "node-1", "name": "london.private.test"}]
    for request in ({"action": "tailscale-exit", "id": "unlisted"},
                    {"action": "mullvad-location", "country": "gb;rm"},
                    {"action": "mullvad-login", "account": "123"},
                    {"action": "arbitrary-command"}):
        try:
            vpn.perform(request)
        except RuntimeError:
            pass
        else:
            raise AssertionError(f"Accepted invalid VPN request: {request}")

print("PASS VPN status and action validation")
