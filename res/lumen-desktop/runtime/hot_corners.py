"""Validated event-driven corner actions; native pointer observation owns geometry."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

CORNERS = ('top-left', 'top-right', 'bottom-left', 'bottom-right')
MODIFIERS = {'shift', 'control', 'alt', 'super'}
ACTIONS = {'disabled', 'overview', 'app-windows', 'show-desktop', 'notifications-toggle', 'app-launcher', 'lock', 'display-sleep'}


def validate(value):
    if not isinstance(value, dict) or set(value) - set(CORNERS):
        raise ValueError('Unknown corner')
    result = {}
    for corner in CORNERS:
        item = value.get(corner, {})
        if not isinstance(item, dict) or set(item) - {'action', 'modifiers', 'allowFullscreen', 'allowAdjacent'}:
            raise ValueError('Invalid corner configuration')
        action = item.get('action', 'disabled')
        modifiers = item.get('modifiers', [])
        if type(action) is not str or action not in ACTIONS or not isinstance(modifiers, list) or any(type(m) is not str or m not in MODIFIERS for m in modifiers) or len(set(modifiers)) != len(modifiers):
            raise ValueError('Unsupported corner action or modifiers')
        if any(type(item.get(key, False)) is not bool for key in ('allowFullscreen', 'allowAdjacent')):
            raise ValueError('Corner safeguards must be boolean')
        result[corner] = dict(action=action, modifiers=modifiers, allowFullscreen=item.get('allowFullscreen', False), allowAdjacent=item.get('allowAdjacent', False))
    return result


class CornerPolicy:
    def __init__(self, config=None):
        self.config = validate(config or {})
        self.entered = None

    def transition(self, event):
        if event.get('kind') != 'lumen-corner' or type(event.get('version')) is not int or event['version'] != 1:
            return None
        corner = event.get('corner')
        if corner is None:
            self.entered = None
            return None
        if corner not in CORNERS or type(event.get('output')) is not str or not event['output']:
            raise ValueError('Invalid native corner transition')
        identity = (event['output'], corner)
        if self.entered == identity:
            return None
        self.entered = identity
        config = self.config[corner]
        modifiers = event.get('modifiers', [])
        if not isinstance(modifiers, list) or any(type(m) is not str or m not in MODIFIERS for m in modifiers):
            raise ValueError('Invalid native modifiers')
        # Missing or malformed inhibition information fails closed.
        if any(type(event.get(key)) is not bool for key in ('locked', 'fullscreen', 'adjacent')):
            return None
        if event.get('locked', True) or (event.get('fullscreen', True) and not config['allowFullscreen']) or (event.get('adjacent', True) and not config['allowAdjacent']):
            return None
        if not set(config['modifiers']).issubset(modifiers) or config['action'] == 'disabled':
            return None
        return config['action']


def dispatch(action):
    if action not in ACTIONS or action == 'disabled':
        raise ValueError('Unsupported corner action')
    if action == 'lock':
        command = ['lumen-lock']
    elif action == 'app-launcher':
        command = ['lumen-app-launcher']
    elif action == 'display-sleep':
        command = ['swaymsg', 'output * power off']
    else:
        command = ['lumen-desktop-action', action]
    subprocess.run(command, check=True, timeout=15)


def write_config(path, config):
    value = validate(config)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, name = tempfile.mkstemp(dir=path.parent, prefix='.corners-')
    try:
        with os.fdopen(fd, 'w') as stream:
            json.dump(value, stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', type=Path, default=Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state')) / 'lumen-desktop/hot-corners.json')
    parser.add_argument('action', choices=['status', 'configure', 'watch'])
    args = parser.parse_args()
    if args.action == 'configure':
        write_config(args.config, json.loads(sys.stdin.readline()))
        print(json.dumps({'ok': True}))
        return
    load = lambda: validate(json.loads(args.config.read_text()) if args.config.exists() else {})
    if args.action == 'status':
        version = json.loads(subprocess.check_output(['swaymsg', '-r', '-t', 'get_version'], timeout=5))
        print(json.dumps({'corners': load(), 'nativeAvailable': version.get('native_hot_corners', False)}))
        return
    policy = CornerPolicy(load())
    stream = subprocess.Popen(['swaymsg', '-r', '-m', '-t', 'subscribe', '["tick"]'], stdout=subprocess.PIPE, text=True)
    try:
        for line in stream.stdout:
            event = json.loads(line)
            if not event.get('payload'):
                continue
            try:
                transition = json.loads(event['payload'])
            except json.JSONDecodeError:
                continue
            if not isinstance(transition, dict) or transition.get('kind') != 'lumen-corner':
                continue
            policy.config = load()
            action = policy.transition(transition)
            if action:
                dispatch(action)
    finally:
        stream.terminate()
        stream.wait(timeout=5)


if __name__ == '__main__':
    main()
