"""Focus a Sway window by exact app identity and title, without shell interpolation.

Quickshell 0.2.1's foreign-toplevel activation requires a last input device;
IPC/keyboard-triggered activation before pointer input can otherwise do nothing.
"""
import argparse
import json
import subprocess
import sys

def windows(node, workspace=''):
    if node.get('type') == 'workspace':
        workspace = node.get('name', '')
    yield node, workspace
    for child in node.get('nodes', []) + node.get('floating_nodes', []):
        yield from windows(child, workspace)


def focus_command(tree, app, title, action=None):
    candidates = [(node, workspace) for node, workspace in windows(tree)
                  if (node.get('app_id') or (node.get('window_properties') or {}).get('class')) == app
                  and node.get('name') == title]
    # This is a delayed fallback, not the Dock's window-cycling operation.
    # Activation may have succeeded since QML scheduled us. Never undo it.
    if action is None and any(node.get('focused') for node, _ in candidates):
        return None
    if not candidates:
        raise ValueError('The requested window is no longer available')
    if len(candidates) != 1:
        # Foreign-toplevel does not provide the Sway container identity here.
        # Choosing the first identically titled document can target the wrong one.
        raise ValueError('Window identity is ambiguous; select it in Window Overview')
    chosen, workspace = candidates[0]
    if action is not None:
        if action not in ('close', 'minimize', 'fill', 'fullscreen'):
            raise ValueError('Unsupported window action')
        return '[con_id=%d] lumen_window %s' % (int(chosen['id']), action)
    restore = 'scratchpad show, ' if workspace == '__i3_scratch' else ''
    return '[con_id=%d] %sfocus' % (int(chosen['id']), restore)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app')
    parser.add_argument('title')
    parser.add_argument('--action', choices=['close', 'minimize', 'fill', 'fullscreen'])
    args = parser.parse_args(argv)
    tree = json.loads(subprocess.check_output(
        ['swaymsg', '-t', 'get_tree', '-r'], timeout=4))
    command = focus_command(tree, args.app, args.title, args.action)
    if command is not None:
        result = json.loads(subprocess.check_output(['swaymsg', '-r', command], timeout=4))
        if not result or not all(reply.get('success') for reply in result):
            raise ValueError('The window manager rejected the focus request')
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, subprocess.SubprocessError, ValueError) as exc:
        print('Unable to focus window: ' + str(exc), file=sys.stderr)
        sys.exit(1)
