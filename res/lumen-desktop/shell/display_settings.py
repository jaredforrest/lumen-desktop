"""Validated Sway display edits with an independent, fail-safe confirmation timer."""
import fcntl
import json
import os
from pathlib import Path
import re
import select
import subprocess
import sys
import time
import uuid
from nix_settings import read_settings, set_section


def output_data():
    return json.loads(subprocess.check_output(['swaymsg', '-r', '-t', 'get_outputs'], timeout=4))


def mode_string(mode):
    return f"{mode['width']}x{mode['height']}@{mode['refresh']/1000:.3f}Hz"


def identity(output):
    return [output['name'], output.get('make'), output.get('model'), output.get('serial')]


def setting(output):
    return dict(name=output['name'], identity=identity(output), mode=mode_string(output['current_mode']),
                scale=output['scale'], x=output['rect']['x'], y=output['rect']['y'])


def validate(data, outputs):
    output = next((o for o in outputs if o['name']==data.get('name') and o.get('active')), None)
    if output is None:
        raise ValueError('The display is no longer connected')
    mode = data.get('mode')
    reported={mode_string(m) for m in output['modes']}
    if output.get('current_mode'):reported.add(mode_string(output['current_mode']))
    if mode not in reported:
        raise ValueError('Choose a mode reported by this display')
    scale = float(data.get('scale', 1))
    if scale not in (1, 1.25, 1.5, 1.75, 2, 2.5, 3):
        raise ValueError('Choose a supported scale')
    x, y = int(data.get('x', 0)), int(data.get('y', 0))
    if not -32768 <= x <= 32767 or not -32768 <= y <= 32767:
        raise ValueError('Display position is out of range')
    return dict(name=output['name'], identity=identity(output), mode=mode, scale=scale, x=x, y=y)


def apply_one(data):
    result = json.loads(subprocess.check_output(['swaymsg', '-r',
        'output '+json.dumps(data['name'])+' mode '+json.dumps(data['mode'])+
        ' scale '+str(data['scale'])+' position '+str(data['x'])+' '+str(data['y'])], timeout=5))
    if not all(r.get('success') for r in result):
        raise RuntimeError('The compositor rejected the display setting')


def paths():
    runtime = Path(os.environ['XDG_RUNTIME_DIR'])/'lumen-display-change.json'
    saved = None  # Durable layouts are owned by settings/desktop.nix.
    return runtime, saved


def atomic(file, data):
    file.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    temp = file.with_suffix('.tmp')
    with temp.open('w') as stream:
        os.chmod(temp, 0o600)
        json.dump(data, stream)
        stream.flush()
        os.fsync(stream.fileno())
    temp.replace(file)


def read(file, fallback):
    try:
        value=json.loads(file.read_text())
        return value if isinstance(value,type(fallback)) else fallback
    except (OSError, ValueError):return fallback


def revert(pending, token=None):
    state = read(pending, {})
    if not state or token is not None and state.get('token')!=token:return
    outputs = output_data()
    errors = []
    for previous in state['previous']:
        if any(identity(o)==previous['identity'] and o.get('active') for o in outputs):
            try:apply_one(previous)
            except (OSError, RuntimeError, subprocess.SubprocessError) as exc:errors.append(str(exc))
    if errors:raise RuntimeError('; '.join(errors))
    pending.unlink(missing_ok=True)


def handle(data):
    pending, saved = paths()
    action = data.get('action', 'status')
    if action=='status':
        outputs = [dict(setting(o), label=" ".join(str(o.get(k) or "") for k in ("make","model")).strip() or o["name"], modes=list(dict.fromkeys([mode_string(o['current_mode'])]+[mode_string(m) for m in o['modes']]))) for o in output_data() if o.get('active') and o.get('current_mode')]
        state = read(pending, {})
        return dict(outputs=outputs, pending=state.get('token',''), seconds=max(0,int(state.get('deadline',0)-time.time())))
    with pending.with_suffix('.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if action=='apply':
            if pending.exists():raise ValueError('Confirm or revert the pending display change first')
            outputs=output_data();candidate=validate(data,outputs);token=uuid.uuid4().hex
            state=dict(token=token,candidate=candidate,previous=[setting(o) for o in outputs if o.get('active') and o.get('current_mode')],deadline=time.time()+20)
            atomic(pending,state)
            # Keep the watchdog outside the desktop shell's cgroup so a shell crash
            # or reload cannot strand the user with an unconfirmed display mode.
            command=['lumen-launch-app','--',sys.executable,str(Path(__file__).resolve()),'guard',token]
            guard=subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
            if not select.select([guard.stdout],[],[],5)[0] or guard.stdout.readline().strip()!=b'ready':
                pending.unlink(missing_ok=True);raise RuntimeError('Could not start display rollback protection; no settings changed')
            guard.stdout.close()
            try:apply_one(candidate)
            except Exception:
                revert(pending,token);raise
            return dict(pending=token,seconds=20)
        if action=='confirm':
            state=read(pending,{})
            if not state or state.get('token')!=data.get('token') or time.time()>=state['deadline']:
                raise ValueError('The confirmation period has expired')
            current=output_data()
            candidate=state['candidate']
            if not any(identity(o)==candidate['identity'] and o.get('active') for o in current):
                raise ValueError('The display disconnected; the change was not saved')
            layouts=read_settings()['displays']
            layouts[candidate['name']]=candidate
            # The user has accepted the visible mode. End the safety countdown,
            # but keep the pending marker so the watcher does not restore the
            # old configured layout during the authenticated NixOS rebuild.
            state['confirmed']=True
            atomic(pending,state)
            try:set_section('displays',layouts)
            except Exception:
                revert(pending,state['token'])
                raise
            pending.unlink(missing_ok=True)
            return dict(ok=True)
        if action=='revert':revert(pending,data.get('token'));return dict(ok=True)
        if action=='forget':set_section('displays',{});return dict(ok=True)
        raise ValueError('Unknown display action')


def guard(token):
    pending,_=paths()
    print('ready',flush=True)
    while True:
        state=read(pending,{})
        if state.get('token')!=token or state.get('confirmed'):return
        if time.time()>=state['deadline']:
            with pending.with_suffix('.lock').open('a') as lock:
                fcntl.flock(lock,fcntl.LOCK_EX);revert(pending,token)
            return
        time.sleep(.25)


def restore_saved():
    pending,saved=paths()
    if pending.exists():return
    current=output_data()
    for value in read_settings()['displays'].values():
        if not isinstance(value,dict):continue
        output=next((o for o in current if o.get('active') and identity(o)==value.get('identity')),None)
        if output and setting(output)!=value:
            try:apply_one(validate(value,current))
            except (ValueError,OSError,RuntimeError,subprocess.SubprocessError):pass


def main():
    os.umask(0o077)
    if len(sys.argv)>1 and sys.argv[1]=='guard':guard(sys.argv[2]);return
    if len(sys.argv)>1 and sys.argv[1]=='watch':
        restore_saved()
        with subprocess.Popen(['swaymsg','-m','-r','-t','subscribe','["output"]'],stdout=subprocess.PIPE,text=True) as events:
            for line in events.stdout:
                time.sleep(.3);restore_saved()
        return
    try:print(json.dumps(handle(json.loads(sys.stdin.readline()))))
    except Exception as exc:print(json.dumps(dict(error=str(exc))))


if __name__=='__main__':main()
