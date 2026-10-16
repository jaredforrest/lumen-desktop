"""Sway convenience controls; invoked by the Nix wrapper with pinned tools."""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess as sp
import sys
import tempfile
import time

UNIT = 'lumen-screen-record'

def run(*args, **kwargs):
    return sp.run(args, check=True, **kwargs)

def output(*args):
    return run(*args, stdout=sp.PIPE, text=True).stdout.strip()

def menu(title, choices):
    result = sp.run(['rofi', '-dmenu', '-i', '-no-custom', '-p', title],
                    input='\n'.join(choices), text=True, stdout=sp.PIPE)
    answer = result.stdout.strip()
    if result.returncode or answer not in choices:
        raise SystemExit(0)
    return answer

def notify(title, body, image=None, persistent=False, replace=None):
    args = ['notify-send', '-a', 'Desktop conveniences']
    if image:
        args += ['-i', str(image)]
    if persistent:
        args += ['-t', '0']
    if replace:
        args += ['-r', replace]
    result = sp.run(args + ['-p', title, body], check=False, stdout=sp.PIPE, text=True)
    return result.stdout.strip()

def tree():
    return json.loads(output('swaymsg', '-t', 'get_tree'))

def focused(node):
    if node.get('focused'):
        return node
    for child in node.get('nodes', []) + node.get('floating_nodes', []):
        found = focused(child)
        if found:
            return found
    return None

def geometry(rect):
    if rect['width'] <= 0 or rect['height'] <= 0:
        raise ValueError('The selected window has no capture area.')
    return f"{rect['x']},{rect['y']} {rect['width']}x{rect['height']}"

def display():
    outputs = json.loads(output('swaymsg', '-t', 'get_outputs'))
    active = [o for o in outputs if o.get('active')]
    if not active:
        raise ValueError('No active display found.')
    names = [o['name'] for o in active]
    return names[0] if len(names) == 1 else menu('Display', names)

def region():
    result = sp.run(['slurp'], stdout=sp.PIPE, text=True)
    if result.returncode or not result.stdout.strip():
        raise SystemExit(0)
    return result.stdout.strip()

def unique_file(folder, suffix):
    folder.mkdir(parents=True, exist_ok=True)
    fd, path = tempfile.mkstemp(prefix=time.strftime('%Y-%m-%d_%H-%M-%S-'), suffix=suffix, dir=folder)
    os.close(fd)
    return Path(path)

def capture(args):
    # Snapshot focus before opening the menu, which takes keyboard focus.
    window = focused(tree())
    mode = args.mode or menu('Screenshot', ['Region', 'Window', 'Display'])
    delay = args.delay
    if delay is None:
        delay = int(menu('Delay in seconds', ['0', '3', '5', '10']))
    action = args.action or menu('Output', ['Preview and annotate', 'Save file', 'Copy to clipboard', 'Save and copy', 'Recognize text'])
    options = []
    if mode == 'Region':
        options = ['-g', args.geometry or region()]
    elif mode == 'Window':
        if not window or window.get('type') not in ('con', 'floating_con'):
            raise ValueError('Focus an application window before capturing it.')
        options = ['-g', geometry(window['rect'])]
    else:
        options = ['-o', args.output or display()]
    time.sleep(max(delay, 0.2))
    dest = Path(args.directory) if args.directory else Path.home() / 'data/screenshots'
    # A private temporary capture is removed on failure or cancellation.
    with tempfile.TemporaryDirectory(prefix='lumen-capture-') as temporary:
        source = Path(temporary) / 'capture.png'
        run('grim', *options, str(source))
        if action == 'Recognize text':
            cache = Path(os.environ.get('XDG_CACHE_HOME', str(Path.home()/'.cache'))) / 'lumen-desktop/ocr-captures'
            cache.mkdir(parents=True, exist_ok=True, mode=0o700)
            saved = unique_file(cache, '.png')
            saved.write_bytes(source.read_bytes())
            saved.chmod(0o600)
            try:
                run('lumen-desktop-action', 'recognize-image', str(saved))
            except Exception:
                saved.unlink(missing_ok=True)
                raise
            for old in sorted(cache.glob('*.png'), key=lambda p:p.stat().st_mtime, reverse=True)[20:]:
                old.unlink(missing_ok=True)
            return
        if action == 'Copy to clipboard':
            with source.open('rb') as data:
                run('wl-copy', '--type', 'image/png', stdin=data)
            notify('Screenshot copied', 'Paste into the destination application.')
            return
        if action == 'Preview and annotate':
            # Give Ctrl+S a real destination; cancellation leaves no image behind.
            saved = unique_file(dest, '.png')
            saved.unlink()
            run('satty', '--filename', str(source), '--output-filename', str(saved))
            return
        saved = unique_file(dest, '.png')
        try:
            saved.write_bytes(source.read_bytes())
        except Exception:
            saved.unlink(missing_ok=True)
            raise
        if action == 'Save and copy':
            with saved.open('rb') as data:
                run('wl-copy', '--type', 'image/png', stdin=data)
        notify('Screenshot saved', str(saved), saved)
        print(saved)

def recording_active():
    state = sp.run(['systemctl', '--user', 'show', UNIT, '-p', 'ActiveState', '--value'],
                   stdout=sp.PIPE, text=True)
    return state.stdout.strip() in ('active', 'activating', 'deactivating')

def record(args):
    if args.stop:
        if recording_active():
            run('systemctl', '--user', 'stop', UNIT)
        return
    if recording_active():
        menu('Recording in progress', ['Stop recording', 'Keep recording']) == 'Stop recording' and run('systemctl', '--user', 'stop', UNIT)
        return
    mode = args.mode or menu('Record screen', ['Region', 'Display'])
    if mode == 'Region':
        options = ['-g', args.geometry or region()]
    else:
        options = ['-o', args.output or display()]
    audio = args.audio or menu('Audio', ['No audio', 'Default audio source'])
    if audio == 'Default audio source':
        options += ['--audio']
    dest = Path(args.directory) if args.directory else Path.home() / 'data/recordings'
    saved = unique_file(dest, '.mp4')
    saved.unlink()  # wf-recorder creates the output itself.
    command = ['systemd-run', '--user', '--collect', '--unit', UNIT,
               '--property=KillSignal=SIGINT', '--property=KillMode=mixed',
               '--property=TimeoutStopSec=30']
    for key in ('WAYLAND_DISPLAY', 'SWAYSOCK', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS'):
        if key in os.environ:
            command += ['--setenv', f'{key}={os.environ[key]}']
    command += [os.environ['LUMEN_UTILITIES_WRAPPER'], 'record-worker', str(saved), *options]
    run(*command)

def worker(args):
    child = sp.Popen(['wf-recorder', '-f', args.file, *args.options])
    def stop(_signal, _frame):
        if child.poll() is None:
            child.send_signal(signal.SIGINT)
    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    time.sleep(0.5)
    notice = None
    if child.poll() is None:
        notice = notify('Screen recording is active', 'Super+Shift+R opens the stop control.', persistent=True)
    result = child.wait()
    saved = Path(args.file)
    if result == 0 and saved.exists() and saved.stat().st_size:
        notify('Screen recording stopped', str(saved), replace=notice)
        return
    notify('Screen recording failed', 'Check the lumen-screen-record user service log.', replace=notice)
    raise SystemExit(result or 1)

def shortcuts_watch():
    # Subscribe before applying bindings, then restore them after every reload.
    wrapper = os.environ['LUMEN_UTILITIES_WRAPPER']
    commands = '; '.join(f'bindsym {key} exec {wrapper} {action}' for key, action in (
        ('XF86EmojiPicker', 'characters'), ('Control+Mod4+space', 'characters'), ('Mod4+period', 'characters'), ('Print', 'capture'), ('Mod4+Shift+r', 'record')))
    def apply():
        replies = json.loads(output('swaymsg', '-r', commands))
        if not all(reply.get('success') for reply in replies):
            raise ValueError(f'Shortcut activation failed: {replies}')
        print('Section E shortcuts applied', flush=True)
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as events:
        events.connect(os.environ['SWAYSOCK'])
        payload = b'["workspace"]'
        events.sendall(struct.pack('<6sII', b'i3-ipc', len(payload), 2) + payload)
        def receive_bytes(size):
            data = b''
            while len(data) < size:
                chunk = events.recv(size - len(data))
                if not chunk:
                    raise OSError('Sway IPC disconnected')
                data += chunk
            return data
        def receive():
            magic, size, kind = struct.unpack('<6sII', receive_bytes(14))
            if magic != b'i3-ipc' or size > 16 * 1024 * 1024:
                raise ValueError('Invalid Sway IPC message')
            return kind, json.loads(receive_bytes(size))
        kind, first = receive()
        if kind != 2 or not first.get('success'):
            raise ValueError(f'Sway subscription failed: {first}')
        apply()
        while True:
            kind, event = receive()
            if kind == 0x80000000 and event.get('change') == 'reload':
                apply()


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('characters')
    commands.add_parser('shortcuts-watch')
    screenshot = commands.add_parser('capture')
    screenshot.add_argument('--mode', choices=['Region', 'Window', 'Display'])
    screenshot.add_argument('--action', choices=['Preview and annotate', 'Save file', 'Copy to clipboard', 'Save and copy', 'Recognize text'])
    screenshot.add_argument('--delay', type=int, choices=range(0, 31))
    screenshot.add_argument('--geometry')
    screenshot.add_argument('--output')
    screenshot.add_argument('--directory')
    recorder = commands.add_parser('record')
    recorder.add_argument('--mode', choices=['Region', 'Display'])
    recorder.add_argument('--audio', choices=['No audio', 'Default audio source'])
    recorder.add_argument('--geometry')
    recorder.add_argument('--output')
    recorder.add_argument('--directory')
    recorder.add_argument('--stop', action='store_true')
    service = commands.add_parser('record-worker')
    service.add_argument('file')
    service.add_argument('options', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    try:
        if args.command == 'characters':
            run('lumen-character-viewer')
        elif args.command == 'shortcuts-watch':
            shortcuts_watch()
        elif args.command == 'capture':
            capture(args)
        elif args.command == 'record':
            record(args)
        else:
            worker(args)
    except (sp.CalledProcessError, ValueError, OSError) as error:
        notify('Desktop action failed', str(error))
        print(error, file=sys.stderr)
        raise SystemExit(1)

if __name__ == '__main__':
    main()
