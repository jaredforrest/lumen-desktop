"""Separate protocol ancestry, keyboard recipient and adapter document authority.

Only a compositor sample supplies parenthood. An adapter may supply its own
main document association; ancestry and recent keyboard focus never invent it.
Application hints remain advertised, not authenticated package identities.
"""
from protocol import ProtocolError


def relationships(value):
    if value is None:
        return None
    if (not isinstance(value, dict) or type(value.get('version')) is not int
            or value['version'] != 1 or type(value.get('parent_supported')) is not bool
            or (value.get('parent') is not None and
                (type(value['parent']) is not int or value['parent'] < 0))
            or (value.get('modal') is not None and type(value['modal']) is not bool)):
        raise ProtocolError('Invalid native window relationships')
    if not value['parent_supported'] and (value.get('parent') is not None or value.get('modal') is not None):
        raise ProtocolError('Unsupported window relationship carries facts')
    return dict(parent_supported=value['parent_supported'], parent=value.get('parent'), modal=value.get('modal'))


def contexts(windows, main_by_app=None):
    """Resolve one sampled live graph, with optional trusted adapter declarations.

    main_by_app is supplied by the caller's application adapter, never loaded
    from a client-provided title, focus history or persisted stale window ID.
    Each declaration names an already observed app and its live main window.
    The function does not dispatch document commands or authenticate adapters.
    """
    if any(type(window.get('id')) not in (str, int) or isinstance(window['id'], bool) for window in windows):
        raise ProtocolError('Invalid window identity in context graph')
    by_id = {window['id']: window for window in windows}
    if len(by_id) != len(windows):
        raise ProtocolError('Duplicate window in context graph')
    result = {}
    for window in windows:
        chain, seen, current = [], set(), window
        while current is not None:
            if current['id'] in seen:
                raise ProtocolError('Cyclic native parent graph')
            seen.add(current['id']); chain.append(current)
            parent = current.get('parent')
            current = by_id.get(parent) if parent is not None else None
        # Native parent relation is sufficient for advertised application
        # context, but does not establish which ancestor is a document.
        hints = [item.get('app_hint') for item in reversed(chain) if item.get('app_hint')]
        result[window['id']] = dict(application_hint=hints[0] if hints else None,
            context_window=chain[-1]['id'], parent=window.get('parent') if window.get('parent') in by_id else None,
            main_window=None, main_source='unavailable')
    for app, main in (main_by_app or {}).items():
        if not isinstance(app, str) or not app or type(main) not in (str, int) or main not in by_id:
            raise ProtocolError('Adapter main window is unavailable')
        if result[main]['application_hint'] != app:
            raise ProtocolError('Adapter main window belongs to another application context')
        for context in result.values():
            if context['application_hint'] == app:
                context.update(main_window=main, main_source='application_adapter')
    return result


def activation_target(windows, target):
    """Redirect only to a live, explicitly modal descendant in this branch.

    Ambiguous sibling modal requests are rejected rather than choosing an
    arbitrary window. Utility/transient parenthood alone never steals focus.
    """
    by_id = {window['id']: window for window in windows}
    if target not in by_id:
        raise ProtocolError('Activation target is unavailable')
    seen = set()
    while True:
        if target in seen:
            raise ProtocolError('Cyclic modal parent graph')
        seen.add(target)
        children = [window for window in windows if window.get('parent') == target and window.get('modal') is True]
        if len(children) > 1:
            raise ProtocolError('Ambiguous modal descendants')
        if not children:
            return target
        target = children[0]['id']


def tree_context(tree):
    """Read protocol observations without copying document titles or content."""
    windows = []
    def walk(node):
        if node.get('app_id') is not None or node.get('window') is not None:
            relation = relationships(node.get('lumen_relationships'))
            windows.append(dict(id=node['id'], app_hint=node.get('app_id') or
                (node.get('window_properties') or {}).get('class'),
                parent=relation['parent'] if relation else None,
                modal=relation['modal'] if relation else None))
        for child in node.get('nodes', []) + node.get('floating_nodes', []):
            walk(child)
    walk(tree)
    return contexts(windows)
