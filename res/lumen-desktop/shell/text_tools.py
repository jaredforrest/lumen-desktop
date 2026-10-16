"""Explicit local text actions; no background keystroke capture or clipboard replacement."""
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import dbus


def snippets(value=None):
    # Snippet text is personal content; enabling replacements is Nix policy.
    import fcntl
    file=Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'lumen-desktop/content/snippets.json'
    file.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    with file.with_suffix('.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        if value is not None:
            fd,name=tempfile.mkstemp(dir=file.parent)
            with os.fdopen(fd,'w') as stream:
                json.dump(value,stream,ensure_ascii=False);stream.flush();os.fsync(stream.fileno())
            os.replace(name,file)
        try:rows=json.loads(file.read_text())
        except (OSError,ValueError):rows=[]
        return rows if isinstance(rows,list) else []


def focused(node):
    if node.get('focused') and node.get('type') in ('con','floating_con'):return node
    for child in node.get('nodes',[])+node.get('floating_nodes',[]):
        result=focused(child)
        if result:return result


def selection():
    result=subprocess.run(['wl-paste','--primary','--no-newline','--type','text'],capture_output=True,timeout=3)
    if result.returncode:raise ValueError('This app does not expose selected text. Paste text into this panel to use the tools.')
    text=result.stdout.decode('utf-8')
    if not text.strip() or len(text)>16384:raise ValueError('Select between 1 and 16,384 characters')
    return text


def menu_command(path, expected_app=None):
    api=dbus.Interface(dbus.SessionBus().get_object('com.canonical.AppMenu.Registrar','/com/canonical/AppMenu/Registrar'),'org.lumendesktop.GlobalMenuPrototype')
    state=json.loads(api.State())
    if expected_app and state.get('appName')!=expected_app:raise ValueError('This shortcut belongs to another application')
    def clean(s):return s.replace('__','\0').replace('_','').replace('\0','_')
    matches=[]
    def walk(node,parents):
        label=clean(node.get('label',''));parts=parents+([label] if label else [])
        if node.get('enabled') and node.get('visible') and not node.get('submenu') and not node.get('separator') and '/'.join(parts)==path:matches.append(node['id'])
        for child in node.get('children',[]):walk(child,parts)
    if not state.get('supported'):raise ValueError('The focused app has no exported menu')
    for child in state['menu']['children']:walk(child,[])
    if len(matches)!=1 or not api.Activate(state['token'],matches[0]):raise ValueError('Menu command is unavailable, ambiguous, or focus changed')
    return dict(ok=True)


def handle(data):
    action=data.get('action')
    if action in ('spelling','learn','detect','copy','ocr'):
        from context_tools import handle as context_action
        return context_action(data)
    if action and action.startswith('dictation-'):
        from dictation import handle as dictation_action
        return dictation_action(data)
    from nix_settings import read_settings, set_section
    if action in ('status','automatic-replacements','language'):
        options=read_settings()['textTools']
        if action=='automatic-replacements':
            options['automaticReplacements']=bool(data['enabled'])
            options=set_section('textTools',options)['textTools']
        if action=='language':
            from context_tools import language
            options['language']=language(data['value'])
            options=set_section('textTools',options)['textTools']
        return dict(snippets=snippets(),automaticReplacements=options['automaticReplacements'],language=options.get('language','en_GB'))
    if action=='save-shortcut':
        slot=int(data['slot']);path=str(data['path']);app=str(data['app'])
        if not 1<=slot<=12 or not app or len(app)>256 or '/' not in path or len(path)>512:raise ValueError('Choose a slot, application and exact menu path such as File/Save')
        shortcuts=read_settings()['menuShortcuts']
        shortcuts[str(slot)]=dict(app=app,path=path)
        set_section('menuShortcuts',shortcuts)
        return dict(message='Saved Super+Alt+F'+str(slot)+' in Nix for '+app)
    if action=='selection':return dict(text=selection())
    if action=='save':
        name,text=str(data.get('name','')).strip(),str(data.get('text',''))
        if not name or len(name)>80 or not text or len(text)>8192:raise ValueError('Provide a short name and 1–8,192 characters of replacement text')
        rows=[r for r in snippets() if r['name']!=name]+[dict(name=name,text=text)]
        if len(rows)>100:raise ValueError('Keep no more than 100 replacements')
        snippets(rows);return dict(snippets=rows)
    if action=='delete':
        rows=[r for r in snippets() if r['name']!=data.get('name')]
        snippets(rows);return dict(snippets=rows)
    if action=='menu':return menu_command(data['path'])
    if action=='stop':
        subprocess.run(['systemctl','--user','stop','lumen-text-speech.service'],check=False,capture_output=True,timeout=5)
        return dict(ok=True)
    text=str(data.get('text',''))
    if not text or len(text)>16384:raise ValueError('Enter between 1 and 16,384 characters')
    if action=='lookup':
        word=text.strip()
        if not re.fullmatch(r"[A-Za-z][A-Za-z '-]{0,79}",word):raise ValueError('Look up one English word or short phrase')
        result=subprocess.run(['wn',word,'-over'],capture_output=True,text=True,timeout=6)
        return dict(definition=result.stdout[:24000].strip() or 'No entry in the local English dictionary')
    if action=='speak':
        handle({'action':'stop'})
        fd,file=tempfile.mkstemp(prefix='lumen-speech-',suffix='.txt',dir=os.environ['XDG_RUNTIME_DIR'])
        with os.fdopen(fd,'w') as stream:stream.write(text)
        try:
            subprocess.run(['systemd-run','--user','--collect','--quiet','--unit=lumen-text-speech',
                '--property=RuntimeMaxSec=600','--setenv=PATH='+os.environ['PATH'],sys.executable,str(Path(__file__).resolve()),'speech-worker',file,str(data.get('language','en_GB'))],check=True,capture_output=True,timeout=5)
        except Exception:Path(file).unlink(missing_ok=True);raise
        return dict(ok=True)
    if action=='insert':
        target=int(data['target'])
        deadline=time.monotonic()+.6
        while time.monotonic()<deadline:
            node=focused(json.loads(subprocess.check_output(['swaymsg','-r','-t','get_tree'],timeout=2)))
            if node and node['id']==target:
                subprocess.run(['wtype','--',text],check=True,timeout=15)
                return dict(ok=True)
            if node:raise ValueError('The original text field is no longer focused; nothing was inserted')
            time.sleep(.03)
        raise ValueError('The original window did not regain focus')
    raise ValueError('Unknown text action')


if __name__=='__main__':
    os.umask(0o077)
    if len(sys.argv)>1 and sys.argv[1]=='speech-worker':
        import signal
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
        file=Path(sys.argv[2])
        from context_tools import language
        voice='en-gb' if language(sys.argv[3] if len(sys.argv)>3 else None)=='en_GB' else 'en-us'
        try:subprocess.run(['espeak-ng','-v',voice,'-f',str(file)],check=True,timeout=600)
        finally:file.unlink(missing_ok=True)
    elif len(sys.argv)>2 and sys.argv[1]=='slot':
        try:
            from nix_settings import read_settings
            shortcuts=read_settings()['menuShortcuts']
            item=shortcuts.get(str(int(sys.argv[2])))
            if item:menu_command(item['path'],item['app'])
        except FileNotFoundError:pass
        except Exception as exc:print(str(exc),file=sys.stderr);sys.exit(1)
    elif len(sys.argv)>2 and sys.argv[1]=='menu':
        try:menu_command(sys.argv[2])
        except Exception as exc:print(str(exc),file=sys.stderr);sys.exit(1)
    else:
        try:print(json.dumps(handle(json.loads(sys.stdin.readline()))))
        except Exception as exc:print(json.dumps(dict(error=str(exc))))
