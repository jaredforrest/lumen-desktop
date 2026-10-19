"""Focus a Sway window by exact app identity and title, without shell interpolation.

Quickshell 0.2.1's foreign-toplevel activation requires a last input device;
IPC/keyboard-triggered activation before pointer input can otherwise do nothing.
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent / "runtime"))
from window_context import relationships, activation_target

def windows(node, workspace=''):
    if node.get('type') == 'workspace':
        workspace = node.get('name', '')
    yield node, workspace
    for child in node.get('nodes', []) + node.get('floating_nodes', []):
        yield from windows(child, workspace)


def modal_target(tree, chosen):
    observed = []
    nodes = {}
    for node, workspace in windows(tree):
        if node.get('app_id') is None and node.get('window') is None and not node.get('window_properties'):
            continue
        relation = relationships(node.get('lumen_relationships'))
        nodes[node['id']] = (node, workspace)
        observed.append(dict(id=node['id'], parent=relation['parent'] if relation else None,
                             modal=relation['modal'] if relation else None))
    return nodes[activation_target(observed, chosen['id'])]


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
    chosen, workspace = modal_target(tree, chosen)
    restore = 'scratchpad show, ' if workspace == '__i3_scratch' else ''
    return '[con_id=%d] %sfocus' % (int(chosen['id']), restore)


def app_focus_command(tree, app, titles, switch_space=True):
    if not isinstance(titles, list) or not all(isinstance(title, str) for title in titles):
        raise ValueError('Application window titles must be a list of strings')
    nodes = [(node, workspace) for node, workspace in windows(tree)]
    current = next((workspace for node, workspace in nodes if node.get('focused') and workspace), None)
    candidates = [(node, workspace) for node, workspace in nodes
                  if (node.get('app_id') or (node.get('window_properties') or {}).get('class')) == app
                  and node.get('name') in titles]
    unhide = ['[con_id=%d] lumen_window unhide' % int(node['id']) for node, _ in candidates
              if (node.get('lumen_state') or {}).get('hidden')]
    prefix = '; '.join(unhide)
    candidates = [(node, (node.get('lumen_state') or {}).get('restore_workspace') or workspace)
                  for node, workspace in candidates]
    if not switch_space:
        if current is None:
            raise ValueError('The active workspace could not be determined')
        candidates = [(node, workspace) for node, workspace in candidates if workspace == current]
    if not candidates:
        return None
    # Titles are supplied in committed MRU order by the shell. An ambiguous
    # title remains unsafe even when app activation may pick another window.
    for title in titles:
        matches = [(node, workspace) for node, workspace in candidates if node.get('name') == title]
        if len(matches) > 1:
            raise ValueError('Window identity is ambiguous; select it in Window Overview')
        if matches:
            chosen, workspace = modal_target(tree, matches[0][0])
            workspace = (chosen.get('lumen_state') or {}).get('restore_workspace') or workspace
            if not switch_space and workspace != current:
                return prefix or None
            if chosen.get('focused'):
                return prefix or None
            state = chosen.get('lumen_state') or {}
            restore = 'scratchpad show, ' if state.get('minimised') or (workspace == '__i3_scratch' and not state.get('hidden')) else ''
            command = '[con_id=%d] %sfocus' % (int(chosen['id']), restore)
            return prefix + '; ' + command if prefix else command
    return None


def app_visibility_command(tree, app, titles, action):
    if action not in ('hide', 'hide-others'):
        raise ValueError('Unsupported application visibility action')
    if not isinstance(titles, list) or not all(isinstance(title, str) for title in titles):
        raise ValueError('Application window titles must be a list of strings')
    targets=[]
    for node, _ in windows(tree):
        if not node.get('pid') or not node.get('id'):
            continue
        identity=node.get('app_id') or (node.get('window_properties') or {}).get('class')
        belongs=identity == app and node.get('name') in titles
        if belongs == (action == 'hide'):
            state=node.get('lumen_state')
            if not state or state.get('version') != 1:
                raise ValueError('This compositor does not support independent application hiding')
            targets.append('[con_id=%d] lumen_window hide' % int(node['id']))
    return '; '.join(targets) or None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app')
    parser.add_argument('title')
    parser.add_argument('--action', choices=['close', 'minimize', 'fill', 'fullscreen'])
    parser.add_argument('--activate-titles', help='Ordered application window title identities as JSON')
    parser.add_argument('--no-switch-space', action='store_true')
    parser.add_argument('--app-action', choices=['hide','hide-others'])
    parser.add_argument('--previous-app')
    parser.add_argument('--previous-title', default='')
    args = parser.parse_args(argv)
    tree = json.loads(subprocess.check_output(
        ['swaymsg', '-t', 'get_tree', '-r'], timeout=4))
    if args.activate_titles is not None:
        if args.action:
            raise ValueError('Application activation cannot be combined with a window action')
        if args.app_action:
            command = app_visibility_command(tree, args.app, json.loads(args.activate_titles), args.app_action)
            if args.app_action == 'hide-others':
                activate = app_focus_command(tree, args.app, json.loads(args.activate_titles), not args.no_switch_space)
                command = '; '.join(part for part in (command, activate) if part) or None
            elif args.previous_app:
                previous = app_focus_command(tree, args.previous_app, [args.previous_title], not args.no_switch_space)
                command = '; '.join(part for part in (command, previous) if part) or None
        else:
            command = app_focus_command(tree, args.app, json.loads(args.activate_titles), not args.no_switch_space)
    else:
        if args.app_action or args.previous_app:
            raise ValueError('Application actions require an application window list')
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
