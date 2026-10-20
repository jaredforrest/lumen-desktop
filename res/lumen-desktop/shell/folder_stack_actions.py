"""Validate one captured stack item before fixed Open or owned directory handoff."""
import json
import stat
import subprocess
import sys
from captured_resource_actions import CapturedResourceActions,ResourceUnavailable,parse_facts,wire_facts,unavailable


def handle(request,runner=subprocess.run):
    if not isinstance(request,dict) or set(request)!={'path','facts'}:raise ResourceUnavailable('invalid_request','Invalid folder stack selection')
    expected=parse_facts(request['facts']);broker=CapturedResourceActions();key=broker.capture(request['path'],expected)
    try:
        entry=broker.selected(key)
        if stat.S_ISDIR(entry['facts'][2]):return dict(folder=str(entry['path']),expectedFacts=wire_facts(entry['facts']))
        return broker.dispatch(key,'open',runner=runner)
    finally:broker.retire([key])


def main():
    try:
        line=sys.stdin.buffer.readline(65537)
        if len(line)>65536:raise ResourceUnavailable('invalid_request','The folder stack selection is too large')
        result=handle(json.loads(line))
    except Exception as exc:
        print(json.dumps(unavailable(exc)),flush=True);return 1
    print(json.dumps(result),flush=True);return 0


if __name__=='__main__':raise SystemExit(main())
