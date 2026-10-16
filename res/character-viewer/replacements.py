"""Match explicit saved abbreviations; never retain or log application text."""
import json
import os
from pathlib import Path


def replacement_for(prefix):
    folder=Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop/preferences'
    try:
        options=json.loads((folder/'text-tools-options.json').read_text())
        if not isinstance(options,dict) or not options.get('automaticReplacements',True):return None
    except FileNotFoundError:pass
    if not prefix or prefix[-1] not in ' \t\n':return None
    rows=json.loads((folder/'text-tools.json').read_text())
    if not isinstance(rows,list):return None
    for row in sorted((r for r in rows[:100] if isinstance(r,dict) and isinstance(r.get('name'),str) and isinstance(r.get('text'),str)),key=lambda r:len(r['name']),reverse=True):
        name,text=row['name'],row['text']
        if not name or len(name)>40 or not text or len(text)>8192 or any(c.isspace() for c in name):continue
        ending=name+prefix[-1]
        if prefix.endswith(ending):
            before=prefix[:-len(ending)]
            if before and (before[-1].isalnum() or before[-1]=='_'):continue
            return len(ending.encode('utf-8')),text+prefix[-1]
    return None
