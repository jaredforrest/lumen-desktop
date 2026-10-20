"""Real window placement policy on a private headless compositor."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess as sp
import sys
import tempfile
import time

p = argparse.ArgumentParser()
p.add_argument('--output', type=Path, required=True)
p.add_argument('--launcher', type=Path, required=True)
p.add_argument('--sway', default='sway')
p.add_argument('--inside', action='store_true')
a = p.parse_args()
if not a.inside:
    with tempfile.TemporaryDirectory(prefix='lumen-spaces-') as private:
        env = dict(os.environ, HOME=private, XDG_RUNTIME_DIR=private, NO_AT_BRIDGE='1', QT_ACCESSIBILITY='0', GTK_A11Y='none')
        for key in ('CONFIG', 'STATE', 'CACHE', 'DATA'):
            env['XDG_' + key + '_HOME'] = str(Path(private) / key.lower())
        for key in ('SWAYSOCK', 'WAYLAND_DISPLAY', 'DISPLAY', 'DBUS_SESSION_BUS_ADDRESS', 'AT_SPI_BUS_ADDRESS'):
            env.pop(key, None)
        raise SystemExit(sp.call(['dbus-run-session', '--', sys.executable, __file__, *sys.argv[1:], '--inside'], env=env))
import re
out = a.output.resolve()
out.mkdir(parents=True, exist_ok=False)
script = a.launcher.read_text()
prefix = re.split(r'\n[ \t]*exec quickshell ', script, maxsplit=1)[0]
env = dict(v.decode().split('=', 1) for v in sp.check_output(['bash', '-c', prefix + '\nexec env -0']).split(b'\0') if b'=' in v)
env.update(WLR_BACKENDS='headless', WLR_RENDERER='pixman', WLR_LIBINPUT_NO_DEVICES='1', NO_AT_BRIDGE='1', QT_ACCESSIBILITY='0', GTK_A11Y='none')
processes = []
source = Path(__file__).resolve().parents[2] / 'res/lumen-desktop/runtime/spaces.py'
spec = importlib.util.spec_from_file_location('spaces', source)
spaces = importlib.util.module_from_spec(spec)
spec.loader.exec_module(spaces)

def launch(cmd, name):
    with (out / name).open('w') as _fixture_log0:
        proc = sp.Popen(cmd, env=env, stdout=_fixture_log0, stderr=sp.STDOUT, start_new_session=True)
    processes.append(proc)
    return proc

def wait(fn):
    end = time.monotonic() + 15
    while time.monotonic() < end:
        try:
            value = fn()
            if value:
                return value
        except (OSError, ValueError, sp.SubprocessError):
            pass
        time.sleep(.05)
    raise AssertionError('Timed out')

def sway(*args):
    return json.loads(sp.check_output(['swaymsg', '-r', *args], env=env, text=True))

def nodes(node):
    yield node
    for child in node.get('nodes', []) + node.get('floating_nodes', []):
        yield from nodes(child)

def windows(app):
    return [n for n in nodes(sway('-t', 'get_tree')) if n.get('app_id') == app]

try:
    config = out / 'sway.conf'
    config.write_text('output HEADLESS-1 resolution 1280x720\nseat seat0 fallback true\n')
    launch([a.sway, '--unsupported-gpu', '-c', str(config)], 'sway.log')
    runtime = Path(env['XDG_RUNTIME_DIR'])
    env['SWAYSOCK'] = str(wait(lambda: next(runtime.glob('sway-ipc.*.sock'), None)))
    env['WAYLAND_DISPLAY'] = wait(lambda: next((v.name for v in runtime.glob('wayland-*') if not v.name.endswith('.lock')), None))
    sway('workspace "1"')
    launch(['foot', '--app-id=spaces-first', 'sleep', '120'], 'first.log')
    wait(lambda: windows('spaces-first'))
    sway('workspace "2"')
    launch(['foot', '--app-id=spaces-anchor', 'sleep', '120'], 'anchor.log')
    wait(lambda: windows('spaces-anchor'))
    sway('workspace "1"')
    policy = spaces.SpacePolicy(out / 'policy.json')
    policy.reconcile(sway('-t', 'get_workspaces'), sway('-t', 'get_outputs'))
    target = next(k for k, item in policy.state['desktops'].items() if item['name'] == '2')
    policy.assign('wayland:spaces-editor', 'desktop', target)
    watcher = launch([sys.executable, str(source), '--state', str(policy.path), 'watch'], 'watch.log')
    time.sleep(.3)
    launch(['foot', '--app-id=spaces-editor', 'sleep', '120'], 'editor.log')
    def placed():
        workspace = next(w for w in sway('-t', 'get_workspaces') if w['name'] == '2')
        node = next(n for n in nodes(sway('-t', 'get_tree')) if n.get('id') == workspace['id'])
        return any(n.get('app_id') == 'spaces-editor' for n in nodes(node))
    wait(placed)
    watcher.terminate()
    watcher.wait(timeout=5)
    launch([sys.executable, str(source), '--state', str(policy.path), 'watch'], 'watch-restarted.log')
    time.sleep(.3)
    launch(['foot', '--app-id=spaces-editor', 'sleep', '120'], 'editor-second.log')
    wait(lambda: len(windows('spaces-editor')) == 2)
    def both_placed():
        workspace = next(w for w in sway('-t', 'get_workspaces') if w['name'] == '2')
        node = next(n for n in nodes(sway('-t', 'get_tree')) if n.get('id') == workspace['id'])
        return len([n for n in nodes(node) if n.get('app_id') == 'spaces-editor']) == 2
    wait(both_placed)
    sway('workspace "3"')
    launch(['foot', '--app-id=spaces-third', 'sleep', '120'], 'third.log')
    wait(lambda: windows('spaces-third'))
    for name in ('1', '2', '3'):
        sway('workspace ' + json.dumps(name))
        time.sleep(.2)
    with policy.transaction():
        policy.reconcile(sway('-t', 'get_workspaces'), sway('-t', 'get_outputs'))
        identities = dict(policy.current)
        assert [policy.state['desktops'][key]['name'] for key in policy.order('HEADLESS-1')][:3] == ['3', '2', '1']
        policy.configure(auto_reorder=False)
        assert [policy.state['desktops'][key]['name'] for key in policy.order('HEADLESS-1')][:3] == ['1', '2', '3']
        assert identities == policy.current
    sway('create_output')
    wait(lambda: any(o['name'] == 'HEADLESS-2' for o in sway('-t', 'get_outputs')))
    sway('workspace "4"; move workspace to output "HEADLESS-2"')
    second = next(o for o in sway('-t', 'get_outputs') if o['name'] == 'HEADLESS-2')
    display_id = spaces.SpacePolicy.display(second)
    with policy.transaction():
        policy.reconcile(sway('-t', 'get_workspaces'), sway('-t', 'get_outputs'))
        policy.assign('wayland:spaces-display', 'display', display_id)
    sway('workspace "1"')
    launch(['foot', '--app-id=spaces-display', 'sleep', '120'], 'display.log')
    wait(lambda: windows('spaces-display'))
    def display_placed():
        output = next(n for n in nodes(sway('-t', 'get_tree')) if n.get('type') == 'output' and n.get('name') == 'HEADLESS-2')
        return any(n.get('app_id') == 'spaces-display' for n in nodes(output))
    wait(display_placed)
    sway('output "HEADLESS-2" disable')
    time.sleep(.3)
    with policy.transaction():
        policy.reconcile(sway('-t', 'get_workspaces'), sway('-t', 'get_outputs'))
        assert policy.destination({'app_id': 'spaces-display'})['status'] == 'unavailable'
        assert policy.state['assignments']['wayland:spaces-display']['target_id'] == display_id
    sway('output "HEADLESS-2" enable')
    wait(lambda: any(o['name'] == 'HEADLESS-2' and o['active'] for o in sway('-t', 'get_outputs')))
    with policy.transaction():
        policy.reconcile(sway('-t', 'get_workspaces'), sway('-t', 'get_outputs'))
        assert policy.destination({'app_id': 'spaces-display'})['status'] == 'available'
    (out / 'result.json').write_text(json.dumps({'passed': True, 'newWindowsPlaced': 2, 'watcherRestartPersistence': True, 'displayAssignment': True, 'displayRemovalRetained': True, 'displayReconnectRecovered': True, 'recentOrderSeparateFromIdentity': True}, indent=2) + '\n')
finally:
    for proc in reversed(processes):
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
    for proc in reversed(processes):
        try:
            proc.wait(timeout=5)
        except sp.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
