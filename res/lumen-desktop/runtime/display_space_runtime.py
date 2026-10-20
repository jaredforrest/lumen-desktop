"""Authoritative preference reads and captured native cohort configuration."""
import json
import os
from pathlib import Path
from copy import deepcopy
from display_space_policy import preferences, primary, plan, identity_record


def configured():
    active = os.environ.get('LUMEN_NIX_SETTINGS_ACTIVE')
    paths = [Path(active)] if active else [Path('/etc/nix/desktop-settings.json'), Path(os.environ.get('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))) / 'lumen-nix-settings/current/settings.json']
    for path in paths:
        if not path.exists():
            continue
        with path.open('rb') as stream:
            data = stream.read(4 * 1024 * 1024 + 1)
        if len(data) > 4 * 1024 * 1024:
            raise ValueError('Display preferences exceed the settings read budget')
        value = json.loads(data)
        if not isinstance(value, dict):
            raise ValueError('Invalid settings object')
        return preferences(value.get('spaces', {}))
    return preferences({})


def synchronize(model, query, desired=None):
    """Persist logical contexts before registering an exact session-local plan.

    Failure never rolls back a user workspace or forces a window action. A later
    observer refresh may register a newly captured plan; failed UI actions are
    never replayed.
    """
    desired = configured() if desired is None else preferences(desired)
    outputs = query('-t', 'get_outputs')
    tree = query('-t', 'get_tree')
    if not isinstance(tree, dict):
        raise ValueError('Invalid native desktop facts')
    native = tree.get('display_spaces')
    if not isinstance(native, dict) or native.get('version') != 1 or not isinstance(native.get('session'), str):
        return dict(available=False, separateDisplays=desired['separateDisplays'], applied=False, primary=None, error='Display-space policy requires the updated compositor')
    chosen = primary(desired['primaryDisplay'], model.state.get('automatic_primary'), outputs)
    if chosen['effective'] is None:
        return dict(available=False, separateDisplays=desired['separateDisplays'], applied=False, primary=None, error='No connected main display is available')
    candidate = deepcopy(model.state)
    candidate['display_separate'] = desired['separateDisplays']
    if candidate.get('automatic_primary') is None:
        candidate['automatic_primary'] = primary(None, None, outputs)['preferred']
    if not desired['separateDisplays']:
        result = plan(candidate.get('display_cohorts', []), model.catalog(), outputs,
                      preserve_visible=not native.get('coupled') or not candidate.get('display_cohorts'))
        if result['cohorts'] != candidate.get('display_cohorts'):
            candidate['display_cohort_provenance'] = result['provenance']
        candidate['display_cohorts'] = result['cohorts']
        for row in result['created']:
            candidate['created_counter'] = candidate.get('created_counter', 0) + 1
            candidate['desktops'][row['logicalId']] = dict(name=row['name'], output=model.outputs[row['outputId']], output_id=row['outputId'],
                persistent=True, generated=True, label='Desktop ' + str(candidate['created_counter']),
                ordinal=max((item['ordinal'] for item in candidate['desktops'].values()), default=-1) + 1, recent=0)
    # Logical identities are durable before a native callback could use them.
    if candidate != model.state:
        model.state = candidate
        model.save()
    by_native = {key: native_id for native_id, key in model.current.items()}
    bindings = []
    active_ids = set(model.outputs)
    for cohort in model.state.get('display_cohorts', []) if not desired['separateDisplays'] else []:
        members = []
        for output_id, key in cohort['members'].items():
            if output_id not in active_ids:
                continue
            item = model.state['desktops'].get(key)
            if item is None:
                raise ValueError('A coupled desktop was retired; refresh before applying')
            members.append(dict(output=model.outputs[output_id], name=item['name'], id=str(by_native[key]) if key in by_native else None))
        if members:
            if len(members) != len(active_ids):
                raise ValueError('Incomplete display context; existing desktops are unchanged')
            bindings.append(dict(id=cohort['id'], members=members))
    captured = [dict(id=str(row['id']), name=row['name'], x=row['rect']['x'], y=row['rect']['y'], width=row['rect']['width'], height=row['rect']['height']) for row in outputs if row.get('active', True)]
    packet = dict(version=1, session=native['session'], separateDisplays=desired['separateDisplays'], primary=chosen['name'], outputs=captured, cohorts=bindings)
    wire = dict(packet, expectedGeneration=native.get('generation'))
    encoded = json.dumps(wire, separators=(',', ':'))
    if len(encoded.encode()) > 65536:
        raise ValueError('Display contexts exceed the native configuration budget')
    if (getattr(model, '_display_packet', None) != packet or native.get('available') is not True
            or native.get('primary') != chosen['name'] or native.get('separateDisplays') is not desired['separateDisplays']
            or getattr(model, '_display_generation', None) != native.get('generation')):
        response = query('lumen_window display-spaces configure ' + encoded.encode().hex())
        if not isinstance(response, list) or not response or not all(row.get('success') is True for row in response):
            raise ValueError('Display bindings changed; the captured policy was not applied')
        model._display_packet = packet
        tree = query('-t', 'get_tree')
        if not isinstance(tree, dict):
            raise ValueError('Invalid native desktop facts')
        native = tree.get('display_spaces', {})
        if not isinstance(native, dict):
            raise ValueError('Invalid native display registration')
        model._display_generation = native.get('generation')
    applied=native.get('available') is True and native.get('separateDisplays') is desired['separateDisplays'] and native.get('primary')==chosen['name']
    return dict(available=native.get('available') is True, separateDisplays=desired['separateDisplays'],
                applied=applied,
                primary=native.get('primary'), primaryIdentity=identity_record(chosen['preferred']), fallback=chosen['fallback'],
                coupled=native.get('coupled') is True, error='' if applied else 'Display registration was superseded or is unavailable')


def activation(model, desktop, query):
    """Return one native captured-cohort action, or independent navigation.

    A failed native dispatch is never retried through the independent path.
    """
    tree = query('-t', 'get_tree')
    if not isinstance(tree, dict):
        raise ValueError('Invalid native desktop facts')
    native = tree.get('display_spaces', {})
    if not isinstance(native, dict):
        raise ValueError('Invalid native display registration')
    if native.get('coupled') is not True:
        return None
    if native.get('available') is not True or model.state.get('display_separate') is not False:
        raise ValueError('Display contexts changed; refresh before switching')
    matches = [row for row in model.state.get('display_cohorts', [])
               if row['members'].get(desktop['outputId']) == desktop['logicalId']]
    if len(matches) != 1:
        raise ValueError('This desktop has no unique display context')
    values = [native.get('session'), native.get('generation'), matches[0]['id'], desktop['output']]
    if not all(isinstance(value, str) and value for value in values):
        raise ValueError('Display selection is unavailable')
    packet = dict(zip(['session', 'expectedGeneration', 'cohort', 'output'], values))
    packet.update(workspace=desktop['name'],workspaceId=str(desktop['id']) if desktop.get('id') is not None else None)
    return 'lumen_window display-spaces activate ' + json.dumps(packet, separators=(',', ':')).encode().hex()
