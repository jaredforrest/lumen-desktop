"""Read-only, title-free compositor state and honest capability observations."""
import argparse
import hashlib
import json
import math
import os
import stat
import subprocess
import sys

from protocol import loads, ProtocolError
from window_context import relationships, contexts


def bounds(value):
    keys = ('x', 'y', 'width', 'height')
    if not isinstance(value, dict) or any(type(value.get(k)) not in (int, float) for k in keys):
        raise ProtocolError('Invalid compositor rectangle')
    if any(not math.isfinite(value[k]) for k in keys):
        raise ProtocolError('Nonfinite compositor rectangle')
    if value['width'] < 0 or value['height'] < 0:
        raise ProtocolError('Negative compositor dimensions')
    return {k: value[k] for k in keys}


def snapshot(tree, outputs, epoch):
    if not isinstance(tree, dict) or not isinstance(outputs, list):
        raise ProtocolError('Invalid compositor snapshot')
    windows, spaces, seen = [], [], set()
    seen_spaces=set()
    extension_seen = False
    pair_extension_seen = False

    def walk(node, workspace=None, output=None, floating=False):
        nonlocal extension_seen, pair_extension_seen
        if not isinstance(node, dict):
            raise ProtocolError('Invalid compositor node')
        for key in ('nodes','floating_nodes'):
            if not isinstance(node.get(key,[]),list):raise ProtocolError('Invalid node children')
        if 'focused' in node and type(node['focused']) is not bool:raise ProtocolError('Invalid focus state')
        if node.get('app_id') is not None and not isinstance(node['app_id'],str):raise ProtocolError('Invalid application hint')
        properties=node.get('window_properties')
        if properties is not None and not isinstance(properties,dict):raise ProtocolError('Invalid window properties')
        if properties and properties.get('class') is not None and not isinstance(properties['class'],str):raise ProtocolError('Invalid application class')
        if node.get('type') == 'output':
            output = node.get('name')
            if not isinstance(output,str) or not output:raise ProtocolError('Invalid output identity')
        if node.get('type') == 'workspace':
            wid=node.get('id')
            if type(wid) is not int or wid<0 or wid in seen_spaces or not isinstance(node.get('name'),str):raise ProtocolError('Invalid workspace identity')
            seen_spaces.add(wid)
            workspace = str(wid)
            spaces.append(dict(id='s-' + workspace, output=output,
                               internal=node.get('name') == '__i3_scratch'))
        properties = node.get('window_properties') or {}
        is_window = node.get('app_id') is not None or node.get('window') is not None
        if is_window:
            container = node.get('id')
            if type(container) is not int or container < 0 or container in seen:
                raise ProtocolError('Invalid or duplicated container identity')
            seen.add(container)
            extension = node.get('lumen_state')
            if extension is not None:
                if not isinstance(extension, dict) or any(type(extension.get(k)) is not bool for k in ('minimised', 'filled')):
                    raise ProtocolError('Invalid placement extension')
                for field in ('zoomed', 'preferred_zoom'):
                    if field in extension and type(extension[field]) is not bool:raise ProtocolError('Invalid preferred zoom state')
                if 'hidden' in extension and type(extension['hidden']) is not bool:raise ProtocolError('Invalid hidden state')
                if 'group_hidden' in extension and type(extension['group_hidden']) is not bool:raise ProtocolError('Invalid group visibility state')
                if 'desktop_hidden' in extension and type(extension['desktop_hidden']) is not bool:raise ProtocolError('Invalid desktop reveal state')
                extension_seen = True
            if type(node.get('fullscreen_mode',0)) is not int or node.get('fullscreen_mode',0) not in (0,1,2):raise ProtocolError('Invalid fullscreen state')
            pairing=node.get('lumen_pair')
            pair=None
            if pairing is not None:
                if not isinstance(pairing,dict) or type(pairing.get('version')) is not int or pairing['version']!=1 or any(type(pairing.get(key)) is not bool for key in ('eligible','survivor')):
                    raise ProtocolError('Invalid paired placement extension')
                pair_extension_seen=True
                if any(key in pairing for key in ('id','partner','side','ratio')):
                    if any(type(pairing.get(key)) is not int or pairing[key]<=0 or pairing[key]==container for key in ('id','partner')) or pairing.get('side') not in ('left','right') or type(pairing.get('ratio')) not in (int,float) or not math.isfinite(pairing['ratio']) or not 0<pairing['ratio']<1 or pairing['survivor']:
                        raise ProtocolError('Invalid paired member identity or ratio')
                    pair=dict(id='p-'+str(pairing['id']),partner='w-'+str(pairing['partner']),side=pairing['side'],ratio=pairing['ratio'])
            relation = relationships(node.get('lumen_relationships'))
            app = node.get('app_id') or properties.get('class')
            windows.append(dict(id='w-' + str(container), app_hint=app,
                identity_assurance='advertised_unverified', main_window=None,
                parent='w-' + str(relation['parent']) if relation and relation['parent'] is not None else None,
                parent_supported=relation['parent_supported'] if relation else False,
                modal=relation['modal'] if relation else None,
                key=bool(node.get('focused')), minimised=extension['minimised'] if extension else None,
                group_hidden=extension.get('group_hidden') if extension else None, app_hidden=extension.get('hidden') if extension else None, desktop_hidden=extension.get('desktop_hidden') if extension else None, occluded=None,
                pair=pair,paired_fullscreen_survivor=pairing['survivor'] if pairing is not None else None,
                placement='paired' if pair else 'fullscreen' if node.get('fullscreen_mode', 0) or pairing and pairing['survivor'] else 'zoomed' if extension and extension.get('zoomed') else 'filled' if extension and extension['filled'] else 'floating' if floating else 'layout_managed',
                space='s-' + workspace if workspace is not None else None,
                output=output, bounds=bounds(node.get('rect')),
                frame_owner='client' if node.get('border') == 'csd' else 'compositor' if node.get('border') in ('normal','pixel') else 'unknown',
                capabilities=dict(document_state=False, application_menu=False,
                                  app_hide=False, preferred_zoom=extension.get('preferred_zoom',False) if extension else False)))
        for child in node.get('nodes', []):
            walk(child, workspace, output, floating)
        for child in node.get('floating_nodes', []):
            walk(child, workspace, output, True)

    walk(tree)
    associations = contexts(windows)
    for window in windows:
        window.update(associations[window['id']])
    live_outputs = []
    output_ids=set()
    for output in outputs:
        if not isinstance(output, dict):
            raise ProtocolError('Invalid output')
        if type(output.get('active',False)) is not bool or type(output.get('focused',False)) is not bool:raise ProtocolError('Invalid output state')
        name=output.get('name')
        if not isinstance(name,str) or not name or name in output_ids:raise ProtocolError('Invalid or repeated output identity')
        output_ids.add(name)
        scale=output.get('scale',1)
        if type(scale) not in (int,float) or not math.isfinite(scale) or scale<=0:raise ProtocolError('Invalid output scale')
        if output.get('active'):
            live_outputs.append(dict(id=output['name'], bounds=bounds(output['rect']),
                                     scale=output.get('scale', 1), focused=bool(output.get('focused'))))
    live_ids={o['id'] for o in live_outputs}
    for sp in spaces:
        if not sp['internal'] and sp['output'] not in live_ids:raise ProtocolError('Workspace references unavailable output')
    groups={}
    for window in windows:
        if window['pair']:groups.setdefault(window['pair']['id'],[]).append(window)
    for members in groups.values():
        if len(members)!=2:raise ProtocolError('Incomplete paired membership')
        first,second=members
        if first['pair']['partner']!=second['id'] or second['pair']['partner']!=first['id'] or first['pair']['side']==second['pair']['side'] or first['pair']['ratio']!=second['pair']['ratio'] or first['space']!=second['space'] or first['output']!=second['output']:
            raise ProtocolError('Inconsistent paired membership')
    keys = [w['id'] for w in windows if w['key']]
    if len(keys) > 1:
        raise ProtocolError('Ambiguous key-window snapshot')
    return dict(schema_version=1, backend='sway', session_epoch=epoch,
        observation='sequential IPC samples; not an atomic revisioned broker snapshot',
        windows=windows, spaces=spaces, outputs=live_outputs,
        focus=dict(key_window=keys[0] if keys else None, seat='backend_default'),
        capabilities=dict(read_only=True, placement_extension=extension_seen, paired_placement_extension=pair_extension_seen,
                          application_registry=False, atomic_snapshot=False,
                          per_seat_focus=False, app_main_window=False),
        unavailable=['Application lifetime and main-window identity require an application adapter',
                     'Hidden, occluded and unknown minimised state must not be inferred from scratchpad membership',
                     'Transport primitives do not constitute a running command broker'])


def runtime_epoch(path):
    metadata = os.lstat(path)
    if not stat.S_ISSOCK(metadata.st_mode) or metadata.st_uid != os.getuid():
        raise ProtocolError('Compositor endpoint is not an owned socket')
    raw = f'{metadata.st_dev}:{metadata.st_ino}:{metadata.st_ctime_ns}'
    return hashlib.sha256(raw.encode()).hexdigest()[:24]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', help='Read synthetic tree/outputs JSON instead of contacting a compositor')
    args = parser.parse_args()
    if args.fixture:
        with open(args.fixture, 'rb') as stream:
            value = loads(stream.read())
        result = snapshot(value['tree'], value['outputs'], 'fixture')
    else:
        socket = os.environ.get('SWAYSOCK')
        if not socket:
            raise ProtocolError('No compatible compositor endpoint')
        epoch = runtime_epoch(socket)
        def query(kind):
            return loads(subprocess.check_output(['swaymsg', '-r', '-t', kind], timeout=4))
        tree, outputs = query('get_tree'), query('get_outputs')
        if runtime_epoch(socket) != epoch:
            raise ProtocolError('Compositor changed during snapshot; retry')
        result = snapshot(tree, outputs, epoch)
    print(json.dumps(result, allow_nan=False, sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        # Do not include backend output, titles, paths or environment in errors.
        print(json.dumps(dict(ok=False, error=type(exc).__name__, message='Desktop state could not be validated')), file=sys.stderr)
        sys.exit(1)
