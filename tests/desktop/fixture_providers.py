"""Replace unrelated provider commands only inside disposable full-shell copies."""
from pathlib import Path
import hashlib
import json

PROVIDERS = (
    ('VpnState.qml', 'Qt.resolvedUrl("vpn_control.py").toString().replace("file://", "")', 'vpn',
     dict(mullvad=dict(installed=False, state='unavailable', account=False),
          tailscale=dict(installed=False, state='unavailable', online=False, peers=0, exitNodes=[]))),
    ('SystemState.qml', 'Qt.resolvedUrl("system-controls.py").toString().replace(/^file:\\/\\//, "")', 'system',
     dict(wifi=dict(available=False, networks=[]), bluetoothRadios={}, power=dict(available=False))),
)


def isolate_providers(shell, output):
    """Fail before launching on changed command seams; record reversible test edits.

    This does not isolate D-Bus, HOME, audio, or any other process. Callers must
    already supply their private session environment. No product file is edited.
    """
    shell, output = Path(shell).resolve(), Path(output).resolve()
    if not shell.is_relative_to(output):
        raise ValueError('Provider fixtures require a private copied shell')
    prepared = []
    for filename, needle, kind, payload in PROVIDERS:
        target = shell / filename
        if target.is_symlink() or not target.resolve().is_relative_to(output):
            raise ValueError('Provider fixture target must be a private regular copy')
        original = target.read_text()
        if original.count(needle) != 1:
            raise ValueError('Provider command seam changed: ' + filename)
        helper = output / (kind + '-unavailable-provider.py')
        requests = output / (kind + '-provider-requests.jsonl')
        code = ('import json,sys\nfrom pathlib import Path\n'
                + 'request=json.loads(sys.stdin.readline()) if ' + repr(kind == 'vpn')
                + ' else {"action":sys.argv[1] if len(sys.argv)>1 else ""}\n'
                + 'with Path(' + repr(str(requests)) + ').open("a") as log:log.write(json.dumps(request)+"\\n")\n'
                + 'if request.get("action")!="status":raise SystemExit("Unexpected fixture provider action")\n'
                + 'print(' + repr(json.dumps(payload)) + ')\n')
        replacement = json.dumps(str(helper))
        modified = original.replace(needle, replacement, 1)
        assert modified.replace(replacement, needle, 1) == original
        prepared.append((target, helper, code, original, modified))
    records = []
    for target, helper, code, original, modified in prepared:
        helper.write_text(code)
        target.chmod(0o600)
        target.write_text(modified)
        records.append(dict(file=str(target), originalSha256=_digest(original),
                            modifiedSha256=_digest(modified), helper=str(helper),
                            helperSha256=_digest(code), roundtrip=True, originalMatchCount=1,
                            scope='Only unrelated provider command target; non-status requests rejected'))
    (output / 'provider-isolation.json').write_text(json.dumps(records, indent=2) + '\n')
    return records


def _digest(text):
    return hashlib.sha256(text.encode()).hexdigest()
