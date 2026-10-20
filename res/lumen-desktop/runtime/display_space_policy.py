"""Logical display cohorts, primary selection and exact native switch plans.

This is a placement policy, not an authentication claim about display metadata.
The planner changes logical membership only; it never moves or closes a window.
"""
from copy import deepcopy
import json
import uuid

LIMIT = 64


def text(value):
    return isinstance(value, str) and 0 < len(value) <= 512 and not any(ord(c) < 32 or ord(c) == 127 for c in value)


def display_identity(output):
    parts = [output.get(key, '') for key in ('make', 'model', 'serial')]
    if all(text(part) for part in parts) and parts[2] not in ('Unknown', '0'):
        return 'display:' + json.dumps(parts, separators=(',', ':'))
    name = output.get('name')
    if not text(name):
        raise ValueError('Invalid display connector identity')
    return 'connector:' + name


def identity(value):
    if not isinstance(value, str):
        raise ValueError('Invalid typed display identity')
    if value.startswith('connector:') and text(value[10:]):
        return value
    if value.startswith('display:'):
        try:
            parts = json.loads(value[8:])
        except ValueError as error:
            raise ValueError('Invalid display identity metadata') from error
        if isinstance(parts, list) and len(parts) == 3 and all(text(part) for part in parts) and parts[2] not in ('Unknown', '0'):
            canonical = 'display:' + json.dumps(parts, separators=(',', ':'))
            if canonical == value:
                return value
    raise ValueError('Invalid typed display identity')


def preferences(value):
    if not isinstance(value, dict) or set(value) - {'separateDisplays', 'primaryDisplay'}:
        raise ValueError('Invalid display-space preferences')
    separate = value.get('separateDisplays', True)
    if type(separate) is not bool:
        raise ValueError('Display separation must be boolean')
    chosen = value.get('primaryDisplay')
    if chosen is None:
        return dict(separateDisplays=separate, primaryDisplay=None)
    if not isinstance(chosen, dict) or set(chosen) - {'kind', 'name', 'make', 'model', 'serial'}:
        raise ValueError('Invalid primary display identity')
    if chosen.get('kind') == 'connector' and text(chosen.get('name')) and all(chosen.get(key) is None for key in ('make', 'model', 'serial')):
        key = 'connector:' + chosen['name']
    elif chosen.get('kind') == 'display' and chosen.get('name') is None and all(text(chosen.get(key)) for key in ('make', 'model', 'serial')):
        key = 'display:' + json.dumps([chosen[key] for key in ('make', 'model', 'serial')], separators=(',', ':'))
    else:
        raise ValueError('Invalid primary display identity')
    return dict(separateDisplays=separate, primaryDisplay=identity(key))


def identity_record(key):
    if key is None:
        return None
    identity(key)
    if key.startswith('connector:'):
        return dict(kind='connector', name=key[10:], make=None, model=None, serial=None)
    make, model, serial = json.loads(key[8:])
    return dict(kind='display', name=None, make=make, model=model, serial=serial)


def primary(preferred, remembered, outputs):
    """Retain preferred identity while using a deterministic connected fallback."""
    if preferred is not None:
        identity(preferred)
    if remembered is not None:
        identity(remembered)
    rows = [row for row in outputs if row.get('active', True)]
    by_identity = {}
    for row in rows:
        key = display_identity(row)
        if key in by_identity:
            raise ValueError('Ambiguous display identity')
        by_identity[key] = row
    wanted = preferred or remembered
    selected = by_identity.get(wanted)
    if selected is None and isinstance(wanted,str) and wanted.startswith('connector:'):
        selected = next((row for row in rows if row['name']==wanted[10:]),None)
    if selected is not None:
        return dict(preferred=wanted, effective=display_identity(selected), name=selected['name'], fallback=False)
    if not rows:
        return dict(preferred=wanted, effective=None, name=None, fallback=bool(wanted))
    # Geometry and stable identity determine first-use/main fallback, never focus.
    chosen = min(rows, key=lambda row: (row.get('rect', {}).get('x', 0), row.get('rect', {}).get('y', 0), display_identity(row)))
    key = display_identity(chosen)
    return dict(preferred=wanted or key, effective=key, name=chosen['name'], fallback=bool(wanted and wanted != key))


def plan(previous, desktops, outputs, *, preserve_visible=False, new_id=None):
    """Reconcile one member per connected output without changing desktop IDs.

    Output loss may put several members on one surviving output. Repacking them
    into additional contexts keeps every migrated desktop reachable. Original
    cohorts are retained as provenance, not a command to move windows back.
    """
    new_id = new_id or (lambda: str(uuid.uuid4()))
    if not isinstance(desktops, list) or any(not isinstance(row, dict) for row in desktops):
        raise ValueError("Invalid desktop list")
    if not isinstance(outputs, list) or len(outputs) > LIMIT or any(not isinstance(row, dict) for row in outputs):
        raise ValueError("Invalid display list")
    if len(desktops) > LIMIT:
        raise ValueError('The desktop limit is 64')
    rows = deepcopy(desktops)
    keys = [row.get('logicalId') for row in rows]
    if any(not text(key) for key in keys) or len(set(keys)) != len(keys):
        raise ValueError('Invalid logical desktop identities')
    output_keys = [display_identity(output) for output in outputs if output.get('active', True)]
    if len(output_keys) != len(set(output_keys)):
        raise ValueError('Ambiguous display identity')
    active = {key: [row for row in rows if row.get('outputId') == key] for key in output_keys}
    count = max((len(items) for items in active.values()), default=0)
    if not isinstance(previous, list) or len(previous) > LIMIT:
        raise ValueError('Invalid cohort list')
    if any(not isinstance(row, dict) or not isinstance(row.get("members"), dict) or len(row["members"]) > LIMIT for row in previous):
        raise ValueError("Invalid cohort members")
    old = deepcopy(previous)
    ids = [row.get('id') for row in old]
    if any(not text(key) for key in ids) or len(set(ids)) != len(ids):
        raise ValueError('Invalid cohort identity')
    for row in old:
        if not isinstance(row.get('members'), dict):
            raise ValueError('Invalid cohort members')
        for oid, desktop in row['members'].items():
            identity(oid)
            if not text(desktop):
                raise ValueError('Invalid cohort member')
    if count == 0:
        kept = [dict(id=row['id'], members={oid:key for oid,key in row['members'].items() if key in keys}) for row in old]
        return dict(cohorts=[row for row in kept if row['members']], created=[], provenance=old)
    cohorts = [dict(id=row['id'], members={}) for row in old[:count]]
    while len(cohorts) < count:
        key = new_id()
        if not text(key) or key in {row['id'] for row in cohorts}:
            raise ValueError('Invalid generated cohort identity')
        cohorts.append(dict(id=key, members={}))
    assigned = set()
    # Entering coupled mode starts with the exact current visible composition.
    # A mode toggle therefore does not hide a currently visible user window.
    if preserve_visible:
        for oid, items in active.items():
            visible = [row for row in items if row.get('visible')]
            if len(visible) != 1:
                raise ValueError('A connected display has no unique visible desktop')
            cohorts[0]['members'][oid] = visible[0]['logicalId']
            assigned.add(visible[0]['logicalId'])
    for index, cohort in enumerate(cohorts):
        members = old[index]['members'] if index < len(old) else {}
        for oid, items in active.items():
            candidate = members.get(oid)
            if oid not in cohort['members'] and candidate not in assigned and any(row['logicalId'] == candidate for row in items):
                cohort['members'][oid] = candidate
                assigned.add(candidate)
    for oid, items in active.items():
        remaining = [row for row in items if row['logicalId'] not in assigned]
        for cohort in cohorts:
            if oid not in cohort['members'] and remaining:
                candidate = remaining.pop(0)['logicalId']
                cohort['members'][oid] = candidate
                assigned.add(candidate)
    missing = sum(oid not in cohort['members'] for cohort in cohorts for oid in output_keys)
    if len(rows) + missing > LIMIT:
        raise ValueError('Coupled desktops require more than the 64-desktop budget; existing desktops are unchanged')
    created = []
    for cohort in cohorts:
        for oid in output_keys:
            if oid not in cohort['members']:
                key = new_id()
                if not text(key) or key in keys:
                    raise ValueError('Invalid generated desktop identity')
                keys.append(key)
                cohort['members'][oid] = key
                created.append(dict(logicalId=key, outputId=oid, name='lumen:' + key, persistent=True, generated=True))
    # Keep disconnected memberships only when their desktop did not migrate to
    # another connected output. They remain unavailable until their output returns.
    known = {row['logicalId']: row for row in rows}
    retained = {key for cohort in cohorts for key in cohort['members'].values()}
    for index, before in enumerate(old):
        for oid, key in before['members'].items():
            if oid in output_keys or key not in known or key in retained:
                continue
            target = next((row for row in cohorts if row['id'] == before['id']), None)
            if target is None:
                target = dict(id=before['id'], members={})
                cohorts.append(target)
            target['members'][oid] = key
            retained.add(key)
    return dict(cohorts=cohorts, created=created, provenance=old)
