"""Persistent logical desktops and exact application placement policy.

Compositor IDs are session-local. Logical IDs survive restarts; order is separate
from identity. Application identities are advertised, never authentication.
"""
import json
import os
from pathlib import Path
import tempfile
import uuid
from contextlib import contextmanager
import fcntl


class SpacePolicy:
    def __init__(self, path):
        self.path = Path(path)
        self.state = {"version": 1, "desktops": {}, "assignments": {}, "counter": 0, "auto_reorder": True}
        if self.path.exists():
            data = json.loads(self.path.read_text())
            if data.get("version") != 1 or not isinstance(data.get("desktops"), dict) or not isinstance(data.get("assignments"), dict):
                raise ValueError("Invalid desktop policy state")
            self.state = data
        self.state.setdefault("revision", 0)
        self.state.setdefault("created_counter", 0)
        self.current = {}
        self.outputs = {}

    @contextmanager
    def transaction(self):
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with open(str(self.path) + ".lock", "a", encoding="utf-8") as lock:
            os.chmod(lock.name, 0o600)
            fcntl.flock(lock, fcntl.LOCK_EX)
            if self.path.exists():
                self.state = json.loads(self.path.read_text())
            self.state.setdefault("revision", 0)
            self.state.setdefault("created_counter", 0)
            yield

    def save(self):
        self.state["revision"] = self.state.get("revision", 0) + 1
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        fd, name = tempfile.mkstemp(dir=self.path.parent, prefix=".spaces-")
        try:
            with os.fdopen(fd, "w") as stream:
                json.dump(self.state, stream, sort_keys=True)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(name, self.path)
            directory = os.open(self.path.parent, os.O_RDONLY | os.O_DIRECTORY)
            try:os.fsync(directory)
            finally:os.close(directory)
        finally:
            if os.path.exists(name):
                os.unlink(name)

    @staticmethod
    def application(node):
        if node.get("app_id"):
            return "wayland:" + node["app_id"]
        value = node.get("window_properties", {}).get("class")
        return "x11:" + value if value else None

    @staticmethod
    def display(output):
        identity = [output.get(k, "") for k in ("make", "model", "serial")]
        if all(identity) and identity[2] not in ("Unknown", "0"):
            return "display:" + json.dumps(identity, separators=(",", ":"))
        return "connector:" + output["name"]

    def reconcile(self, workspaces, outputs):
        before = json.dumps(self.state, sort_keys=True)
        self.outputs = {self.display(o): o["name"] for o in outputs if o.get("active", True)}
        focused_output = next((o["name"] for o in outputs if o.get("active", True) and o.get("focused")), next(iter(self.outputs.values()), None))
        previous = self.current
        current = {}
        for workspace in workspaces:
            if workspace["name"] == "__i3_scratch":
                continue
            key = previous.get(workspace["id"])
            if key is None:
                key = next((key for key, item in self.state["desktops"].items() if item["name"] == workspace["name"]), None)
            if key is None:
                key = str(uuid.uuid4())
                self.state["desktops"][key] = {"ordinal": max((item["ordinal"] for item in self.state["desktops"].values()), default=-1) + 1, "recent": 0}
            self.state["desktops"][key].update(name=workspace["name"], output=workspace["output"],
                output_id=next((oid for oid, name in self.outputs.items() if name == workspace["output"]), None))
            current[workspace["id"]] = key
        self.current = current
        self.workspaces = workspaces
        for key, item in self.state["desktops"].items():
            if not item.get("persistent") or key in current.values():
                continue
            if item.get("output_id") in self.outputs:
                item["output"] = self.outputs[item["output_id"]]
            elif focused_output is not None:
                # An empty logical desktop has no native windows to strand.
                item["output"] = focused_output
                item["output_id"] = next(oid for oid, name in self.outputs.items() if name == focused_output)
        if json.dumps(self.state, sort_keys=True) != before:
            self.save()

    def focus(self, compositor_id):
        key = self.current.get(compositor_id)
        if key:
            self.state["counter"] += 1
            self.state["desktops"][key]["recent"] = self.state["counter"]
            self.save()

    def order(self, output=None):
        keys = [key for key, item in self.state["desktops"].items()
                if (key in self.current.values() or item.get("persistent"))
                and (output is None or item["output"] == output)]
        field = "recent" if self.state["auto_reorder"] else "ordinal"
        return sorted(keys, key=lambda key: ((-1 if field == "recent" else 1) * self.state["desktops"][key][field], self.state["desktops"][key]["ordinal"]))

    def require_revision(self, revision):
        if type(revision) is not int or revision != self.state["revision"]:
            raise ValueError("Desktop list changed; refresh and try again")

    def catalog(self):
        live = {key: wid for wid, key in self.current.items()}
        by_id = {workspace["id"]: workspace for workspace in self.workspaces}
        return [dict(logicalId=key, name=self.state["desktops"][key]["name"],
            label=self.state["desktops"][key].get("label", "Desktop " + self.state["desktops"][key]["name"]),
            output=self.state["desktops"][key]["output"], outputId=self.state["desktops"][key].get("output_id"),
            available=self.state["desktops"][key]["output"] in self.outputs.values(),
            persistent=bool(self.state["desktops"][key].get("persistent")),
            id=live.get(key), focused=bool(by_id.get(live.get(key), {}).get("focused")),
            visible=bool(by_id.get(live.get(key), {}).get("visible"))) for key in self.order()]

    def desktop(self, key, *, connected=True):
        item = next((item for item in self.catalog() if item["logicalId"] == key), None)
        if item is None:
            raise ValueError("This desktop is no longer available")
        if connected and not item["available"]:
            raise ValueError("The desktop display is disconnected")
        return item

    def create(self, output_id, revision):
        self.require_revision(revision)
        if output_id not in self.outputs:
            raise ValueError("The desktop display is disconnected")
        if len(self.catalog()) >= 64:
            raise ValueError("The desktop limit is 64")
        key = str(uuid.uuid4())
        numbers = [int(item["name"]) for item in self.state["desktops"].values() if item["name"].isdigit()]
        self.state["created_counter"] = max(self.state["created_counter"], max(numbers, default=0)) + 1
        self.state["desktops"][key] = dict(name="lumen:" + key, label="Desktop " + str(self.state["created_counter"]),
            persistent=True, output=self.outputs[output_id], output_id=output_id,
            ordinal=max((item["ordinal"] for item in self.state["desktops"].values()), default=-1) + 1, recent=0)
        self.save()
        return self.desktop(key)

    def reorder(self, output_id, order, revision):
        self.require_revision(revision)
        if output_id not in self.outputs:
            raise ValueError("The desktop display is disconnected")
        current = self.order(self.outputs[output_id])
        if (not isinstance(order, list) or any(not isinstance(key, str) for key in order)
                or len(order) != len(set(order)) or set(order) != set(current)):
            raise ValueError("Manual order must contain every desktop on this display exactly once")
        slots = sorted(self.state["desktops"][key]["ordinal"] for key in current)
        for key, ordinal in zip(order, slots):
            self.state["desktops"][key]["ordinal"] = ordinal
        self.state["auto_reorder"] = False
        self.save()

    def remove(self, source, destination, revision):
        """Retire only after the caller has verified native evacuation."""
        self.require_revision(revision)
        old = self.state["desktops"].get(source)
        target = self.desktop(destination)
        if old is None:
            raise ValueError("This desktop is no longer available")
        if source == destination or old.get("output_id") != target["outputId"]:
            raise ValueError("Choose a different surviving desktop on the same display")
        if source in self.current.values():
            raise ValueError("The desktop still exists in the compositor")
        del self.state["desktops"][source]
        self.save()

    def configure(self, *, auto_reorder):
        if type(auto_reorder) is not bool:
            raise ValueError("Automatic ordering must be boolean")
        self.state["auto_reorder"] = auto_reorder
        self.save()

    def assign(self, application, kind, target_id=None):
        if not isinstance(application, str) or not application.startswith(("wayland:", "x11:")) or not application.partition(":")[2]:
            raise ValueError("An exact typed application identity is required")
        if kind not in ("none", "all", "desktop", "display") or (kind in ("none", "all")) != (target_id is None):
            raise ValueError("Invalid application assignment")
        if target_id is not None and (not isinstance(target_id, str) or not target_id):
            raise ValueError("Invalid assignment target")
        self.state["assignments"][application] = {"kind": kind, "target_id": target_id}
        self.save()

    def destination(self, node):
        assignment = self.state["assignments"].get(self.application(node), {"kind": "none", "target_id": None})
        kind, target = assignment["kind"], assignment["target_id"]
        if kind == "none":
            return {"status": "unassigned"}
        if kind == "all":
            return {"status": "membership", "scope": "current-display", "assignment": assignment}
        if kind == "desktop":
            item = self.state["desktops"].get(target)
            if item and target in self.current.values():
                return {"status": "available", "workspace": item["name"]}
            if item and item["output"] in self.outputs.values():
                return {"status": "available", "workspace": item["name"], "recreate_on_output": item["output"]}
        elif kind == "display":
            connector = self.outputs.get(target)
            visible = next((w for w in self.workspaces if w["output"] == connector and w.get("visible")), None)
            if visible:
                return {"status": "available", "workspace": visible["name"]}
        return {"status": "unavailable", "reason": "Assignment target is unavailable", "fallback": "keep-current-placement", "assignment": assignment}


def sway(*arguments):
    import subprocess
    return json.loads(subprocess.check_output(['swaymsg', '-r', *arguments], timeout=5))


def refresh(policy):
    policy.reconcile(sway('-t', 'get_workspaces'), sway('-t', 'get_outputs'))


def place(policy, node):
    """Move a newly mapped window only; never change keyboard focus."""
    result = policy.destination(node)
    if result['status'] == 'available':
        identifier = node.get('id')
        if type(identifier) is not int or identifier <= 0:
            raise ValueError('Invalid compositor window identity')
        command = '[con_id=' + str(identifier) + '] move container to workspace ' + json.dumps(result['workspace'])
        if result.get('recreate_on_output'):
            command += '; [con_id=' + str(identifier) + '] move workspace to output ' + json.dumps(result['recreate_on_output'])
        reply = sway(command)
        if not all(item.get('success') for item in reply):
            raise RuntimeError('Compositor rejected application placement')
    return result


def checked_membership(command):
    reply = sway(command)
    if not reply or not all(item.get('success') for item in reply):
        raise RuntimeError('Compositor rejected desktop membership')


def main():
    import argparse
    from application_desktops import reconcile as reconcile_membership
    parser = argparse.ArgumentParser()
    parser.add_argument('--state', default=str(Path(os.environ.get('XDG_STATE_HOME', Path.home() / '.local/state')) / 'lumen-desktop/spaces.json'))
    sub = parser.add_subparsers(dest='action', required=True)
    sub.add_parser('status')
    sub.add_parser('watch')
    order = sub.add_parser('ordering')
    order.add_argument('enabled', choices=['true', 'false'])
    assign = sub.add_parser('assign')
    assign.add_argument('application')
    assign.add_argument('kind', choices=['none', 'all', 'desktop', 'display'])
    assign.add_argument('target', nargs='?')
    switch = sub.add_parser('switch')
    switch.add_argument('direction', choices=['previous', 'next'])
    args = parser.parse_args()
    policy = SpacePolicy(args.state)
    if args.action == 'watch':
        import socket
        import select
        from application_lifecycle import send, receive
        pending = set()
        # Subscribe and acknowledge before the initial snapshot. swaymsg's monitor
        # output does not publish its subscription acknowledgement to a pipe.
        with socket.socket(socket.AF_UNIX) as stream:
            stream.settimeout(5)
            stream.connect(os.environ['SWAYSOCK'])
            send(stream, 2, '["window","workspace","output"]')
            _, ack = receive(stream)
            if not isinstance(ack, dict) or ack.get('success') is not True:
                raise RuntimeError('Compositor subscription failed')
            with policy.transaction():
                refresh(policy)
                reconcile_membership(policy, sway('-t', 'get_tree'), checked_membership)
            print(json.dumps({"ready": True}), flush=True)
            stream.settimeout(None)
            while True:
                # Ownership/role changes need not emit a window event. Recheck
                # pending membership without inventing a client role from its title.
                ready, _, _ = select.select([stream], [], [], 1)
                event = receive(stream)[1] if ready else {}
                with policy.transaction():
                    if not ready and not policy.state.get("managed_sticky") and not any(item.get("kind") == "all" for item in policy.state["assignments"].values()):
                        continue
                    refresh(policy)
                    if event.get('change') == 'focus' and event.get('current', {}).get('type') == 'workspace':
                        policy.focus(event['current']['id'])
                    node = event.get('container', {})
                    if event.get('change') == 'new' and node:
                        pending.add(node['id'])
                    if node.get('id') in pending and policy.application(node):
                        print(json.dumps(place(policy, node)), flush=True)
                        pending.remove(node['id'])
                    if event.get('change') == 'close':
                        pending.discard(node.get('id'))
                    reconcile_membership(policy, sway('-t', 'get_tree'), checked_membership)
        return
    with policy.transaction():
        refresh(policy)
        if args.action == 'assign':
            tree = sway('-t', 'get_tree')
            from application_desktops import windows
            if args.kind == 'all' and any(window.get('_grouped') and policy.application(window) == args.application for window in windows(tree)):
                raise ValueError('Turn off window groups before assigning all desktops')
            policy.assign(args.application, args.kind, args.target)
            reconcile_membership(policy, tree, checked_membership)
        elif args.action == 'ordering':
            policy.configure(auto_reorder=args.enabled == 'true')
        elif args.action == 'switch':
            focused = next((w for w in policy.workspaces if w.get('focused')), None)
            if focused:
                order = policy.order(focused['output'])
                index = order.index(policy.current[focused['id']]) + (-1 if args.direction == 'previous' else 1)
                if 0 <= index < len(order):
                    sway('workspace ' + json.dumps(policy.state['desktops'][order[index]]['name']))
        else:
            print(json.dumps(dict(desktops=policy.state['desktops'], order=policy.order(), assignments=policy.state['assignments'], auto_reorder=policy.state['auto_reorder'], capabilities={'all_desktops': {'supported': True, 'scope': 'current-display', 'window_kind': 'independent-floating'}}, outputs=policy.outputs)))


if __name__ == '__main__':
    main()
