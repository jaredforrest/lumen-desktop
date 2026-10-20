"""Explicit user edits to consumer-owned display-space preferences."""
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'runtime'))
from display_space_policy import preferences, identity_record
from display_space_runtime import configured
from nix_settings import set_section


def handle(data):
    if not isinstance(data,dict) or set(data)!={'key','value'} or data['key'] not in ('separateDisplays','primaryDisplay'):
        raise ValueError('Invalid display-space preference action')
    current=configured()
    value=dict(separateDisplays=current['separateDisplays'],primaryDisplay=identity_record(current['primaryDisplay']))
    value[data['key']]=data['value']
    parsed=preferences(value)
    canonical=dict(separateDisplays=parsed['separateDisplays'],primaryDisplay=identity_record(parsed['primaryDisplay']))
    set_section('spaces',canonical)
    return dict(ok=True)


if __name__=='__main__':
    try:print(json.dumps(handle(json.loads(sys.stdin.readline()))))
    except Exception as error:print(json.dumps(dict(error=str(error))))
