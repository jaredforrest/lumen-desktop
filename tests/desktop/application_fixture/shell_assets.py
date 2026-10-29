"""Disposable full-shell copy with explicit unavailable unrelated producers."""
import hashlib
import json
import shutil
import sys
from pathlib import Path
from fixture_providers import isolate_providers


def prepare(repo, output):
    repo,output=map(Path,(repo,output))
    assets=output/'assets';shutil.copytree(repo/'res/lumen-desktop',assets)
    for directory in [assets,*[p for p in assets.rglob('*')if p.is_dir()]]:directory.chmod(0o700)
    for path in assets.rglob('*'):
        if path.is_file():path.chmod(0o600)
    isolate_providers(assets/'shell',output)
    edits=[]
    def edit(name,transform,scope):
        path=assets/'shell'/name;before=path.read_text();after=transform(before);assert before!=after
        path.write_text(after);edits.append(dict(path=name,scope=scope,before=hashlib.sha256(before.encode()).hexdigest(),after=hashlib.sha256(after.encode()).hexdigest()))
    edit('AudioState.qml',lambda text:text.replace('import Quickshell.Services.Pipewire','').replace('id: audio','id: audio\n QtObject {id:privateAudio;property var nodes:({values:[]});property var defaultAudioSource:null;property var defaultAudioSink:null;property bool ready:false;property var preferredDefaultAudioSource:null;property var preferredDefaultAudioSink:null}').replace('Pipewire.','privateAudio.').replace('PwObjectTracker { objects:','QtObject {property var objects:'),'Unavailable audio controls; separate real private null sink supplies QtMultimedia')
    for name in ['ConnectivityPanel.qml','ControlCentre.qml']:
        edit(name,lambda text:text.replace('import Quickshell.Bluetooth','').replace('Bluetooth.defaultAdapter','null'),'Unavailable Bluetooth adapter; no host system bus')
    edit('SystemPanels.qml',lambda text:text.replace('import Quickshell.Bluetooth',''),'No Bluetooth singleton initialization')
    helper=output/'network-unavailable.py';helper.write_text('import json\nprint(json.dumps(dict(status="unknown",reason="unavailable",message="Private fixture")))\n')
    edit('SystemState.qml',lambda text:text.replace('"desktop-network-diagnose"',json.dumps(sys.executable)+','+json.dumps(str(helper))),'Unavailable internet monitor; no host command')
    # Identity, client discovery, shell surfaces and actual command routing are unmodified.
    (output/'optional-provider-isolation.json').write_text(json.dumps(edits,indent=2)+'\n')
    return assets
