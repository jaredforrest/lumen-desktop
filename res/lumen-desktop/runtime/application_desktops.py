"""Conservative, session-bound all-desktop membership for floating windows.

Native stickiness follows the visible desktop on a window's current display;
this policy never duplicates windows or changes their floating/group state.
"""
import os
from pathlib import Path
from state import runtime_epoch


def windows(tree):
    result = []
    groups = {member['id'] for group in (tree.get('window_groups') or {}).get('groups', []) for member in group['members']}
    def walk(node, workspace='', output='', floating_id=None):
        if node.get('type') == 'workspace':workspace = node['name']
        if node.get('type') == 'output':output = node['name']
        if node.get('pid') and (node.get('app_id') or node.get('window')):
            result.append(dict(node, _workspace=workspace, _output=output, _floating=floating_id == node['id'], _grouped=node['id'] in groups))
        for child in node.get('nodes', []):walk(child, workspace, output, floating_id)
        for child in node.get('floating_nodes', []):walk(child, workspace, output, child.get('id'))
    walk(tree)
    return result


def reason(node):
    relation = node.get('lumen_relationships') or {}
    state = node.get('lumen_state') or {}
    if relation.get('parent_supported') is not True:return 'Window relationships are unavailable for this application'
    if relation.get('parent') is not None or relation.get('modal') is True:return 'Owned and modal windows retain their parent placement'
    if node.get('_grouped'):return 'Turn off window groups before assigning all desktops'
    if not node.get('_floating'):return 'Tiled windows retain their layout; float the window to use all desktops'
    pair = node.get('lumen_pair') or {}
    if node.get('fullscreen_mode') or pair.get('id') or pair.get('survivor'):return 'Full screen and paired windows retain their saved placement'
    if node.get('_workspace') == '__i3_scratch' or any(state.get(k) for k in ('minimised','filled','hidden','desktop_hidden')):
        return 'Restore this window before assigning all desktops'
    return None


def instance(node, epoch):
    pid = node.get('pid')
    if type(pid) is not int or pid <= 0:return None
    try:
        # comm can contain spaces and parentheses; fields after its last ')' are fixed.
        start = Path('/proc/' + str(pid) + '/stat').read_text().rsplit(')', 1)[1].split()[19]
    except (OSError, IndexError):return None
    return dict(epoch=epoch, id=node['id'], pid=pid, start=start)


def membership(policy, node, epoch):
    assignment = policy.state['assignments'].get(policy.application(node), {})
    if assignment.get('kind') != 'all':return dict(status='unassigned')
    unavailable = reason(node)
    if unavailable:return dict(status='pending', reason=unavailable)
    identity = instance(node, epoch)
    if identity is None:return dict(status='pending', reason='The application process instance is unavailable')
    return dict(status='active' if node.get('sticky') else 'pending', reason='Every desktop on this window’s current display' if node.get('sticky') else 'Waiting for native membership')


def reconcile(policy, tree, command, *, epoch=None):
    epoch = epoch or runtime_epoch(os.environ['SWAYSOCK'])
    before = dict(policy.state.get('managed_sticky', {}))
    managed = dict(before)
    observed = windows(tree)
    live = {str(node['id']): node for node in observed}
    for key, record in list(managed.items()):
        node = live.get(key)
        if node is None or instance(node, epoch) != record:
            del managed[key]
            continue
        assignment = policy.state['assignments'].get(policy.application(node), {})
        if assignment.get('kind') != 'all' or reason(node):
            if node.get('sticky'):command('[con_id=' + key + '] sticky disable')
            del managed[key]
    for node in observed:
        assignment = policy.state['assignments'].get(policy.application(node), {})
        if assignment.get('kind') != 'all' or reason(node):continue
        identity = instance(node, epoch)
        if identity is None or node.get('sticky'):continue
        key = str(node['id'])
        # Record ownership durably before the native operation, so watcher restart
        # can undo only this policy's change. Failure retains a harmless pending entry.
        managed[key] = identity
        policy.state['managed_sticky'] = managed
        policy.save()
        command('[con_id=' + key + '] sticky enable')
    if managed != before:
        policy.state['managed_sticky'] = managed
        policy.save()
    return {str(node['id']): membership(policy, node, epoch) for node in observed}
