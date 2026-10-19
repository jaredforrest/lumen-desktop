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
            yield

    def save(self):
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        fd, name = tempfile.mkstemp(dir=self.path.parent, prefix=".spaces-")
        try:
            with os.fdopen(fd, "w") as stream:
                json.dump(self.state, stream, sort_keys=True)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(name, self.path)
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
        self.outputs = {self.display(o): o["name"] for o in outputs if o.get("active", True)}
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
                self.state["desktops"][key] = {"ordinal": len(self.state["desktops"]), "recent": 0}
            self.state["desktops"][key].update(name=workspace["name"], output=workspace["output"])
            current[workspace["id"]] = key
        self.current = current
        self.workspaces = workspaces
        self.save()

    def focus(self, compositor_id):
        key = self.current.get(compositor_id)
        if key:
            self.state["counter"] += 1
            self.state["desktops"][key]["recent"] = self.state["counter"]
            self.save()

    def order(self, output=None):
        keys = [key for key in self.current.values() if output is None or self.state["desktops"][key]["output"] == output]
        field = "recent" if self.state["auto_reorder"] else "ordinal"
        return sorted(keys, key=lambda key: ((-1 if field == "recent" else 1) * self.state["desktops"][key][field], self.state["desktops"][key]["ordinal"]))

    def configure(self, *, auto_reorder):
        if type(auto_reorder) is not bool:
            raise ValueError("Automatic ordering must be boolean")
        self.state["auto_reorder"] = auto_reorder
        self.save()

    def assign(self, application, kind, target_id=None):
        if not isinstance(application, str) or not application.startswith(("wayland:", "x11:")) or not application.partition(":")[2]:
            raise ValueError("An exact typed application identity is required")
        if kind == "all":
            raise ValueError("All-desktop placement is unavailable: compositor lacks all-window membership")
        if kind not in ("none", "desktop", "display") or (kind == "none") != (target_id is None):
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


def main():
    import argparse
    import subprocess
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
        stream = subprocess.Popen(['swaymsg', '-r', '-m', '-t', 'subscribe', '["window","workspace","output"]'], stdout=subprocess.PIPE, text=True)
        pending = set()
        try:
            for line in stream.stdout:
                event = json.loads(line)
                with policy.transaction():
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
        finally:
            stream.terminate()
            stream.wait(timeout=5)
        return
    with policy.transaction():
        refresh(policy)
        if args.action == 'assign':
            policy.assign(args.application, args.kind, args.target)
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
            print(json.dumps(dict(desktops=policy.state['desktops'], order=policy.order(), assignments=policy.state['assignments'], auto_reorder=policy.state['auto_reorder'], capabilities={'all_desktops': False}, outputs=policy.outputs)))


if __name__ == '__main__':
    main()
