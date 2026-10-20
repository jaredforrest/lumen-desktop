"""Provider-scoped file action capabilities; dispatch identity is not handler identity."""
import contextlib
import mimetypes
import os
from pathlib import Path
import secrets
import re
import signal
import stat
import subprocess
import threading
import time


class ResourceUnavailable(ValueError):
    def __init__(self, reason, message):
        super().__init__(message);self.reason=reason


def unavailable(exc):
    if isinstance(exc,ResourceUnavailable):return dict(reason=exc.reason,error=str(exc))
    if isinstance(exc,PermissionError):return dict(reason='permission_denied',error='Permission to read the selected file was denied')
    if isinstance(exc,(FileNotFoundError,NotADirectoryError)):return dict(reason='source_changed',error='The selected file is no longer available')
    if isinstance(exc,subprocess.TimeoutExpired):return dict(reason='provider_timeout',error='The provider timed out')
    return dict(reason='provider_failure',error='The provider could not complete this request')


def facts(path):
    info=Path(path).stat(follow_symlinks=False)
    return (info.st_dev,info.st_ino,info.st_mode,info.st_size,info.st_mtime_ns,info.st_ctime_ns)


def wire_facts(value):return [str(number) for number in value]


def parse_facts(value):
    if not isinstance(value,list) or len(value)!=6 or any(not isinstance(number,str) or not re.fullmatch(r'-?(0|[1-9][0-9]{0,19})',number) for number in value):raise ResourceUnavailable('invalid_request','Invalid captured file version')
    parsed=tuple(int(number) for number in value)
    if wire_facts(parsed)!=value or any(not -(2**63)<=number<2**64 for number in parsed):raise ResourceUnavailable('invalid_request','Invalid captured file version')
    return parsed


def application_facts(value):
    path=Path(value)
    try:resolved=path.resolve(strict=True);target=facts(resolved)
    except (OSError,RuntimeError) as exc:raise ResourceUnavailable('application_unavailable','This application registration is unavailable') from exc
    if not stat.S_ISREG(target[2]):raise ResourceUnavailable('application_unavailable','This application registration is unavailable')
    return (facts(path),str(resolved),target)


class CapturedResourceActions:
    def __init__(self):
        self.generation=secrets.token_hex(24);self.entries={};self.lock=threading.RLock()

    def capture(self,value,expected=None,resource=None):
        path=Path(value)
        if not path.is_absolute() or '\0' in str(path):raise ResourceUnavailable('invalid_request','An absolute local file is required')
        current=facts(path)
        if stat.S_ISLNK(current[2]) or not (stat.S_ISREG(current[2]) or stat.S_ISDIR(current[2])):raise ResourceUnavailable('unsupported','This file type does not support captured actions')
        if expected is not None and tuple(expected)!=current:raise ResourceUnavailable('source_changed','The selected file changed')
        with self.lock:
            if len(self.entries)>=10000:raise ResourceUnavailable('policy_limit','Too many retained action capabilities; refresh this provider')
            key=secrets.token_hex(24);self.entries[key]=dict(path=path,facts=current,resource=Path(resource) if resource else None)
        return key

    def bind_resource(self,key,resource,token=None):
        with self.lock:
            entry=self.selected(key);entry["resource"]=Path(resource);entry["resourceToken"]=token

    def retire(self,keys):
        with self.lock:
            for key in keys:self.entries.pop(key,None)

    def selected(self,key):
        with self.lock:entry=self.entries.get(key)
        if entry is None:raise ResourceUnavailable('source_changed','This captured selection is no longer available')
        try:current=facts(entry['path'])
        except PermissionError as exc:raise ResourceUnavailable('permission_denied','Permission to inspect the selected file was denied') from exc
        except OSError as exc:raise ResourceUnavailable('source_changed','The selected file is no longer available') from exc
        if current!=entry['facts']:raise ResourceUnavailable('source_changed','The selected file changed')
        return entry

    def applications(self,key):
        from gi.repository import Gio
        entry=self.selected(key);mime=mimetypes.guess_type(entry['path'].name)[0] or 'application/octet-stream'
        apps=[app for app in Gio.AppInfo.get_all_for_type(mime) if app.should_show() and app.get_id()]
        default=Gio.AppInfo.get_default_for_type(mime,False)
        self.selected(key)
        choices={}
        for app in apps:
            filename=app.get_filename()
            if filename:
                try:choices[app.get_id()]=(filename,application_facts(filename))
                except OSError:continue
        generation=secrets.token_hex(24)
        with self.lock:entry['choices']=(generation,choices)
        return dict(applications=[dict(id=app.get_id(),name=app.get_display_name(),default=bool(default and default.get_id()==app.get_id())) for app in apps if app.get_id() in choices],choice=generation)

    def dispatch(self,key,action,application=None,runner=subprocess.run,choice=None):
        entry=self.selected(key)
        if action=='open':command=['xdg-open',entry['path'].as_uri()]
        elif action in ('open-with','set-default'):
            from gi.repository import Gio
            mime=mimetypes.guess_type(entry['path'].name)[0] or 'application/octet-stream'
            captured=entry.get('choices')
            if not captured or captured[0]!=choice or application not in captured[1]:raise ResourceUnavailable('application_unavailable','This application chooser is no longer current')
            filename,expected=captured[1][application]
            try:unchanged=application_facts(filename)==expected
            except OSError:unchanged=False
            if not unchanged:raise ResourceUnavailable('application_unavailable','The selected application changed')
            app=next((app for app in Gio.AppInfo.get_all_for_type(mime) if app.should_show() and app.get_id()==application and app.get_filename()==filename),None)
            if app is None:raise ResourceUnavailable('application_unavailable','This application is no longer available')
            self.selected(key)
            if application_facts(filename)!=expected:raise ResourceUnavailable('application_unavailable','The selected application changed')
            if action=='set-default':
                if not app.set_as_default_for_type(mime):raise ResourceUnavailable('provider_failure','The default application could not be changed')
                return dict(ok=True)
            filename=app.get_filename()
            if not filename:raise ResourceUnavailable('application_unavailable','This application cannot open files')
            command=['gio','launch',filename,entry['path'].as_uri()]
        else:raise ResourceUnavailable('invalid_request','Unsupported captured action')
        if os.environ.get('LUMEN_LAUNCH_SCOPES')=='1':command=['lumen-launch-app','--',*command]
        self.selected(key)
        if action=='open-with' and application_facts(filename)!=expected:raise ResourceUnavailable('application_unavailable','The selected application changed')
        try:runner(command,check=True,timeout=12,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        except FileNotFoundError as exc:raise ResourceUnavailable('provider_failure','The application launcher is unavailable') from exc
        except subprocess.TimeoutExpired as exc:raise ResourceUnavailable('dispatch_unknown','The launcher did not acknowledge Open; check whether the application opened before trying again') from exc
        except subprocess.CalledProcessError as exc:raise ResourceUnavailable('dispatch_failed','The application launcher reported a failure') from exc
        return dict(ok=True,dispatched=True)

    def annotation_info(self,key):
        from image_markup import describe
        return describe(self.selected(key),lambda:self.selected(key))

    def annotate(self,key,parameters,cancel=None):
        from image_markup import save_copy
        entry=self.selected(key)
        return save_copy(entry,parameters,lambda:self.selected(key),cancel)

    def content(self,key,action,parameters,runner=None,cancel=None):
        entry=self.selected(key);resource=entry['resource']
        if action not in ('ocr','cutout') or resource is None:raise ResourceUnavailable('unsupported','This preview does not support that content action')
        # The source is retained by the caller for this entire synchronous call.
        if not stat.S_ISREG(resource.stat(follow_symlinks=False).st_mode):raise ResourceUnavailable('source_changed','The retained preview is unavailable')
        request=dict(action=action,path=str(resource))
        if action=='ocr':
            request['page']=parameters.get('page',1)
            if 'seconds' in parameters:request['seconds']=parameters['seconds']
        result=runner(request) if runner else run_content(request,cancel)
        self.selected(key)
        if result.get('error'):result.setdefault('reason','provider_failure')
        if action=='cutout' and result.get('subject'):result['subjectFacts']=wire_facts(facts(result['subject']))
        return result


def run_content(request,cancel=None):
    """Bound and reap only the helper session created here, including on cancellation."""
    import json,sys
    command=[sys.executable,str(Path(__file__).with_name('context_tools.py'))]
    child=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True)
    try:
        deadline=time.monotonic()+75;payload=json.dumps(request)+'\n'
        while True:
            if cancel is not None and cancel.is_set():raise ResourceUnavailable('canceled','Content action canceled')
            remaining=deadline-time.monotonic()
            if remaining<=0:raise subprocess.TimeoutExpired(command,75)
            try:output,_=child.communicate(payload,timeout=min(.1,remaining));break
            except subprocess.TimeoutExpired:payload=None
        if child.returncode:raise ResourceUnavailable('provider_failure','The content provider failed')
        result=json.loads(output)
        if not isinstance(result,dict):raise ResourceUnavailable('provider_failure','The content provider returned an invalid result')
        return result
    except subprocess.TimeoutExpired as exc:raise ResourceUnavailable('provider_timeout','The content action timed out') from exc
    finally:
        # start_new_session made this exact owned child the session/group leader.
        # No caller-supplied PID or host process group is accepted.
        if child.returncode is None:
            # Unreaped child PID cannot have been reused. Once communicate has
            # reaped it, never signal a numeric group that could be recycled.
            with contextlib.suppress(ProcessLookupError):os.killpg(child.pid,signal.SIGKILL)
        child.wait()
        for stream in (child.stdin,child.stdout,child.stderr):
            if stream:stream.close()
